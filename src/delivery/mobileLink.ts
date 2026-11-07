// Typed wrappers for the mobile link (src-tauri/src/mobile_link/mod.rs).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface MobileDevice {
  id: string;
  name: string;
  paired_at_ms: number;
  /** Connected to the relay. */
  relay: boolean;
  /** The phone is there. */
  online: boolean;
  error: string | null;
}

export interface MobilePairing {
  /** The QR's text, with the key. */
  link: string;
  qr_svg: string;
  expires_at_ms: number;
  relay: boolean;
  error: string | null;
  /** The phone's name, once it said hello. */
  device: string | null;
}

export interface MobileLinkStatus {
  /** Empty: the link is off. */
  relay: string;
  devices: MobileDevice[];
  pairing: MobilePairing | null;
}

export const EVENT_MOBILE_LINK_CHANGED = "tinto://mobile-link-changed";

export const getMobileLinkStatus = () => invoke<MobileLinkStatus>("mobile_link_status");

export const setMobileLinkRelay = (relay: string) =>
  invoke<MobileLinkStatus>("mobile_link_set_relay", { relay });

export const startMobilePairing = () => invoke<MobileLinkStatus>("mobile_link_start_pairing");

export const confirmMobilePairing = () => invoke<MobileLinkStatus>("mobile_link_confirm_pairing");

export const cancelMobilePairing = () => invoke<MobileLinkStatus>("mobile_link_cancel_pairing");

export const revokeMobileDevice = (deviceId: string) =>
  invoke<MobileLinkStatus>("mobile_link_revoke", { deviceId });

export function onMobileLinkChanged(callback: () => void): Promise<UnlistenFn> {
  try {
    return listen(EVENT_MOBILE_LINK_CHANGED, () => callback()).catch(() => () => {});
  } catch {
    return Promise.resolve(() => {});
  }
}
