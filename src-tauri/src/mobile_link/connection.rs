//! One room's connection to the relay: reconnects with backoff, says hello,
//! pushes a snapshot after Delivery changes (at most one a second) and runs
//! the phone's allowlisted actions.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::time::{sleep, sleep_until, timeout, Instant};
use tokio_tungstenite::tungstenite::Message;

use super::actions::{self, Action};
use super::crypto::{Incoming, Key, Session};
use super::{snapshot, Control, MobileLink};
use crate::delivery::now_ms;

const DEBOUNCE: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// The relay pings every 30 s: this long without a frame, it is gone.
const RELAY_SILENCE: Duration = Duration::from_secs(90);
/// Each side pings the other this often (§4) ...
const PING: Duration = Duration::from_secs(30);
/// ... so this long without a frame from the phone, it left.
const PHONE_SILENCE: Duration = Duration::from_secs(75);
const MAX_NAME: usize = 64;

/// What Tinto does about one message from the phone.
#[derive(Debug, PartialEq)]
pub(super) enum Reply {
    /// The phone named itself while pairing.
    Device(String),
    /// Tinto's hello, with whether the phone is paired.
    Hello {
        paired: bool,
    },
    Snapshot,
    Run {
        id: String,
        action: Action,
    },
    Error {
        id: String,
        message: String,
    },
}

fn short(text: &str) -> String {
    text.chars().take(MAX_NAME).collect()
}

/// A phone that is not paired yet only gets to name itself; nothing it asks
/// for runs.
pub(super) fn react(paired: bool, incoming: &Incoming) -> Vec<Reply> {
    let text = |field: &str| incoming.body.get(field).and_then(Value::as_str);
    match incoming.kind.as_str() {
        "hello" => {
            let mut replies = Vec::new();
            if !paired {
                replies.push(Reply::Device(short(text("device").unwrap_or("Móvil"))));
            }
            if incoming.answer_hello || !paired {
                replies.push(Reply::Hello { paired });
            }
            if paired {
                replies.push(Reply::Snapshot);
            }
            replies
        }
        "action" => {
            let id = short(text("id").unwrap_or(""));
            if !paired {
                return vec![Reply::Error {
                    id,
                    message: "Confirma este móvil en Tinto antes de usarlo.".into(),
                }];
            }
            let action = incoming.body.get("action").unwrap_or(&Value::Null);
            vec![match actions::parse(action) {
                Ok(action) => Reply::Run { id, action },
                Err(message) => Reply::Error { id, message },
            }]
        }
        _ => Vec::new(),
    }
}

enum End {
    Retry,
    Stop,
}

