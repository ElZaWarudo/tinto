//! The mobile link's encryption and freshness (docs/contracts/mobile-link.md
//! §3): each frame is ChaCha20-Poly1305 with a random nonce, and each
//! plaintext names its sender's session, the receiver's session and a
//! sequence number, so a frame recorded earlier is refused.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{json, Map, Value};

pub const AAD: &[u8] = b"tinto-link/1";
pub const VERSION: u64 = 1;
const TAG_LEN: usize = 16;
/// The relay's frame limit.
pub const MAX_FRAME: usize = 256 * 1024;
pub const MAX_PLAINTEXT: usize = MAX_FRAME - NONCE_LEN - TAG_LEN;

pub fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("the system's random source failed");
    bytes
}

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(text: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(text).ok()
}

/// A pairing's key.
pub struct Key(LessSafeKey);

impl Key {
    pub fn from_b64(key: &str) -> Option<Self> {
        let bytes = unb64(key).filter(|bytes| bytes.len() == 32)?;
        Some(Self(LessSafeKey::new(
            UnboundKey::new(&CHACHA20_POLY1305, &bytes).ok()?,
        )))
    }

    pub fn seal(&self, plaintext: &[u8]) -> Vec<u8> {
        self.seal_with(random::<NONCE_LEN>(), plaintext)
    }

    fn seal_with(&self, nonce: [u8; NONCE_LEN], plaintext: &[u8]) -> Vec<u8> {
        let mut frame = nonce.to_vec();
        let mut body = plaintext.to_vec();
        self.0
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(AAD),
                &mut body,
            )
            .expect("a frame within limits always seals");
        frame.extend_from_slice(&body);
        frame
    }

    /// The plaintext, or `None` for a frame this key did not seal.
    pub fn open(&self, frame: &[u8]) -> Option<Vec<u8>> {
        if frame.len() < NONCE_LEN + TAG_LEN {
            return None;
        }
        let (nonce, sealed) = frame.split_at(NONCE_LEN);
        let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
        let mut body = sealed.to_vec();
        let plain = self
            .0
            .open_in_place(nonce, Aad::from(AAD), &mut body)
            .ok()?;
        Some(plain.to_vec())
    }
}

/// One side of a connection: its own session and what it last accepted
/// from the other's.
pub struct Session {
    role: &'static str,
    pub sid: String,
    seq: u64,
    peer: Option<(String, u64)>,
}

/// A message that passed the freshness rules.
#[derive(Debug, PartialEq)]
pub struct Incoming {
    pub kind: String,
    pub body: Map<String, Value>,
    /// A hello from a peer that does not know this session yet: answer it
    /// with a hello.
    pub answer_hello: bool,
}

impl Session {
    pub fn new(role: &'static str) -> Self {
        Self {
            role,
            sid: b64(&random::<16>()),
            seq: 0,
            peer: None,
        }
    }

    /// The other side's session is known: messages other than hello reach it.
    pub fn knows_peer(&self) -> bool {
        self.peer.is_some()
    }

    /// The plaintext of a message of `kind` with `body`'s fields.
    pub fn wrap(&mut self, kind: &str, body: Value) -> Vec<u8> {
        self.seq += 1;
        let mut message = match body {
            Value::Object(fields) => fields,
            _ => Map::new(),
        };
        let peer = self.peer.as_ref().map_or("", |(sid, _)| sid.as_str());
        for (field, value) in [
            ("v", json!(VERSION)),
            ("from", json!(self.role)),
            ("type", json!(kind)),
            ("sid", json!(self.sid)),
            ("peer", json!(peer)),
            ("seq", json!(self.seq)),
        ] {
            message.insert(field.into(), value);
        }
        serde_json::to_vec(&message).expect("a JSON object always serializes")
    }

