// Fixed top bar (outside the dock area): brand, workbench switcher, add/auto-
// detect actions, and a watching indicator. Switching swaps data; the layout is
// untouched (handled by the workspace).

import { useBusState } from "../bus/store";
import { useWorkspaceActions } from "../workspace/actions";
import { addRepoFlow, autodetectFlow, switchWorkbench } from "./operations";

export function TopBar() {
  const { config, watching } = useBusState();
  const { openTimeline } = useWorkspaceActions();
  const active = config?.active ?? "";
  const workbenches = config?.workbenches ?? [];

  return (
    <div className="top-bar">
      <span className="top-bar__brand">Tinto</span>

      <select
        className="top-bar__switcher"
        data-testid="wb-switcher"
        value={active}
        onChange={(e) => void switchWorkbench(e.target.value, active || null)}
      >
        {workbenches.map((w) => (
          <option key={w.name} value={w.name}>
            {w.name}
          </option>
        ))}
      </select>

      <span className="top-bar__spacer" />

      <button data-testid="add-repo" onClick={() => active && void addRepoFlow(active)}>
        Add repo
      </button>
      <button data-testid="autodetect" onClick={() => active && void autodetectFlow(active)}>
        Auto-detect
      </button>
      <button data-testid="open-timeline" onClick={openTimeline}>
        Timeline
      </button>

      <span
        className={watching.available ? "watch watch--ok" : "watch watch--bad"}
        title={watching.reason ?? "watching"}
        data-testid="watch-indicator"
      >
        {watching.available ? "● watching" : "○ degraded"}
      </span>
    </div>
  );
}
