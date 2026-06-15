import { describe, it, expect, vi, beforeEach } from "vitest";
import type { DiffLineKind, FileDiff } from "./contract";

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

// Client wrappers must issue the exact registered snake_case command names with
// single-word arg keys (Tauri maps camelCase→snake_case, but these are all
// single-word so identical either way).
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

import {
  getBlob,
  getCommitDiff,
  getFileContent,
  getWorktreeDiff,
  listRepoTree,
  setSubscriptions,
  updateRepo,
} from "./client";

describe("RDM-008 client wrappers", () => {
  beforeEach(() => invokeMock.mockClear());

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

  it("list_repo_tree passes repo", () => {
    void listRepoTree("/r/api");
    expect(invokeMock).toHaveBeenCalledWith("list_repo_tree", { repo: "/r/api" });
  });

  it("set_subscriptions passes the targets array", () => {
    const targets = [{ repo: "/r/api", path: "src/a.ts" }];
    void setSubscriptions(targets);
    expect(invokeMock).toHaveBeenCalledWith("set_subscriptions", { targets });
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
});
