//! The mobile link: Delivery followed and answered from a phone through a
//! blind relay (docs/contracts/mobile-link.md). Off until the user sets a
//! relay and pairs a phone; each paired phone has its own room and key, and
//! Tinto keeps one connection to the relay per phone.

pub mod actions;
mod connection;
pub mod crypto;
pub mod snapshot;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::{mpsc, watch};

use crate::agent_console::commands::CommandError;
use crate::delivery::service::DeliveryService;
use crate::delivery::{now_ms, DeliveryError};

pub const EVENT_MOBILE_LINK_CHANGED: &str = "tinto://mobile-link-changed";
/// How long a pairing waits for its phone and the user's confirmation.
const PAIRING_MS: u64 = 10 * 60 * 1000;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Record {
    #[serde(default)]
    relay: String,
    #[serde(default)]
    devices: Vec<Device>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Device {
    room: String,
    key: String,
    name: String,
    paired_at_ms: u64,
}

struct Pending {
    room: String,
    key: String,
    expires_at_ms: u64,
    /// The name the phone gave in its hello.
    device: Option<String>,
}

/// A room's connection as the settings show it.
#[derive(Default)]
struct Live {
    relay: bool,
    phone: bool,
    error: Option<String>,
}

enum Control {
    /// The user confirmed the phone: it gets its snapshot.
    Paired,
    /// The room is no longer wanted; `revoked` tells the phone first.
    Stop { revoked: bool },
}

#[derive(Default)]
struct LinkState {
    record: Record,
    pending: Option<Pending>,
    live: HashMap<String, Live>,
    /// Running connections, by room: the relay they use and their control.
    conns: HashMap<String, (String, mpsc::UnboundedSender<Control>)>,
}

type OnChange = Arc<dyn Fn() + Send + Sync>;

struct Inner {
    service: DeliveryService,
    state: Mutex<LinkState>,
    /// Counts Delivery's changes; every connection pushes a snapshot on them.
    changes: watch::Sender<u64>,
    on_change: Mutex<Option<OnChange>>,
}

#[derive(Clone)]
pub struct MobileLink {
    inner: Arc<Inner>,
}

#[derive(Debug, Serialize)]
pub struct MobileLinkStatus {
    /// Empty: the link is off.
    pub relay: String,
    pub devices: Vec<DeviceStatus>,
    pub pairing: Option<PairingStatus>,
}

#[derive(Debug, Serialize)]
pub struct DeviceStatus {
    pub id: String,
    pub name: String,
    pub paired_at_ms: u64,
    /// Connected to the relay.
    pub relay: bool,
    /// The phone is there.
    pub online: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PairingStatus {
    /// What the QR holds, with the key: shown only while pairing.
    pub link: String,
    pub qr_svg: String,
    pub expires_at_ms: u64,
    pub relay: bool,
    pub error: Option<String>,
    /// The phone's name, once it said hello: the user confirms it.
    pub device: Option<String>,
}

fn invalid(message: &str) -> DeliveryError {
    DeliveryError::new("mobile_link_invalid", message)
}

/// `wss://host[:port]` (or `ws://` for a local test), without a path.
fn relay_url(input: &str) -> Result<String, DeliveryError> {
    let url = input.trim().trim_end_matches('/');
    if url.is_empty() {
        return Ok(String::new());
    }
    let host = url
        .strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))
        .ok_or_else(|| invalid("El relay empieza por wss:// (o ws:// para pruebas)."))?;
    if host.is_empty() || host.contains(['/', '?', '#', '@', ' ']) {
        return Err(invalid(
            "El relay es solo wss://servidor[:puerto], sin ruta.",
        ));
    }
    Ok(url.to_string())
}

/// The pairing link of docs/contracts/mobile-link.md §1; the key goes only
/// in the fragment.
fn pairing_link(relay: &str, room: &str, key: &str) -> String {
    match relay.strip_prefix("ws://") {
        Some(host) => format!("tinto-link://{host}/{room}?tls=0#{key}"),
        None => format!(
            "tinto-link://{}/{room}#{key}",
            relay.trim_start_matches("wss://")
        ),
    }
}

fn qr_svg(text: &str) -> String {
    use qrcode::render::svg;
    qrcode::QrCode::new(text.as_bytes())
        .map(|code| {
            code.render::<svg::Color>()
                .min_dimensions(240, 240)
                .dark_color(svg::Color("#000000"))
                .light_color(svg::Color("#ffffff"))
                .build()
        })
        .unwrap_or_default()
}

