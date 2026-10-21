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

/** The models an agent offers. Codex's and Muse Code's come from the catalog
 *  Agents keeps; Sol 6.1 is Codex's default when the account has it. */
export function modelChoices(agent: DeliveryAgent): { choices: Choice[]; preferred: string } {
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
    from?.agent === "claude" || from?.agent === "muse" ? from.agent : ("codex" as const);
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
