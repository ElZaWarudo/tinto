import { useState } from "react";
import { createRoot } from "react-dom/client";
import type { AgentRuntimeCatalog } from "../bus/contract";
import {
  AgentRuntimeControls,
  type CodexRuntimeMenu,
} from "../panels/terminal/AgentRuntimeControls";
import type {
  CodexModelSelection,
  CodexReasoningSelection,
  CodexSpeedSelection,
} from "../panels/terminal/agentRuntimeCatalog";
import "../App.css";
import "./agentRuntime.css";

const efforts = [
  { value: "medium", description: "Equilibrado" },
  { value: "high", description: "Profundo" },
];

const catalog: AgentRuntimeCatalog = {
  status: "ready",
  source: "codex_app_server",
  default_model: "gpt-5.6-sol",
  error: null,
  updated_at_ms: Date.now(),
  models: [
    {
      id: "gpt-5.6-sol",
      model: "gpt-5.6-sol",
      display_name: "GPT-5.6 Sol",
      description: "Propósito general",
      supported_reasoning_efforts: efforts,
      default_reasoning_effort: "medium",
      service_tiers: [{ id: "fast", name: "Rápido", description: "Menor latencia" }],
      default_service_tier: null,
      is_default: true,
    },
    ...["Luna", "Bruma", "Norte", "Sur", "Atlas", "Cobre", "Oliva", "Río", "Faro"].map(
      (name, index) => ({
        id: `codex-${name.toLocaleLowerCase()}`,
        model: `codex-${name.toLocaleLowerCase()}`,
        display_name: `Codex ${name}`,
        description: `Perfil ${index + 1}`,
        supported_reasoning_efforts: efforts,
        default_reasoning_effort: "medium",
        service_tiers: [],
        default_service_tier: null,
        is_default: false,
      }),
    ),
  ],
};

export function AgentRuntimeFixture() {
  const [menu, setMenu] = useState<CodexRuntimeMenu>(null);
  const [model, setModel] = useState<CodexModelSelection>("auto");
  const [reasoning, setReasoning] = useState<CodexReasoningSelection>("auto");
  const [speed, setSpeed] = useState<CodexSpeedSelection>("standard");

  return (
    <main className="agent-runtime-fixture">
      <section className="agent-runtime-fixture__panel" aria-label="Sesión de prueba de Codex">
        <header className="agent-runtime-fixture__head">
          <strong>CODEX</strong>
          <span>EN CURSO</span>
        </header>
        <div className="agent-runtime-fixture__space" />
        <form className="agent-panel__composer" onSubmit={(event) => event.preventDefault()}>
          <AgentRuntimeControls
            catalog={catalog}
            disabled={false}
            idBase="runtime-fixture"
            menu={menu}
            model={model}
            notice={null}
            onMenuChange={setMenu}
            onModelChange={setModel}
            onReasoningChange={setReasoning}
            onRefreshCatalog={() => {}}
            onSpeedChange={setSpeed}
            providerLabel="Codex"
            reasoning={reasoning}
            speed={speed}
          />
          <div className="agent-panel__composer-row">
            <textarea aria-label="Mensaje para Codex" placeholder="Mensaje para Codex" rows={2} />
            <button className="agent-panel__send" type="submit">
              Enviar
            </button>
          </div>
        </form>
      </section>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<AgentRuntimeFixture />);
