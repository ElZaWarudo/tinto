import { describe, it, expect, vi, beforeEach } from "vitest";
import type {
  AgentSession,
  AgentSessionChangeLog,
  AgentSessionOutput,
  AgentSessionStatus,
  DiffLineKind,
  FileDiff,
  RepoEntry,
  RepoDelta,
  WorkbenchConfig,
} from "./contract";

// D-008-5: the TS diff types must match the backend's serde output exactly.
// The Rust enum `DiffLineKind` derives Serialize with no rename_all, so it
// serializes PascalCase ("Added"/"Removed"/"Context"). A lowercase guess would
// compile but fail silently at runtime, so pin it here.
describe("diff contract types", () => {
  it("round-trips a FileDiff with PascalCase line kinds", () => {
    // Shape exactly as the backend emits it (snake_case fields, PascalCase kinds).
    const wire = JSON.stringify({
      path: "src/a.ts",
      old_path: null,
      is_binary: false,
      hunks: [
        {
          old_start: 12,
          new_start: 12,
          lines: [
            { kind: "Context", content: "ctx", old_lineno: 12, new_lineno: 12 },
            { kind: "Removed", content: "old", old_lineno: 13, new_lineno: null },
            { kind: "Added", content: "new", old_lineno: null, new_lineno: 13 },
          ],
        },
      ],
    });

    const diff = JSON.parse(wire) as FileDiff;
    expect(diff.path).toBe("src/a.ts");
    expect(diff.hunks[0].lines.map((l) => l.kind)).toEqual<DiffLineKind[]>([
      "Context",
      "Removed",
      "Added",
    ]);
    // Per-side line numbers: Removed has no new line; Added has no old line.
    expect(diff.hunks[0].lines[1].new_lineno).toBeNull();
    expect(diff.hunks[0].lines[2].old_lineno).toBeNull();
  });
});

describe("passive signal contract types", () => {
  it("accepts additive RepoDelta metrics and signals with snake_case kinds", () => {
    const wire = JSON.stringify({
      repo: "/r/api",
      revision: 2,
      status: { modified: [".env"], staged: [], untracked: [] },
      branch: null,
      head: null,
      last_activity_ms: 1000,
      error: null,
      metrics: { changed_files: 1, lines_added: 3, lines_removed: 1 },
      gitleaks_configured: false,
      signals: [
        {
          kind: "sensitive_path",
          severity: "warning",
          path: ".env",
          message: "Sensitive filename changed",
        },
      ],
      secret_findings: [
        {
          path: "src/config.ts",
          line: 12,
          rule_id: "generic-api-key",
          description: "Possible secret",
        },
      ],
    });
    const delta = JSON.parse(wire) as RepoDelta;
    expect(delta.metrics?.lines_added).toBe(3);
    expect(delta.gitleaks_configured).toBe(false);
    expect(delta.signals?.[0].kind).toBe("sensitive_path");
    expect(delta.signals?.[0].severity).toBe("warning");
    expect(delta.secret_findings?.[0].line).toBe(12);
  });
});

describe("workbench source contract types", () => {
  it("accepts legacy local entries without source and additive WSL entries", () => {
    const wire = JSON.stringify({
      version: 1,
      active: "Work",
      workbenches: [
        {
          name: "Work",
          repos: [
            { path: "/local/repo", alias: null, fs_watch: [] },
            {
              source: "wsl",
              path: "/home/me/repo",
              distro: "Ubuntu",
              alias: "WSL",
              fs_watch: [],
            },
          ],
        },
      ],
    });

    const config = JSON.parse(wire) as WorkbenchConfig;
    const local = config.workbenches[0].repos[0] satisfies RepoEntry;
    const wsl = config.workbenches[0].repos[1] satisfies RepoEntry;
    expect(local.source).toBeUndefined();
    expect(wsl.source).toBe("wsl");
    expect(wsl.distro).toBe("Ubuntu");
  });
});

