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
let fileContentRejects: unknown[] = [];
let mediaContent: unknown = { encoding: "base64", content: "iVBORw0KGgo=", truncated: false };
const invokeMock = vi.fn((cmd: string) => {
  if (cmd === "get_worktree_diff")
    return worktree.reject ? Promise.reject(worktree.reject) : Promise.resolve(worktree.value);
  if (cmd === "get_file_content") {
    const nextReject = fileContentRejects.shift();
    return nextReject ? Promise.reject(nextReject) : Promise.resolve(fileContent);
  }
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

const fileDiffWithAddedLines = (path: string, lines: number[]): FileDiff => ({
  path,
  old_path: null,
  is_binary: false,
  hunks: [
    {
      old_start: 1,
      new_start: 1,
      lines: lines.map((line) => ({
        kind: "Added",
        content: `line-${line}`,
        old_lineno: null,
        new_lineno: line,
      })),
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
    act(() => busStore.loadSnapshot([delta({ revision: 0 })], { available: true }));
    reconciler.reset();
    invokeMock.mockClear();
    Element.prototype.scrollIntoView = vi.fn();
    worktree = { value: [] };
    fileContent = { encoding: "utf8", content: "a\nb\nc", truncated: false };
    fileContentRejects = [];
    mediaContent = { encoding: "base64", content: "iVBORw0KGgo=", truncated: false };
  });

  it("a clean file shows the normal full-file view by default (not a diff)", async () => {
    worktree = { value: [] };
    render(<FileView repo={REPO} path={PATH} />);
    // No change → defaults to "full"; FullFileView paints the content directly.
    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
    expect(screen.queryByTestId("diff-empty")).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("get_worktree_diff", { repo: REPO });
  });

  it("does not keep the prior file content visible while a new path loads", async () => {
    fileContent = { encoding: "utf8", content: "first-file", truncated: false };
    const { rerender } = render(<FileView repo={REPO} path="src/first.ts" />);
    expect(await screen.findAllByText("first-file")).not.toHaveLength(0);

    fileContent = { encoding: "utf8", content: "second-file", truncated: false };
    rerender(<FileView repo={REPO} path="src/second.ts" />);

    expect(screen.queryAllByText("first-file")).toHaveLength(0);
    expect(await screen.findAllByText("second-file")).not.toHaveLength(0);
  });

  it("waits for the restored repo snapshot before loading file content", async () => {
    busStore.resetAll();
    render(<FileView repo={REPO} path={PATH} />);

    expect(screen.getByTestId("file-repo-loading")).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("get_file_content", { repo: REPO, path: PATH });

    act(() => busStore.loadSnapshot([delta({ revision: 0 })], { available: true }));

    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
  });

  it("a changed file defaults to the diff and can toggle to full file", async () => {
    act(() =>
      busStore.applyDelta(delta({ status: { modified: [PATH], staged: [], untracked: [] } })),
    );
    worktree = { value: [fileDiff(PATH, "code")] };
    render(<FileView repo={REPO} path={PATH} />);
    expect(await screen.findByText("code")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Full file"));
    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
    expect(screen.getByText("Inline")).toBeDisabled();
    fireEvent.click(screen.getByText("Hunks"));
    expect(await screen.findByText("code")).toBeInTheDocument();
  });

  it("shows overview markers for added possible secret lines", async () => {
    act(() =>
      busStore.applyDelta(delta({ status: { modified: [PATH], staged: [], untracked: [] } })),
    );
    worktree = { value: [fileDiff(PATH, 'api_key = "secret"')] };
    render(<FileView repo={REPO} path={PATH} />);

    expect(await screen.findByTestId("overview-marker-1-0")).toBeInTheDocument();
    expect(screen.getByText('api_key = "secret"').closest(".diff-line")).toHaveClass(
      "diff-line--signal-critical",
    );
  });

  it("keeps changed-line markers but collapses the inline labels per consecutive group", async () => {
    act(() =>
      busStore.applyDelta(delta({ status: { modified: [PATH], staged: [], untracked: [] } })),
    );
    worktree = { value: [fileDiffWithAddedLines(PATH, [1, 2, 3, 7, 8])] };
    const { container } = render(<FileView repo={REPO} path={PATH} />);

    expect(await screen.findByTestId("overview-marker-1-0")).toBeInTheDocument();
    expect(screen.getByTestId("overview-marker-2-1")).toBeInTheDocument();
    expect(screen.getByTestId("overview-marker-3-2")).toBeInTheDocument();
    expect(screen.getByTestId("overview-marker-7-3")).toBeInTheDocument();
    expect(screen.getByTestId("overview-marker-8-4")).toBeInTheDocument();
    expect(screen.getByText("line-2").closest(".diff-line")).toHaveClass(
      "diff-line--signal-critical",
    );

    const labels = Array.from(container.querySelectorAll(".line-marker-label--hunk"));
    expect(labels).toHaveLength(2);
    expect(labels.map((label) => label.textContent)).toEqual(["~Changed lines", "~Changed lines"]);
  });

  it("prefers backend secret findings for overview markers when present", async () => {
    act(() =>
      busStore.applyDelta(
        delta({
          revision: 2,
          status: { modified: [PATH], staged: [], untracked: [] },
          secret_findings: [
            {
              path: PATH,
              line: 1,
              rule_id: "generic-api-key",
              description: "Possible secret",
            },
          ],
        }),
      ),
    );
    worktree = { value: [fileDiff(PATH, 'const tokenLabel = "public";')] };
    render(<FileView repo={REPO} path={PATH} />);

    expect(await screen.findByTestId("overview-marker-1-0")).toBeInTheDocument();
    expect(screen.getByText('const tokenLabel = "public";').closest(".diff-line")).toHaveClass(
      "diff-line--signal-critical",
    );
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

  it("shows the backend file-content error for Markdown files", async () => {
    fileContentRejects = [
      { category: "child_exit", message: "el agente WSL cerro stdout" },
      { category: "child_exit", message: "el agente WSL cerro stdout" },
      { category: "child_exit", message: "el agente WSL cerro stdout" },
    ];
    render(<FileView repo={REPO} path="README.md" />);

    expect(await screen.findByTestId("md-error")).toHaveTextContent(
      "Could not load file: child_exit: el agente WSL cerro stdout",
    );
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByTestId("markdown-view")).toBeInTheDocument();
  });

  it("automatically retries a transient Markdown file-content failure", async () => {
    fileContentRejects = [{ category: "child_exit", message: "el agente WSL cerro stdout" }];
    render(<FileView repo={REPO} path="README.md" />);

    expect(await screen.findByTestId("markdown-view")).toBeInTheDocument();
    expect(screen.queryByTestId("md-error")).not.toBeInTheDocument();
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

  it("a changed file loads the initial one-shot diff", async () => {
    act(() =>
      busStore.applyDelta(delta({ status: { modified: [PATH], staged: [], untracked: [] } })),
    );
    worktree = { value: [fileDiff(PATH, "from one-shot")] };
    render(<FileView repo={REPO} path={PATH} />);
    expect(await screen.findByText("from one-shot")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("get_worktree_diff", { repo: REPO });
  });

  it("uses the live subscribed diff before falling back to the full repo diff", async () => {
    act(() =>
      busStore.applyDelta(delta({ status: { modified: [PATH], staged: [], untracked: [] } })),
    );
    render(<FileView repo={REPO} path={PATH} />);

    expect(await screen.findByTestId("diff-loading")).toBeInTheDocument();

    act(() =>
      busStore.applyDelta(
        delta({
          revision: 2,
          status: { modified: [PATH], staged: [], untracked: [] },
          subscribed_diffs: [fileDiff(PATH, "from live")],
        }),
      ),
    );

    expect(await screen.findByText("from live")).toBeInTheDocument();
    await new Promise((resolve) => window.setTimeout(resolve, 300));
    expect(invokeMock).not.toHaveBeenCalledWith("get_worktree_diff", { repo: REPO });
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
