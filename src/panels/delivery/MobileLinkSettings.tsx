// The mobile link's settings: the relay, pairing a phone by QR, and the
// paired phones (docs/contracts/mobile-link.md).

import { useEffect, useState } from "react";
import {
  cancelMobilePairing,
  confirmMobilePairing,
  getMobileLinkStatus,
  onMobileLinkChanged,
  revokeMobileDevice,
  setMobileLinkRelay,
  startMobilePairing,
  type MobileDevice,
  type MobileLinkStatus,
} from "../../delivery/mobileLink";
import { ActionButton, ActionGroup } from "./ActionButton";

const time = (ms: number) =>
  new Date(ms).toLocaleTimeString("es", { hour: "2-digit", minute: "2-digit" });

function connection(device: MobileDevice): string {
  if (device.online) return "En línea";
  if (device.relay) return "Esperando al móvil";
  return device.error ?? "Sin conexión con el relay";
}

export function MobileLinkSettings() {
  const [status, setStatus] = useState<MobileLinkStatus | null>(null);
  const [relay, setRelay] = useState("");

  useEffect(() => {
    let live = true;
    const read = () =>
      getMobileLinkStatus()
        .then((next) => live && setStatus(next))
        .catch(() => {});
    getMobileLinkStatus()
      .then((next) => {
        if (!live) return;
        setStatus(next);
        setRelay(next.relay);
      })
      .catch(() => {});
    const unlisten = onMobileLinkChanged(() => void read());
    return () => {
      live = false;
      void unlisten.then((stop) => stop());
    };
  }, []);

  if (!status) return null;
  const pairing = status.pairing;
  return (
    <>
      <h3 className="delivery-dialog__subtitle">Móvil</h3>
      <p className="delivery-settings__intro">
        Sigue Delivery y responde decisiones y aprobaciones desde el móvil. Tinto y el móvil hablan
        a través de un relay que solo reenvía mensajes cifrados de extremo a extremo: no puede
        leerlos. Sin relay, Tinto no se conecta a nada.
      </p>
      <label className="delivery-field">
        <span className="delivery-field__label">Relay</span>
        <span className="delivery-field-row">
          <input
            value={relay}
            placeholder="wss://relay.ejemplo.com"
            spellCheck={false}
            onChange={(event) => setRelay(event.target.value)}
          />
          <ActionButton
            className="file-op-modal__button"
            disabled={relay.trim() === status.relay}
            onAction={() => setMobileLinkRelay(relay).then(setStatus)}
          >
            Guardar
          </ActionButton>
        </span>
        <small className="delivery-field__hint">
          wss://servidor[:puerto]. Vacío: apagado. Cambiarlo desconecta los móviles hasta que el
          relay nuevo responda.
        </small>
      </label>
      {status.relay && !pairing && (
        <div className="delivery-actions">
          <ActionButton
            className="delivery-button--primary"
            onAction={() => startMobilePairing().then(setStatus)}
          >
            Vincular móvil
          </ActionButton>
        </div>
      )}
      {pairing && (
        <div className="delivery-mobile-pairing" aria-label="Vincular móvil">
          <img
            src={`data:image/svg+xml;utf8,${encodeURIComponent(pairing.qr_svg)}`}
            alt="Código QR para vincular el móvil"
            width={240}
            height={240}
          />
          <div>
            <p>
              Escanéalo con la app Tinto Delivery, o copia el enlace y pégalo en ella. El código
              lleva la clave del móvil: no lo compartas. Caduca a las {time(pairing.expires_at_ms)}.
            </p>
            {pairing.error && <p className="delivery-tone--danger">{pairing.error}</p>}
            <ActionGroup>
              {pairing.device ? (
                <>
                  <p role="status">
                    <strong>{pairing.device}</strong> quiere vincularse. Confírmalo solo si es tu
                    móvil.
                  </p>
                  <div className="delivery-actions">
                    <ActionButton
                      className="delivery-button--primary"
                      onAction={() => confirmMobilePairing().then(setStatus)}
                    >
                      Confirmar
                    </ActionButton>
                    <ActionButton onAction={() => cancelMobilePairing().then(setStatus)}>
                      Cancelar
                    </ActionButton>
                  </div>
                </>
              ) : (
                <>
                  <p className="delivery-muted" role="status">
                    {pairing.relay ? "Esperando al móvil…" : "Conectando con el relay…"}
                  </p>
                  <div className="delivery-actions">
                    <ActionButton onAction={() => navigator.clipboard.writeText(pairing.link)}>
                      Copiar enlace
                    </ActionButton>
                    <ActionButton onAction={() => cancelMobilePairing().then(setStatus)}>
                      Cancelar
                    </ActionButton>
                  </div>
                </>
              )}
            </ActionGroup>
          </div>
        </div>
      )}
      {status.devices.length > 0 && (
        <ul className="delivery-mobile-devices" aria-label="Móviles vinculados">
          {status.devices.map((device) => (
            <li key={device.id}>
              <span>
                {device.name}
                <small className="delivery-muted">
                  {" "}
                  · {connection(device)} · vinculado el{" "}
                  {new Date(device.paired_at_ms).toLocaleDateString("es")}
                </small>
              </span>
              <ActionButton onAction={() => revokeMobileDevice(device.id).then(setStatus)}>
                Revocar
              </ActionButton>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