describe("agent session contract types", () => {
  it("accepts session lifecycle metadata with snake_case status", () => {
    const wire = JSON.stringify({
      id: "sess-1",
      repo: "/r/api",
      agent_type: "codex",
      status: "running",
      pid: 42,
      started_at_ms: 1760000000000,
      ended_at_ms: null,
      exit_code: null,
      error: { category: "spawn_failed", message: "no se pudo iniciar la sesion" },
      checkpoint: {
        checkpoint_type: "git_ref",
        git_hash: "abc123",
        snapshot_files: [],
      },
      change_log: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 1760000000001 }],
      turn_status: "settling",
      turn_checkpoints: [
        {
          id: "sess-1:turn-1",
          index: 1,
          started_at_ms: 1760000000000,
          ended_at_ms: 1760000000100,
          checkpoint: {
            checkpoint_type: "git_ref",
            git_hash: "abc123",
            snapshot_files: [],
          },
          restore_checkpoint: {
            checkpoint_type: "git_ref",
            git_hash: "def456",
            snapshot_files: [],
          },
          changes: [{ path: "src/a.ts", kind: "modified", timestamp_ms: 1760000000100 }],
        },
      ],
      reverted_at_ms: null,
      restored_to_turn_index: null,
      active_sessions: 1,
      age_ms: 100,
      output_bytes_per_second: null,
    });

    const session = JSON.parse(wire) as AgentSession;
    expect(session.status).toEqual<AgentSessionStatus>("running");
    expect(session.agent_type).toBe("codex");
    expect(session.error?.category).toBe("spawn_failed");
    expect(session.checkpoint?.checkpoint_type).toBe("git_ref");
    expect(session.change_log?.[0].kind).toBe("modified");
    expect(session.turn_status).toBe("settling");
    expect(session.turn_checkpoints?.[0].changes[0].path).toBe("src/a.ts");
    expect(session.turn_checkpoints?.[0].restore_checkpoint?.git_hash).toBe("def456");
    expect(session.restored_to_turn_index).toBeNull();
    expect(session.active_sessions).toBe(1);
    expect(session.age_ms).toBe(100);
  });

  it("accepts output chunks with base64 payloads", () => {
    const wire = JSON.stringify({
      session_id: "sess-1",
      chunk_base64: "SG9sYQ0K",
      timestamp_ms: 1760000000001,
    });

    const output = JSON.parse(wire) as AgentSessionOutput;
    expect(output.session_id).toBe("sess-1");
    expect(output.chunk_base64).toBe("SG9sYQ0K");
    expect(output.timestamp_ms).toBe(1760000000001);
  });

  it("accepts change-log events with changed paths", () => {
    const wire = JSON.stringify({
      session_id: "sess-1",
      changes: [{ path: "src/a.ts", kind: "created", timestamp_ms: 1760000000002 }],
    });

    const log = JSON.parse(wire) as AgentSessionChangeLog;
    expect(log.session_id).toBe("sess-1");
    expect(log.changes[0].kind).toBe("created");
  });
});

// Client wrappers must issue the exact registered snake_case command names with
// single-word arg keys (Tauri maps camelCase→snake_case, but these are all
// single-word so identical either way).
const invokeMock = vi.fn();
const listenMock = vi.fn((...args: unknown[]) => {
  void args;
  return Promise.resolve(() => {});
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: (...a: unknown[]) => listenMock(...a) }));

import {
  getBlob,
  agentBinaryAvailable,
  agentBinaryAvailableForRepo,
  addWslRepo,
  forgetRepo,
  getAgentJournalSession,
  getCommitDiff,
  getGitleaksSetupStatus,
  getRepoGitleaksSetupStatus,
  getFileContent,
  getMediaContent,
  getWorktreeDiff,
  installGitleaks,
  installRepoGitleaks,
  createRepoAgentsMdConfig,
  createRepoGitleaksConfig,
  listAgentJournalSessions,
  listAgentSessions,
  listRepoTree,
  listWslDirectory,
  listWslDistros,
  onAgentSessionOutput,
  onAgentSessionChangeLog,
  resizeAgentSession,
  revertSession,
  revertSessionTurnFile,
  restoreSessionTurn,
  removeWslRepo,
  setSubscriptions,
  startAgentSession,
  stopAgentSession,
  updateRepo,
  writeAgentSessionInput,
} from "./client";