impl MobileLink {
    /// Loads the record and connects the paired phones, if a relay is set.
    pub fn start(service: DeliveryService) -> Self {
        let record = service
            .mobile_link_record::<Record>()
            .unwrap_or_else(|error| {
                eprintln!("tinto: no se pudo leer el enlace móvil: {error}");
                None
            })
            .unwrap_or_default();
        let link = Self {
            inner: Arc::new(Inner {
                service,
                state: Mutex::new(LinkState {
                    record,
                    ..LinkState::default()
                }),
                changes: watch::channel(0).0,
                on_change: Mutex::new(None),
            }),
        };
        link.reconcile();
        link
    }

    pub fn set_on_change(&self, on_change: OnChange) {
        if let Ok(mut slot) = self.inner.on_change.lock() {
            *slot = Some(on_change);
        }
    }

    /// Delivery changed: connected phones get a new snapshot.
    pub fn delivery_changed(&self) {
        self.inner.changes.send_modify(|count| *count += 1);
    }

    fn changed(&self) {
        let on_change = self.inner.on_change.lock().ok().and_then(|s| s.clone());
        if let Some(on_change) = on_change {
            on_change();
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, LinkState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn save(&self, state: &LinkState) -> Result<(), DeliveryError> {
        self.inner.service.set_mobile_link_record(&state.record)
    }

    fn is_paired(&self, room: &str) -> bool {
        self.state().record.devices.iter().any(|d| d.room == room)
    }

    fn set_live(&self, room: &str, update: impl FnOnce(&mut Live)) {
        update(self.state().live.entry(room.to_string()).or_default());
        self.changed();
    }

    /// The phone named itself in a pending pairing.
    fn pairing_device(&self, room: &str, name: String) {
        if let Some(pending) = self.state().pending.as_mut().filter(|p| p.room == room) {
            pending.device = Some(name);
        }
        self.changed();
    }

    /// Starts the connections that should run and stops the others.
    fn reconcile(&self) {
        let mut state = self.state();
        let relay = state.record.relay.clone();
        let mut wanted: Vec<(String, String)> = Vec::new();
        if !relay.is_empty() {
            wanted.extend(
                state
                    .record
                    .devices
                    .iter()
                    .map(|d| (d.room.clone(), d.key.clone())),
            );
            wanted.extend(
                state
                    .pending
                    .iter()
                    .map(|p| (p.room.clone(), p.key.clone())),
            );
        }
        let stale: Vec<String> = state
            .conns
            .iter()
            .filter(|(room, (used, _))| *used != relay || !wanted.iter().any(|(w, _)| w == *room))
            .map(|(room, _)| room.clone())
            .collect();
        for room in stale {
            if let Some((_, control)) = state.conns.remove(&room) {
                let _ = control.send(Control::Stop { revoked: false });
            }
            state.live.remove(&room);
        }
        for (room, key) in wanted {
            if state.conns.contains_key(&room) {
                continue;
            }
            let Some(key) = crypto::Key::from_b64(&key) else {
                continue;
            };
            let (control, controls) = mpsc::unbounded_channel();
            state.conns.insert(room.clone(), (relay.clone(), control));
            tauri::async_runtime::spawn(connection::run(
                self.clone(),
                relay.clone(),
                room,
                Arc::new(key),
                controls,
            ));
        }
    }

    pub fn status(&self) -> MobileLinkStatus {
        let state = self.state();
        let live = |room: &str| state.live.get(room);
        MobileLinkStatus {
            relay: state.record.relay.clone(),
            devices: state
                .record
                .devices
                .iter()
                .map(|device| DeviceStatus {
                    id: device.room.clone(),
                    name: device.name.clone(),
                    paired_at_ms: device.paired_at_ms,
                    relay: live(&device.room).is_some_and(|l| l.relay),
                    online: live(&device.room).is_some_and(|l| l.phone),
                    error: live(&device.room).and_then(|l| l.error.clone()),
                })
                .collect(),
            pairing: state.pending.as_ref().map(|pending| {
                let link = pairing_link(&state.record.relay, &pending.room, &pending.key);
                PairingStatus {
                    qr_svg: qr_svg(&link),
                    link,
                    expires_at_ms: pending.expires_at_ms,
                    relay: live(&pending.room).is_some_and(|l| l.relay),
                    error: live(&pending.room).and_then(|l| l.error.clone()),
                    device: pending.device.clone(),
                }
            }),
        }
    }

    pub fn set_relay(&self, relay: &str) -> Result<(), DeliveryError> {
        let relay = relay_url(relay)?;
        {
            let mut state = self.state();
            state.record.relay = relay;
            state.pending = None;
            self.save(&state)?;
        }
        self.reconcile();
        self.changed();
        Ok(())
    }

    pub fn start_pairing(&self) -> Result<(), DeliveryError> {
        let expires_at_ms = now_ms() + PAIRING_MS;
        let room = {
            let mut state = self.state();
            if state.record.relay.is_empty() {
                return Err(invalid("Primero indica el relay."));
            }
            let room = crypto::b64(&crypto::random::<16>());
            state.pending = Some(Pending {
                room: room.clone(),
                key: crypto::b64(&crypto::random::<32>()),
                expires_at_ms,
                device: None,
            });
            room
        };
        self.reconcile();
        self.changed();
        let link = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(PAIRING_MS)).await;
            let expired = {
                let mut state = link.state();
                let expired = state.pending.as_ref().is_some_and(|p| p.room == room);
                if expired {
                    state.pending = None;
                }
                expired
            };
            if expired {
                link.reconcile();
                link.changed();
            }
        });
        Ok(())
    }

    /// The user confirms the phone that said hello: it is paired from now on.
    pub fn confirm_pairing(&self) -> Result<(), DeliveryError> {
        {
            let mut state = self.state();
            let Some(pending) = state.pending.take_if(|p| p.device.is_some()) else {
                return Err(invalid("Ningún móvil espera tu confirmación."));
            };
            state.record.devices.push(Device {
                room: pending.room.clone(),
                key: pending.key,
                name: pending.device.unwrap_or_default(),
                paired_at_ms: now_ms(),
            });
            if let Err(error) = self.save(&state) {
                state.record.devices.pop();
                return Err(error);
            }
            if let Some((_, control)) = state.conns.get(&pending.room) {
                let _ = control.send(Control::Paired);
            }
        }
        self.changed();
        Ok(())
    }

    pub fn cancel_pairing(&self) {
        self.state().pending = None;
        self.reconcile();
        self.changed();
    }

    /// Forgets the phone and its key; it is told if it is there.
    pub fn revoke(&self, id: &str) -> Result<(), DeliveryError> {
        {
            let mut state = self.state();
            let before = state.record.devices.clone();
            state.record.devices.retain(|device| device.room != id);
            if state.record.devices.len() == before.len() {
                return Err(DeliveryError::not_found("el móvil", id));
            }
            if let Err(error) = self.save(&state) {
                state.record.devices = before;
                return Err(error);
            }
            if let Some((_, control)) = state.conns.remove(id) {
                let _ = control.send(Control::Stop { revoked: true });
            }
            state.live.remove(id);
        }
        self.changed();
        Ok(())
    }
}

