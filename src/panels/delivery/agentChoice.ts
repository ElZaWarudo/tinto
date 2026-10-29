// Who runs a Delivery agent — a stage or a coordinator — as the pickers hold
// it: the agent, its model and its reasoning, from what this machine offers.

import { codexReasoningLabel, lastRuntimeCatalog } from "../terminal/agentRuntimeCatalog";
import type { DeliveryAgent, DeliveryStageAgent } from "../../delivery/types";

export interface Choice {
  value: string;
  label: string;
}

const DEFAULT_EFFORT = "medium";
const CLAUDE_MODELS: Choice[] = [
  { value: "", label: "Predeterminado" },
  { value: "opus", label: "Opus" },
  { value: "sonnet", label: "Sonnet" },
  { value: "fable", label: "Fable" },
  { value: "haiku", label: "Haiku" },
];
const CLAUDE_EFFORTS = ["low", "medium", "high", "xhigh", "max"];
const CODEX_FALLBACK_EFFORTS = ["low", "medium", "high"];

/** Where the models OpenCode last listed are kept, for the pickers to have
 *  them before it answers again. */
const OPENCODE_MODELS_KEY = "tinto.delivery.opencodeModels";

/** Keeps the models OpenCode listed on this machine. */
export function rememberOpencodeModels(models: string[]) {
  try {
    localStorage.setItem(OPENCODE_MODELS_KEY, JSON.stringify(models));
  } catch {
    // Without storage the pickers offer OpenCode's default only.
  }
}

function opencodeModels(): string[] {
  try {
    const stored = JSON.parse(localStorage.getItem(OPENCODE_MODELS_KEY) ?? "[]");
    return Array.isArray(stored) ? stored.filter((model) => typeof model === "string") : [];
  } catch {
    return [];
  }
}

/** The models an agent offers. Codex's and Muse Code's come from the catalog
 *  Agents keeps; Sol 6.1 is Codex's default when the account has it.
 *  OpenCode's are what it lists on this machine. */
export function modelChoices(agent: DeliveryAgent): { choices: Choice[]; preferred: string } {
  if (agent === "opencode") {
    const choices = [
      { value: "", label: "Predeterminado" },
      ...opencodeModels().map((model) => ({ value: model, label: model })),
    ];
    return { choices, preferred: "" };
  }
  if (agent !== "codex" && agent !== "muse") return { choices: CLAUDE_MODELS, preferred: "" };
  const catalog = lastRuntimeCatalog(agent);
  const models = catalog?.models ?? [];
  if (models.length === 0)
    return { choices: [{ value: "", label: "Predeterminado" }], preferred: "" };
  const sol = models.find(
    (model) =>
      /sol/i.test(`${model.display_name} ${model.model}`) &&
      /6\.1/.test(`${model.display_name} ${model.model}`),
  );
  const fallback = models.find(
    (model) => model.model === catalog?.default_model || model.id === catalog?.default_model,
  );
  return {
    choices: models.map((model) => ({
      value: model.model,
      label: model.display_name || model.model,
    })),
    preferred: (sol ?? fallback ?? models[0]).model,
  };
}

/** Reasoning levels for an agent and model, and the one to preselect: medium
 *  when the model supports it. */
export function effortChoices(
  agent: DeliveryAgent,
  model: string,
): { choices: Choice[]; preferred: string } {
  // OpenCode's effort is a variant each provider names its own way: the
  // model's default, unless a coordinator names one.
  if (agent === "opencode") return { choices: [{ value: "", label: "Del modelo" }], preferred: "" };
  const values =
    agent === "codex" || agent === "muse"
      ? (lastRuntimeCatalog(agent)
          ?.models?.find((option) => option.model === model)
          ?.supported_reasoning_efforts.map((effort) => effort.value) ?? CODEX_FALLBACK_EFFORTS)
      : CLAUDE_EFFORTS;
  const preferred = values.includes(DEFAULT_EFFORT) ? DEFAULT_EFFORT : (values[0] ?? "");
  return {
    choices: values.map((value) => ({ value, label: codexReasoningLabel(value) })),
    preferred,
  };
}

/** Who runs a stage, as the selectors hold it ("" is the CLI's default). */
export interface AgentChoice {
  agent: DeliveryAgent;
  model: string;
  reasoning: string;
}

/** What the selectors show for a stage's agent: its own model and reasoning
 *  where this machine offers them, the preferred ones otherwise. */
export function agentChoice(from: DeliveryStageAgent | null | undefined): AgentChoice {
  const agent =
    from?.agent === "claude" || from?.agent === "muse" || from?.agent === "opencode"
      ? from.agent
      : ("codex" as const);
  const models = modelChoices(agent);
  const wanted = from?.agent === agent ? (from.model ?? "") : null;
  const model = models.choices.some((choice) => choice.value === wanted)
    ? (wanted as string)
    : models.preferred;
  return withModel({ agent, model, reasoning: from?.reasoning ?? "" }, model);
}

/** The choice with another model, on a reasoning level that model supports. */
export function withModel(choice: AgentChoice, model: string): AgentChoice {
  const efforts = effortChoices(choice.agent, model);
  const keeps = efforts.choices.some((effort) => effort.value === choice.reasoning);
  return { ...choice, model, reasoning: keeps ? choice.reasoning : efforts.preferred };
}