pub(super) async fn run(
    link: MobileLink,
    relay: String,
    room: String,
    key: Arc<Key>,
    mut control: mpsc::UnboundedReceiver<Control>,
) {
    let url = format!("{relay}/v1/rooms/{room}?role=desktop");
    let mut backoff = Duration::from_secs(1);
    loop {
        match timeout(
            Duration::from_secs(20),
            tokio_tungstenite::connect_async(url.as_str()),
        )
        .await
        {
            Ok(Ok((ws, _))) => {
                link.set_live(&room, |live| {
                    live.relay = true;
                    live.error = None;
                });
                backoff = Duration::from_secs(1);
                let end = session(&link, ws, &room, &key, &mut control).await;
                if let End::Stop = end {
                    return;
                }
                link.set_live(&room, |live| {
                    live.relay = false;
                    live.phone = false;
                });
            }
            Ok(Err(error)) => link.set_live(&room, |live| {
                live.error = Some(format!("No se pudo conectar con el relay: {error}"))
            }),
            Err(_) => link.set_live(&room, |live| {
                live.error = Some("El relay no responde.".into())
            }),
        }
        tokio::select! {
            _ = sleep(backoff) => {}
            command = control.recv() => {
                if matches!(command, Some(Control::Stop { .. }) | None) {
                    return;
                }
            }
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn snapshot_body(link: &MobileLink) -> Option<Value> {
    let service = link.inner.service.clone();
    let overview = tauri::async_runtime::spawn_blocking(move || service.overview())
        .await
        .ok()?
        .map_err(|error| eprintln!("tinto: el enlace móvil no pudo leer Delivery: {error}"))
        .ok()?;
    Some(snapshot::body(snapshot::build(&overview, now_ms())))
}

async fn session<S>(
    link: &MobileLink,
    ws: tokio_tungstenite::WebSocketStream<S>,
    room: &str,
    key: &Key,
    control: &mut mpsc::UnboundedReceiver<Control>,
) -> End
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut sink, mut stream) = ws.split();
    let mut session = Session::new("desktop");
    let mut changes = link.inner.changes.subscribe();
    let (done_tx, mut done) = mpsc::unbounded_channel::<(&'static str, Value)>();
    let mut push_at: Option<Instant> = None;
    let mut ping = tokio::time::interval(PING);
    let mut heard: Option<Instant> = None;

    macro_rules! send {
        ($kind:expr, $body:expr) => {{
            let frame = key.seal(&session.wrap($kind, $body));
            if sink.send(Message::Binary(frame.into())).await.is_err() {
                return End::Retry;
            }
        }};
    }
    macro_rules! send_snapshot {
        () => {{
            if let Some(body) = snapshot_body(link).await {
                send!("snapshot", body);
            }
        }};
    }

    let paired = link.is_paired(room);
    send!(
        "hello",
        json!({ "status": if paired { "paired" } else { "pending" } })
    );
    loop {
        let at = push_at;
        let push = async move {
            match at {
                Some(at) => sleep_until(at).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            frame = timeout(RELAY_SILENCE, stream.next()) => {
                let bytes = match frame {
                    Ok(Some(Ok(Message::Binary(bytes)))) => bytes,
                    Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Err(_))) | Ok(None) | Err(_) => {
                        return End::Retry;
                    }
                    Ok(Some(Ok(_))) => continue,
                };
                let Some(incoming) = key.open(&bytes).and_then(|plain| session.accept(&plain))
                else {
                    continue;
                };
                if heard.replace(Instant::now()).is_none() {
                    link.set_live(room, |live| live.phone = true);
                }
                let paired = link.is_paired(room);
                for reply in react(paired, &incoming) {
                    match reply {
                        Reply::Device(name) => link.pairing_device(room, name),
                        Reply::Hello { paired } => send!(
                            "hello",
                            json!({ "status": if paired { "paired" } else { "pending" } })
                        ),
                        Reply::Snapshot => send_snapshot!(),
                        Reply::Error { id, message } => {
                            send!("error", json!({ "id": id, "message": message }))
                        }
                        Reply::Run { id, action } => {
                            let service = link.inner.service.clone();
                            let done_tx = done_tx.clone();
                            tauri::async_runtime::spawn_blocking(move || {
                                let reply = match actions::run(&service, action) {
                                    Ok(()) => ("ack", json!({ "id": id })),
                                    Err(message) => {
                                        ("error", json!({ "id": id, "message": message }))
                                    }
                                };
                                let _ = done_tx.send(reply);
                            });
                        }
                    }
                }
            }
            _ = ping.tick() => {
                if heard.is_some_and(|at| at.elapsed() > PHONE_SILENCE) {
                    heard = None;
                    link.set_live(room, |live| live.phone = false);
                }
                if session.knows_peer() {
                    send!("ping", json!({}));
                }
            }
            _ = changes.changed() => {
                push_at.get_or_insert_with(|| Instant::now() + DEBOUNCE);
            }
            _ = push => {
                push_at = None;
                if session.knows_peer() && link.is_paired(room) {
                    send_snapshot!();
                }
            }
            Some((kind, body)) = done.recv() => send!(kind, body),
            command = control.recv() => match command {
                Some(Control::Paired) => {
                    send!("hello", json!({ "status": "paired" }));
                    send_snapshot!();
                }
                Some(Control::Stop { revoked }) => {
                    if revoked && session.knows_peer() {
                        send!("revoked", json!({}));
                    }
                    let _ = sink.close().await;
                    return End::Stop;
                }
                None => return End::Stop,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    fn incoming(kind: &str, body: Value, answer_hello: bool) -> Incoming {
        let Value::Object(body) = body else {
            panic!("an object")
        };
        Incoming {
            kind: kind.into(),
            body: body as Map<String, Value>,
            answer_hello,
        }
    }

    #[test]
    fn an_unpaired_phone_only_names_itself_and_a_paired_one_gets_its_snapshot() {
        let hello = incoming("hello", json!({ "device": "Pixel de Ana" }), true);
        assert_eq!(
            react(false, &hello),
            vec![
                Reply::Device("Pixel de Ana".into()),
                Reply::Hello { paired: false }
            ]
        );
        assert_eq!(
            react(true, &hello),
            vec![Reply::Hello { paired: true }, Reply::Snapshot]
        );

        let pause = incoming(
            "action",
            json!({ "id": "7", "action": { "action": "set_run_paused", "run_id": "r1", "paused": true } }),
            false,
        );
        assert!(matches!(&react(false, &pause)[..], [Reply::Error { id, .. }] if id == "7"));
        assert!(matches!(&react(true, &pause)[..], [Reply::Run { id, .. }] if id == "7"));

        let other = incoming(
            "action",
            json!({ "id": "8", "action": { "action": "read_file" } }),
            false,
        );
        assert!(matches!(&react(true, &other)[..], [Reply::Error { .. }]));
        assert!(react(true, &incoming("snapshot", json!({}), false)).is_empty());
    }
}