#[tauri::command]
pub fn mobile_link_status(link: State<'_, MobileLink>) -> MobileLinkStatus {
    link.status()
}

#[tauri::command]
pub fn mobile_link_set_relay(
    link: State<'_, MobileLink>,
    relay: String,
) -> Result<MobileLinkStatus, CommandError> {
    link.set_relay(&relay)?;
    Ok(link.status())
}

#[tauri::command]
pub fn mobile_link_start_pairing(
    link: State<'_, MobileLink>,
) -> Result<MobileLinkStatus, CommandError> {
    link.start_pairing()?;
    Ok(link.status())
}

#[tauri::command]
pub fn mobile_link_confirm_pairing(
    link: State<'_, MobileLink>,
) -> Result<MobileLinkStatus, CommandError> {
    link.confirm_pairing()?;
    Ok(link.status())
}

#[tauri::command]
pub fn mobile_link_cancel_pairing(link: State<'_, MobileLink>) -> MobileLinkStatus {
    link.cancel_pairing();
    link.status()
}

#[tauri::command]
pub fn mobile_link_revoke(
    link: State<'_, MobileLink>,
    device_id: String,
) -> Result<MobileLinkStatus, CommandError> {
    link.revoke(&device_id)?;
    Ok(link.status())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_relay_is_a_websocket_origin_and_the_key_stays_in_the_fragment() {
        assert_eq!(
            relay_url(" wss://relay.example.com/ ").unwrap(),
            "wss://relay.example.com"
        );
        assert_eq!(relay_url("").unwrap(), "");
        for bad in [
            "https://relay",
            "wss://relay/path",
            "wss://user@relay",
            "wss://",
        ] {
            assert!(relay_url(bad).is_err(), "{bad}");
        }
        assert_eq!(
            pairing_link("wss://relay.example.com:8443", "ROOM", "KEY"),
            "tinto-link://relay.example.com:8443/ROOM#KEY"
        );
        assert_eq!(
            pairing_link("ws://192.168.1.5:8787", "ROOM", "KEY"),
            "tinto-link://192.168.1.5:8787/ROOM?tls=0#KEY"
        );
    }
}
