import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";
import type { IDockviewPanelProps } from "dockview-react";

// Keep Shiki out of jsdom (plain fallback render).
vi.mock("./highlight", () => ({
  MAX_HIGHLIGHT_BYTES: 1_000_000,
  languageFromPath: () => "typescript",
  loadHighlighter: () => Promise.resolve(null),
  highlightLine: () => null,
}));

// Controllable backend.
let worktree: { value: unknown; reject?: unknown } = { value: [] };
let fileContent: unknown = { encoding: "utf8", content: "a\nb\nc", truncated: false };
const invokeMock = vi.fn((cmd: string) => {
  if (cmd === "get_worktree_diff")
    return worktree.reject ? Promise.reject(worktree.reject) : Promise.resolve(worktree.value);
  if (cmd === "get_file_content") return Promise.resolve(fileContent);
  return Promise.resolve(undefined); // set_subscriptions, etc.
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => invokeMock(...(a as [string])),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

import { DiffPanel } from "./DiffPanel";
import { busStore } from "../../bus/store";
import { reconciler } from "../../workspace/subscriptions";
import type { FileDiff, RepoDelta } from "../../bus/contract";

const REPO = "/r/a";
const PATH = "src/a.ts";

const fileDiff = (path: string, content = "hello"): FileDiff => ({
  path,
  old_path: null,
  is_binary: false,
  hunks: [
    {
      old_start: 1,
      new_start: 1,
      lines: [{ kind: "Added", content, old_lineno: null, new_lineno: 1 }],
    },
  ],
});

const renamedDiff = (oldPath: string, newPath: string): FileDiff => ({
  ...fileDiff(newPath, "renamed"),
  old_path: oldPath,
});

function delta(over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo: REPO,
    revision: 1,
    status: { modified: [], staged: [], untracked: [] },
    branch: null,
    head: null,
    last_activity_ms: 1000,
    error: null,
    ...over,
  };
}

const renderPanel = (repo = REPO, path = PATH) =>
  render(
    <DiffPanel
      {...({ params: { repo, path } } as unknown as IDockviewPanelProps<{
        repo: string;
        path: string;
      }>)}
    />,
  );

describe("DiffPanel", () => {
  beforeEach(() => {
    busStore.resetAll();
    reconciler.reset();
    invokeMock.mockClear();
    worktree = { value: [] };
    fileContent = { encoding: "utf8", content: "a\nb\nc", truncated: false };
  });

  it("paints a tracked file from the one-shot, then a live delta supersedes it", async () => {
    worktree = { value: [fileDiff(PATH, "from one-shot")] };
    renderPanel();
    expect(await screen.findByText("from one-shot")).toBeInTheDocument();

    act(() =>
      busStore.applyDelta(delta({ revision: 2, subscribed_diffs: [fileDiff(PATH, "live!")] })),
    );
    expect(await screen.findByText("live!")).toBeInTheDocument();
    expect(screen.queryByText("from one-shot")).not.toBeInTheDocument();
  });

  it("does not resurface the one-shot when a fresh live compute clears a reverted file", async () => {
    worktree = { value: [fileDiff(PATH, "stale one-shot")] };
    renderPanel();
    expect(await screen.findByText("stale one-shot")).toBeInTheDocument();

    act(() =>
      busStore.applyDelta(delta({ revision: 2, subscribed_diffs: [fileDiff(PATH, "live")] })),
    );
    expect(await screen.findByText("live")).toBeInTheDocument();

    act(() => busStore.applyDelta(delta({ revision: 3, subscribed_diffs: [] })));
    expect(await screen.findByTestId("diff-empty")).toBeInTheDocument();
    expect(screen.queryByText("stale one-shot")).not.toBeInTheDocument();
    expect(screen.queryByText("live")).not.toBeInTheDocument();
  });

  it("keeps a live diff authoritative when the one-shot resolves later", async () => {
    let resolveOneShot: (value: FileDiff[]) => void = () => {};
    worktree = { value: new Promise<FileDiff[]>((resolve) => (resolveOneShot = resolve)) };
    renderPanel();

    act(() => busStore.applyDelta(delta({ subscribed_diffs: [fileDiff(PATH, "live first")] })));
    expect(await screen.findByText("live first")).toBeInTheDocument();

    await act(async () => resolveOneShot([fileDiff(PATH, "late one-shot")]));
    expect(screen.getByText("live first")).toBeInTheDocument();
    expect(screen.queryByText("late one-shot")).not.toBeInTheDocument();
  });

  it("a status-only delta (null diffs) does not blank an open diff (AE5)", async () => {
    act(() => busStore.applyDelta(delta({ subscribed_diffs: [fileDiff(PATH, "live!")] })));
    worktree = { value: [] };
    renderPanel();
    expect(await screen.findByText("live!")).toBeInTheDocument();
    act(() =>
      busStore.applyDelta(
        delta({
          revision: 2,
          status: { modified: [PATH], staged: [], untracked: [] },
          subscribed_diffs: null,
        }),
      ),
    );
    expect(screen.getByText("live!")).toBeInTheDocument(); // not blanked
  });

  it("untracked/idle: stays loading until a fresh compute, then shows clean (no hang)", async () => {
    worktree = { value: [] }; // one-shot empty (untracked excluded)
    renderPanel();
    expect(await screen.findByTestId("diff-loading")).toBeInTheDocument();
    // A fresh recompute arrives for the repo WITHOUT this target → computed+absent.
    act(() => busStore.applyDelta(delta({ subscribed_diffs: [] })));
    expect(await screen.findByTestId("diff-empty")).toBeInTheDocument();
  });

  it("shows an error state with retry when the one-shot fails", async () => {
    worktree = { value: [], reject: { category: "repo-not-allowed", message: "not allowed" } };
    renderPanel();
    expect(await screen.findByTestId("diff-error")).toHaveTextContent(
      "repo-not-allowed: not allowed",
    );
    worktree = { value: [fileDiff(PATH, "ok now")] };
    fireEvent.click(screen.getByTestId("diff-error-retry"));
    expect(await screen.findByText("ok now")).toBeInTheDocument();
  });

  it("toggles Hunks ↔ Full file and disables the layout toggle in full mode (AE4)", async () => {
    worktree = { value: [fileDiff(PATH, "code")] };
    renderPanel();
    await screen.findByText("code");
    fireEvent.click(screen.getByText("Full file"));
    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
    expect(screen.getByText("Inline")).toBeDisabled();
    expect(screen.getByText("Side by side")).toBeDisabled();
    fireEvent.click(screen.getByText("Hunks"));
    expect(await screen.findByText("code")).toBeInTheDocument();
  });

  it("shows the live-paused state when the target is over the cap", async () => {
    worktree = { value: [fileDiff(PATH, "code")] };
    renderPanel();
    await screen.findByText("code");
    // Open 8 newer targets so this panel's target (oldest) is evicted from live.
    act(() => {
      for (let i = 0; i < 8; i++) reconciler.add("/r/other", `f${i}`);
    });
    expect(await screen.findByTestId("diff-paused")).toBeInTheDocument();
  });

  it("shows a paused body instead of a false clean state when no cached diff exists", async () => {
    worktree = { value: [] };
    renderPanel();
    act(() => {
      for (let i = 0; i < 8; i++) reconciler.add("/r/other", `f${i}`);
    });

    expect(await screen.findByTestId("diff-paused")).toBeInTheDocument();
    expect(screen.getByTestId("diff-paused-body")).toHaveTextContent("Live updates paused");
    expect(screen.queryByTestId("diff-empty")).not.toBeInTheDocument();
    expect(screen.queryByTestId("diff-loading")).not.toBeInTheDocument();
  });

  it("shows a renamed-away state when the live slice only contains the new path", async () => {
    worktree = { value: [] };
    renderPanel();
    act(() =>
      busStore.applyDelta(delta({ subscribed_diffs: [renamedDiff(PATH, "src/renamed.ts")] })),
    );

    expect(await screen.findByTestId("diff-renamed")).toHaveTextContent(
      "This file was renamed to “src/renamed.ts”.",
    );
  });

  it("drops the subscription and the cached diff on unmount", async () => {
    act(() => busStore.applyDelta(delta({ subscribed_diffs: [fileDiff(PATH, "live!")] })));
    worktree = { value: [] };
    const { unmount } = renderPanel();
    await screen.findByText("live!");
    expect(reconciler.isLive(REPO, PATH)).toBe(true);
    unmount();
    await waitFor(() => expect(reconciler.isLive(REPO, PATH)).toBe(false));
    expect(busStore.getState().diffs[REPO]?.[PATH]).toBeUndefined();
  });
});
