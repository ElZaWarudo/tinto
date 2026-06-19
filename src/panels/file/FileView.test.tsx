import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, act, waitFor } from "@testing-library/react";

// Keep Shiki out of jsdom (plain fallback render).
vi.mock("../diff/highlight", () => ({
  MAX_HIGHLIGHT_BYTES: 1_000_000,
  languageFromPath: () => "typescript",
  loadHighlighter: () => Promise.resolve(null),
  highlightLine: () => null,
}));

// Render Markdown as a transparent passthrough (avoids ESM parsing in jsdom).
vi.mock("react-markdown", () => ({
  default: ({ children }: { children: string }) => <div data-testid="md-rendered">{children}</div>,
}));
vi.mock("remark-gfm", () => ({ default: {} }));

let worktree: { value: unknown; reject?: unknown } = { value: [] };
let fileContent: unknown = { encoding: "utf8", content: "a\nb\nc", truncated: false };
let mediaContent: unknown = { encoding: "base64", content: "iVBORw0KGgo=", truncated: false };
const invokeMock = vi.fn((cmd: string) => {
  if (cmd === "get_worktree_diff")
    return worktree.reject ? Promise.reject(worktree.reject) : Promise.resolve(worktree.value);
  if (cmd === "get_file_content") return Promise.resolve(fileContent);
  if (cmd === "get_media_content") return Promise.resolve(mediaContent);
  return Promise.resolve(undefined);
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => invokeMock(...(a as [string])),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

import { FileView } from "./FileView";
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

describe("FileView", () => {
  beforeEach(() => {
    busStore.resetAll();
    reconciler.reset();
    invokeMock.mockClear();
    worktree = { value: [] };
    fileContent = { encoding: "utf8", content: "a\nb\nc", truncated: false };
    mediaContent = { encoding: "base64", content: "iVBORw0KGgo=", truncated: false };
  });

  it("a clean file shows the normal full-file view by default (not a diff)", async () => {
    worktree = { value: [] };
    render(<FileView repo={REPO} path={PATH} />);
    // No change → defaults to "full"; FullFileView paints the content directly.
    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
    expect(screen.queryByTestId("diff-empty")).not.toBeInTheDocument();
  });

  it("a changed file defaults to the diff and can toggle to full file", async () => {
    worktree = { value: [fileDiff(PATH, "code")] };
    render(<FileView repo={REPO} path={PATH} />);
    expect(await screen.findByText("code")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Full file"));
    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
    expect(screen.getByText("Inline")).toBeDisabled();
    fireEvent.click(screen.getByText("Hunks"));
    expect(await screen.findByText("code")).toBeInTheDocument();
  });

  it("an untracked status file defaults to the diff surface even before its diff loads", async () => {
    act(() =>
      busStore.applyDelta(delta({ status: { modified: [], staged: [], untracked: [PATH] } })),
    );
    worktree = { value: [] };
    render(<FileView repo={REPO} path={PATH} />);
    // status says changed → "hunks"; no diff yet & not computed → loading (not full).
    expect(await screen.findByTestId("diff-loading")).toBeInTheDocument();
  });

  it("shows a spinner (not a 'no changes' flash) while a changed file's diff loads", async () => {
    // The repo already has computed diffs, but THIS file's live diff hasn't been
    // delivered yet — previously this flashed "No changes for this file".
    act(() =>
      busStore.applyDelta(
        delta({ status: { modified: [PATH], staged: [], untracked: [] }, subscribed_diffs: [] }),
      ),
    );
    worktree = { value: [] };
    render(<FileView repo={REPO} path={PATH} />);
    expect(await screen.findByTestId("diff-loading")).toBeInTheDocument();
    expect(screen.queryByTestId("diff-empty")).not.toBeInTheDocument();
  });

  it("renders Markdown formatted by default and toggles to source", async () => {
    fileContent = { encoding: "utf8", content: "# Title", truncated: false };
    render(<FileView repo={REPO} path="README.md" />);
    expect(await screen.findByTestId("markdown-view")).toBeInTheDocument();
    expect(screen.getByTestId("md-rendered")).toHaveTextContent("# Title");
    fireEvent.click(screen.getByText("Fuente"));
    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
  });

  it("renders visual media with the media preview surface", async () => {
    render(<FileView repo={REPO} path="brand/logo.png" />);
    expect(await screen.findByTestId("image-view")).toBeInTheDocument();
    expect(screen.getByTestId("media-mode")).toHaveTextContent("Image preview");
    expect(screen.queryByText("Hunks")).not.toBeInTheDocument();
    expect(screen.queryByTestId("diff-paused")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("get_media_content", {
      repo: REPO,
      path: "brand/logo.png",
    });
    expect(invokeMock).not.toHaveBeenCalledWith("get_worktree_diff", { repo: REPO });
  });

  it("renders PDFs with the media preview surface", async () => {
    mediaContent = { encoding: "base64", content: "JVBERi0x", truncated: false };
    render(<FileView repo={REPO} path="docs/spec.pdf" />);
    expect(await screen.findByTestId("pdf-view")).toBeInTheDocument();
    expect(screen.getByTestId("media-mode")).toHaveTextContent("PDF preview");
  });

  it("a live delta supersedes the one-shot diff", async () => {
    worktree = { value: [fileDiff(PATH, "from one-shot")] };
    render(<FileView repo={REPO} path={PATH} />);
    expect(await screen.findByText("from one-shot")).toBeInTheDocument();
    act(() =>
      busStore.applyDelta(delta({ revision: 2, subscribed_diffs: [fileDiff(PATH, "live!")] })),
    );
    expect(await screen.findByText("live!")).toBeInTheDocument();
    expect(screen.queryByText("from one-shot")).not.toBeInTheDocument();
  });

  it("shows an error state with retry when the one-shot fails (changed file)", async () => {
    act(() =>
      busStore.applyDelta(delta({ status: { modified: [PATH], staged: [], untracked: [] } })),
    );
    worktree = { value: [], reject: { category: "repo-not-allowed", message: "not allowed" } };
    render(<FileView repo={REPO} path={PATH} />);
    expect(await screen.findByTestId("diff-error")).toHaveTextContent(
      "repo-not-allowed: not allowed",
    );
    worktree = { value: [fileDiff(PATH, "ok now")] };
    fireEvent.click(screen.getByTestId("diff-error-retry"));
    expect(await screen.findByText("ok now")).toBeInTheDocument();
  });

  it("drops the subscription and the cached diff on unmount", async () => {
    act(() => busStore.applyDelta(delta({ subscribed_diffs: [fileDiff(PATH, "live!")] })));
    const { unmount } = render(<FileView repo={REPO} path={PATH} />);
    await screen.findByText("live!");
    expect(reconciler.isLive(REPO, PATH)).toBe(true);
    unmount();
    await waitFor(() => expect(reconciler.isLive(REPO, PATH)).toBe(false));
    expect(busStore.getState().diffs[REPO]?.[PATH]).toBeUndefined();
  });
});
