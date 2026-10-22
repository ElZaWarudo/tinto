// "Reiniciar o actualizar": says what keeps running while Tinto is closed and
// what it resumes when it opens again, then restarts it or runs an installer.
// Nothing waits for the agents to finish.

import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { basename } from "../bus/store";
import { getRestartPlan, restartTinto, type RestartPlan } from "./restart";
import { useAccessibleDialog } from "./useAccessibleDialog";

export function RestartDialog({ onClose }: { onClose: () => void }) {
  const [plan, setPlan] = useState<RestartPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const dialogRef = useAccessibleDialog<HTMLDivElement>({ onClose });

  useEffect(() => {
    let active = true;
    getRestartPlan().then(
      (loaded) => active && setPlan(loaded),
      (failure) => active && setError(message(failure, "No se pudo leer lo que está en marcha.")),
    );
    return () => {
      active = false;
    };
  }, []);

  const restart = async (installer?: string) => {
    setBusy(true);
    setError(null);
    try {
      await restartTinto(installer);
    } catch (failure) {
      setError(message(failure, "No se pudo reiniciar Tinto."));
      setBusy(false);
    }
  };

  const install = async () => {
    const installer = await open({
      multiple: false,
      directory: false,
      title: "Instalador de Tinto",
      filters: [{ name: "Instalador", extensions: ["exe"] }],
    });
    if (typeof installer === "string") await restart(installer);
  };

  return (
    <div className="file-op-modal-overlay" onClick={onClose}>
      <div
        ref={dialogRef}
        className="file-op-modal restart-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="restart-title"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 id="restart-title" className="file-op-modal__title">
          Reiniciar o actualizar Tinto
        </h2>
        <div className="restart-modal__body">
          <p className="file-op-modal__body">
            Nada se detiene: los trabajos siguen solos y las conversaciones se reabren.
          </p>
          {plan ? (
            <>
              <section aria-label="Siguen en marcha">
                <h3>Siguen en marcha mientras Tinto está cerrado</h3>
                {plan.jobs.length === 0 ? (
                  <p className="restart-modal__empty">Ningún trabajo de Delivery en marcha.</p>
                ) : (
                  <ul>
                    {plan.jobs.map((job, index) => (
                      <li key={`${job.task_key}-${job.role}-${index}`}>
                        {job.task_key} · {job.role} <span>({job.agent})</span>
                      </li>
                    ))}
                  </ul>
                )}
              </section>
              <section aria-label="Se retoman al abrir">
                <h3>Se retoman al abrir</h3>
                {plan.conversations.length === 0 ? (
                  <p className="restart-modal__empty">Ninguna conversación abierta.</p>
                ) : (
                  <ul>
                    {plan.conversations.map((conversation) => (
                      <li key={conversation.session_id}>
                        {conversation.agent_type} · {basename(conversation.repo)}
                        {conversation.working && (
                          <span> — a mitad de turno: se le pide seguir</span>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            </>
          ) : (
            !error && <p className="restart-modal__empty">Leyendo lo que está en marcha…</p>
          )}
          {error && (
            <p className="restart-modal__error" role="alert">
              {error}
            </p>
          )}
        </div>
        <div className="file-op-modal__actions">
          <button
            type="button"
            className="file-op-modal__button file-op-modal__button--cancel"
            onClick={onClose}
            disabled={busy}
          >
            Cancelar
          </button>
          <button
            type="button"
            className="file-op-modal__button"
            onClick={() => void install()}
            disabled={busy || !plan}
          >
            Instalar actualización…
          </button>
          <button
            type="button"
            className="file-op-modal__button restart-modal__primary"
            onClick={() => void restart()}
            disabled={busy || !plan}
          >
            Reiniciar
          </button>
        </div>
      </div>
    </div>
  );
}

function message(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object" && "message" in error) {
    const text = String((error as { message: unknown }).message);
    if (text.trim()) return text;
  }
  return fallback;
}
