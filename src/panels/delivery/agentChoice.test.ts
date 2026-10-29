import { beforeEach, describe, expect, it } from "vitest";
import { agentChoice, effortChoices, modelChoices, rememberOpencodeModels } from "./agentChoice";

beforeEach(() => localStorage.clear());

describe("OpenCode's choices", () => {
  it("offers the models OpenCode listed and keeps a stage's own, on the model's effort", () => {
    expect(modelChoices("opencode").choices.map((choice) => choice.value)).toEqual([""]);
    rememberOpencodeModels(["opencode/big-pickle", "anthropic/claude-sonnet-5-5"]);
    expect(modelChoices("opencode").choices.map((choice) => choice.value)).toEqual([
      "",
      "opencode/big-pickle",
      "anthropic/claude-sonnet-5-5",
    ]);
    expect(effortChoices("opencode", "opencode/big-pickle").choices).toEqual([
      { value: "", label: "Del modelo" },
    ]);
    expect(
      agentChoice({ agent: "opencode", model: "opencode/big-pickle", reasoning: null }),
    ).toEqual({ agent: "opencode", model: "opencode/big-pickle", reasoning: "" });
  });
});
