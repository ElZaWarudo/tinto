// How much of each agent account's usage limit is spent, in a few words.

import type { DeliveryQuota } from "../delivery/types";

const AGENT_NAME: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex",
  muse: "Muse Code",
  opencode: "OpenCode",
};

const WINDOW_NAME: Record<string, string> = {
  five_hour: "5 h",
  seven_day: "7 días",
};

function resetTime(ms: number, now: number): string {
  const date = new Date(ms);
  const time = date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return ms - now < 24 * 60 * 60_000
    ? time
    : `${date.toLocaleDateString([], { day: "numeric", month: "short" })} ${time}`;
}

/** "Claude Code 94% (5 h) · se renueva 14:30", one per agent; a window that
 *  renewed since says nothing until the agent reports again. */
export function quotaSummary(quotas: DeliveryQuota[], now: number): string | null {
  const parts = quotas
    .filter((quota) => quota.resets_at_ms > now)
    .map(
      (quota) =>
        `${AGENT_NAME[quota.agent] ?? quota.agent} ${quota.utilization}% (${
          WINDOW_NAME[quota.window] ?? quota.window.replace(/_/g, " ")
        }) · se renueva ${resetTime(quota.resets_at_ms, now)}`,
    );
  return parts.length > 0 ? parts.join(" · ") : null;
}
