import React from "react";
import { createRoot } from "react-dom/client";
import type { RepoDelta, WorkbenchConfig } from "../bus/contract";
import { busStore } from "../bus/store";
import { DashboardPanel } from "../panels/DashboardPanel";
import { MenuBar } from "../workbench/MenuBar";
import { WorkspaceActionsContext, type WorkspaceActions } from "../workspace/actions";
import "../App.css";
import "./dashboardReview.css";

const NOW = Date.now();
const REPOS = {
  tinto: "C:\\Users\\User\\Documents\\personal\\tinto",
  passport: "C:\\Users\\User\\Documents\\personal\\digital-product-passport",
  notes: "C:\\Users\\User\\Documents\\personal\\notes-api",
};

function repoDelta(repo: string, changes: string[], offset: number): RepoDelta {
  return {
    repo,
    revision: offset + 1,
    status: {
      modified: changes.slice(0, 2),
      staged: changes.slice(2, 3),
      untracked: changes.slice(3),
    },
    branch: {
      name: offset === 2 ? "feature/mobile-companion" : "develop",
      detached: false,
      unborn: false,
      ahead: offset,
      behind: offset === 1 ? 2 : 0,
    },
    head: {
      id: `0a1b2c${offset}`,
      summary: offset === 0 ? "Improve responsive workbench" : "Keep repo state current",
      author: "Tinto",
      timestamp: Math.floor((NOW - offset * 3_600_000) / 1000),
    },
    last_activity_ms: NOW - offset * 70_000,
    error: null,
    metrics: {
      changed_files: changes.length,
      lines_added: 24 + offset * 17,
      lines_removed: 6 + offset * 4,
    },
    signals:
      offset === 1
        ? [
            {
              kind: "config_change",
              severity: "warning",
              path: "vite.config.ts",
              message: "Configuración de compilación modificada",
            },
          ]
        : [],
    secret_findings: [],
    subscribed_diffs: null,
    gitleaks_configured: true,
    agents_md_configured: true,
    secret_scan_status:
      offset === 1
        ? {
            state: "degraded",
            engine: "heuristic",
            failure_category: "binary_unavailable",
            message: "Gitleaks no está instalado; se usó el detector básico.",
          }
        : { state: "clean", engine: "gitleaks", version: "8.30.1" },
  };
}

const config: WorkbenchConfig = {
  version: 1,
  active: "Producto",
  workbenches: [
    {
      name: "Producto",
      repos: [
        { path: REPOS.tinto, alias: "tinto", fs_watch: [] },
        { path: REPOS.passport, alias: "product-passport", fs_watch: [] },
        { path: REPOS.notes, alias: "notes-api", fs_watch: [] },
      ],
    },
  ],
};

function installTauriFixture() {
  const windowWithTauri = window as typeof window & {
    __TAURI_INTERNALS__?: {
      invoke?: (cmd: string, args?: unknown) => Promise<unknown>;
      transformCallback?: () => number;
    };
  };

  windowWithTauri.__TAURI_INTERNALS__ = {
    ...(windowWithTauri.__TAURI_INTERNALS__ ?? {}),
    invoke: async (cmd: string) => {
      if (cmd === "agent_binary_available_for_repo") return true;
      if (cmd === "list_agent_sessions") return [];
      return null;
    },
    transformCallback: () => 0,
  };
}

installTauriFixture();
busStore.resetAll();
busStore.loadWorkbench(
  config,
  [
    repoDelta(REPOS.tinto, ["src/App.css", "src/workbench/MenuBar.tsx", "README.md"], 0),
    repoDelta(
      REPOS.passport,
      ["src/agent-view.tsx", "vite.config.ts", "docs/agent-lens.md", "src/mobile.ts"],
      1,
    ),
    repoDelta(REPOS.notes, ["src/routes/notes.ts", "tests/notes.test.ts"], 2),
  ],
  { available: true },
);

const actions: WorkspaceActions = {
  openRepo: () => {},
  addRepo: () => {},
  removeRepo: () => {},
  openFile: () => {},
  openTimeline: () => {},
  openDashboard: () => {},
  openAgents: () => {},
  openAgentTerminal: () => {},
};

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <WorkspaceActionsContext.Provider value={actions}>
      <div className="dashboard-review">
        <MenuBar />
        <main className="dashboard-review__body">
          <DashboardPanel />
        </main>
      </div>
    </WorkspaceActionsContext.Provider>
  </React.StrictMode>,
);