    /// The message, or `None` when it is stale, replayed, reflected or not ours.
    pub fn accept(&mut self, plaintext: &[u8]) -> Option<Incoming> {
        let Ok(Value::Object(mut body)) = serde_json::from_slice::<Value>(plaintext) else {
            return None;
        };
        let text = |body: &Map<String, Value>, field: &str| {
            body.get(field).and_then(Value::as_str).map(str::to_string)
        };
        let from = text(&body, "from")?;
        if body.get("v").and_then(Value::as_u64) != Some(VERSION) || from == self.role {
            return None;
        }
        let kind = text(&body, "type")?;
        let sid = text(&body, "sid").filter(|sid| !sid.is_empty())?;
        let peer = text(&body, "peer")?;
        let seq = body.get("seq").and_then(Value::as_u64)?;
        let fresh = |last: &Option<(String, u64)>| {
            last.as_ref()
                .is_none_or(|(known, last)| *known != sid || seq > *last)
        };
        let answer_hello = if kind == "hello" {
            if !fresh(&self.peer) {
                return None;
            }
            peer != self.sid
        } else {
            let current = self.peer.as_ref().is_some_and(|(known, _)| *known == sid);
            if !current || peer != self.sid || !fresh(&self.peer) {
                return None;
            }
            false
        };
        self.peer = Some((sid, seq));
        for field in ["v", "from", "type", "sid", "peer", "seq"] {
            body.remove(field);
        }
        Some(Incoming {
            kind,
            body,
            answer_hello,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// docs/contracts/mobile-link.md §7, computed with Node's OpenSSL.
    const VECTOR_KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
    const VECTOR_PLAINTEXT: &str = r#"{"v":1,"from":"phone","type":"hello","sid":"c2lkLXBob25lLTAwMDAwMQ","peer":"","seq":1,"device":"Prueba"}"#;
    const VECTOR_FRAME: &str = "404142434445464748494a4b83760aa3487bc2233ebd996d99d54fbddca30d7d66d0631b877cb779d005520f9cdff6520a69119f6e4d1565096682f5ad0b2ea69c44b84397964e4c452f721686610b717163824e302c57abe1020719b82ab214cd91c20ad807c54e697eb856ce4afb3f4ee5370a59138dded052b70cb1ac0871a1bda69c";

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn the_shared_vector_seals_and_opens_as_the_contract_says() {
        let key = Key::from_b64(VECTOR_KEY).unwrap();
        let nonce: [u8; 12] = std::array::from_fn(|i| 0x40 + i as u8);
        let frame = key.seal_with(nonce, VECTOR_PLAINTEXT.as_bytes());
        assert_eq!(hex(&frame), VECTOR_FRAME);
        assert_eq!(key.open(&frame).unwrap(), VECTOR_PLAINTEXT.as_bytes());
    }

    #[test]
    fn a_frame_round_trips_and_any_change_or_other_key_is_refused() {
        let key = Key::from_b64(&b64(&random::<32>())).unwrap();
        let frame = key.seal(b"hola");
        assert_eq!(key.open(&frame).unwrap(), b"hola");
        let mut tampered = frame.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert_eq!(key.open(&tampered), None);
        let other = Key::from_b64(&b64(&random::<32>())).unwrap();
        assert_eq!(other.open(&frame), None);
        assert!(Key::from_b64("short").is_none());
    }

    #[test]
    fn sessions_meet_by_hello_and_refuse_replays_reflections_and_old_sessions() {
        let mut desktop = Session::new("desktop");
        let mut phone = Session::new("phone");

        // The phone says hello first; the desktop answers, the phone does not.
        let hello = phone.wrap("hello", json!({ "device": "Pixel" }));
        let got = desktop.accept(&hello).unwrap();
        assert!(got.answer_hello);
        assert_eq!(got.body["device"], "Pixel");
        let answer = desktop.wrap("hello", json!({ "status": "paired" }));
        assert!(!phone.accept(&answer).unwrap().answer_hello);

        let action = phone.wrap("action", json!({ "id": "1" }));
        assert_eq!(desktop.accept(&action).unwrap().kind, "action");
        // The same frame again, and the desktop's own frame sent back.
        assert_eq!(desktop.accept(&action), None);
        let own = desktop.wrap("snapshot", json!({}));
        assert_eq!(desktop.accept(&own), None);

        // After a reconnection, what the phone sent before is refused.
        let mut desktop = Session::new("desktop");
        desktop.accept(&phone.wrap("hello", json!({}))).unwrap();
        let old_action = phone.wrap("action", json!({ "id": "2" }));
        assert_eq!(desktop.accept(&old_action), None);
        // Once the phone hears the new session, it is in.
        phone.accept(&desktop.wrap("hello", json!({}))).unwrap();
        let action = phone.wrap("action", json!({ "id": "3" }));
        assert_eq!(desktop.accept(&action).unwrap().body["id"], "3");
    }
}