describe("RDM-008 client wrappers", () => {
  beforeEach(() => {
    invokeMock.mockClear();
    listenMock.mockClear();
  });

  it("get_worktree_diff passes repo", () => {
    void getWorktreeDiff("/r/api");
    expect(invokeMock).toHaveBeenCalledWith("get_worktree_diff", { repo: "/r/api" });
  });

  it("get_file_content passes repo + path", () => {
    void getFileContent("/r/api", "src/a.ts");
    expect(invokeMock).toHaveBeenCalledWith("get_file_content", {
      repo: "/r/api",
      path: "src/a.ts",
    });
  });

  it("get_media_content passes repo + path", () => {
    void getMediaContent("/r/api", "brand/logo.png");
    expect(invokeMock).toHaveBeenCalledWith("get_media_content", {
      repo: "/r/api",
      path: "brand/logo.png",
    });
  });

  it("list_repo_tree passes repo", () => {
    void listRepoTree("/r/api");
    expect(invokeMock).toHaveBeenCalledWith("list_repo_tree", { repo: "/r/api" });
  });

  it("set_subscriptions passes the targets array", () => {
    const targets = [{ repo: "/r/api", path: "src/a.ts" }];
    void setSubscriptions(targets);
    expect(invokeMock).toHaveBeenCalledWith("set_subscriptions", { targets });
  });

  it("forgetRepo drops a repo from the live bus snapshot", () => {
    void forgetRepo("/r/api");
    expect(invokeMock).toHaveBeenCalledWith("forget_repo", { repo: "/r/api" });
  });

  it("get_commit_diff passes repo + commitId using Tauri camelCase arg keys", () => {
    void getCommitDiff("/r/api", "abc123");
    expect(invokeMock).toHaveBeenCalledWith("get_commit_diff", {
      repo: "/r/api",
      commitId: "abc123",
    });
  });

  it("get_blob passes repo + commitId + path using Tauri camelCase arg keys", () => {
    void getBlob("/r/api", "abc123", "src/a.ts");
    expect(invokeMock).toHaveBeenCalledWith("get_blob", {
      repo: "/r/api",
      commitId: "abc123",
      path: "src/a.ts",
    });
  });

  it("update_repo passes fsWatch with Tauri's camelCase arg keys", () => {
    void updateRepo("Work", "/r/api", { fsWatch: [".env"], clearAlias: false });
    expect(invokeMock).toHaveBeenCalledWith("update_repo", {
      workbench: "Work",
      path: "/r/api",
      alias: undefined,
      clearAlias: false,
      fsWatch: [".env"],
    });
  });

  it("WSL config wrappers use Windows-only command names", () => {
    void addWslRepo("Work", "Ubuntu", "/home/me/repo", "WSL");
    expect(invokeMock).toHaveBeenCalledWith("add_wsl_repo", {
      workbench: "Work",
      distro: "Ubuntu",
      path: "/home/me/repo",
      alias: "WSL",
    });

    void removeWslRepo("Work", "Ubuntu", "/home/me/repo");
    expect(invokeMock).toHaveBeenCalledWith("remove_wsl_repo", {
      workbench: "Work",
      distro: "Ubuntu",
      path: "/home/me/repo",
    });

    void listWslDistros();
    expect(invokeMock).toHaveBeenCalledWith("list_wsl_distros");

    void listWslDirectory("Ubuntu", "/home/me");
    expect(invokeMock).toHaveBeenCalledWith("list_wsl_directory", {
      distro: "Ubuntu",
      path: "/home/me",
    });
  });

  it("agent session wrappers use registered command names", () => {
    void startAgentSession("/r/api", "codex");
    expect(invokeMock).toHaveBeenCalledWith("start_agent_session", {
      repo: "/r/api",
      agentType: "codex",
    });

    void stopAgentSession("sess-1");
    expect(invokeMock).toHaveBeenCalledWith("stop_agent_session", {
      sessionId: "sess-1",
    });

    void listAgentSessions();
    expect(invokeMock).toHaveBeenCalledWith("list_agent_sessions");

    void listAgentJournalSessions(12);
    expect(invokeMock).toHaveBeenCalledWith("list_agent_journal_sessions", {
      limit: 12,
    });

    void getAgentJournalSession("sess-1");
    expect(invokeMock).toHaveBeenCalledWith("get_agent_journal_session", {
      sessionId: "sess-1",
    });

    void revertSession("sess-1", true);
    expect(invokeMock).toHaveBeenCalledWith("revert_session", {
      sessionId: "sess-1",
      userConsent: true,
    });

    void revertSessionTurnFile("sess-1", "sess-1:turn-1", "src/a.ts", true);
    expect(invokeMock).toHaveBeenCalledWith("revert_session_turn_file", {
      sessionId: "sess-1",
      turnCheckpointId: "sess-1:turn-1",
      path: "src/a.ts",
      userConsent: true,
    });

    void restoreSessionTurn("sess-1", "sess-1:turn-1", true);
    expect(invokeMock).toHaveBeenCalledWith("restore_session_turn", {
      sessionId: "sess-1",
      turnCheckpointId: "sess-1:turn-1",
      userConsent: true,
    });

    void agentBinaryAvailable("codex");
    expect(invokeMock).toHaveBeenCalledWith("agent_binary_available", {
      agentType: "codex",
    });

    void agentBinaryAvailableForRepo("/r/api", "codex");
    expect(invokeMock).toHaveBeenCalledWith("agent_binary_available_for_repo", {
      repo: "/r/api",
      agentType: "codex",
    });
  });

  it("agent terminal stream wrappers encode input and resize dimensions", () => {
    void writeAgentSessionInput("sess-1", "hi\r");
    expect(invokeMock).toHaveBeenCalledWith("write_agent_session_input", {
      sessionId: "sess-1",
      inputBase64: "aGkN",
    });

    void writeAgentSessionInput("sess-1", new Uint8Array([0x1b, 0x5b, 0x41]));
    expect(invokeMock).toHaveBeenCalledWith("write_agent_session_input", {
      sessionId: "sess-1",
      inputBase64: "G1tB",
    });

    void resizeAgentSession("sess-1", 120, 36);
    expect(invokeMock).toHaveBeenCalledWith("resize_agent_session", {
      sessionId: "sess-1",
      cols: 120,
      rows: 36,
    });
  });

  it("listens for agent session output event", () => {
    void onAgentSessionOutput(() => {});

    expect(listenMock).toHaveBeenCalledWith("tinto://agent-session-output", expect.any(Function));
  });

  it("listens for agent session change-log events", () => {
    void onAgentSessionChangeLog(() => {});

    expect(listenMock).toHaveBeenCalledWith(
      "tinto://agent-session-change-log",
      expect.any(Function),
    );
  });

  it("addon wrappers call registered command names", () => {
    void getGitleaksSetupStatus();
    expect(invokeMock).toHaveBeenCalledWith("get_gitleaks_setup_status");

    void installGitleaks();
    expect(invokeMock).toHaveBeenCalledWith("install_gitleaks");

    void getRepoGitleaksSetupStatus("/r/api");
    expect(invokeMock).toHaveBeenCalledWith("get_repo_gitleaks_setup_status", { repo: "/r/api" });

    void installRepoGitleaks("/r/api");
    expect(invokeMock).toHaveBeenCalledWith("install_repo_gitleaks", { repo: "/r/api" });

    void createRepoGitleaksConfig("/r/api");
    expect(invokeMock).toHaveBeenCalledWith("create_repo_gitleaks_config", { repo: "/r/api" });

    void createRepoAgentsMdConfig("/r/api");
    expect(invokeMock).toHaveBeenCalledWith("create_repo_agents_md_config", { repo: "/r/api" });
  });
});
