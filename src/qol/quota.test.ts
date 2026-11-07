import { describe, expect, it } from "vitest";
import { quotaSummary } from "./quota";

describe("quotaSummary", () => {
  it("names each agent's share and window, and drops windows that renewed since", () => {
    const now = Date.UTC(2026, 10, 7, 12);
    const summary = quotaSummary(
      [
        {
          agent: "claude",
          window: "five_hour",
          utilization: 94,
          resets_at_ms: now + 60 * 60_000,
          seen_at_ms: now,
        },
        {
          agent: "codex",
          window: "seven_day",
          utilization: 40,
          resets_at_ms: now - 1,
          seen_at_ms: now - 10,
        },
      ],
      now,
    );
    expect(summary).toMatch(/^Claude Code 94% \(5 h\) · se renueva \d/);
    expect(summary).not.toContain("Codex");
    expect(quotaSummary([], now)).toBeNull();
  });
});
