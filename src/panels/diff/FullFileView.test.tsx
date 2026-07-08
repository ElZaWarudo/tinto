import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { FileContent } from "../../bus/contract";

let content: FileContent = { encoding: "utf8", content: "one\ntwo\n", truncated: false };
let rejects: unknown[] = [];
let contentLoads: Array<Promise<FileContent>> = [];

vi.mock("../../bus/client", () => ({
  getFileContent: vi.fn(() => {
    const nextLoad = contentLoads.shift();
    if (nextLoad) return nextLoad;
    const nextReject = rejects.shift();
    if (nextReject) return Promise.reject(nextReject);
    return Promise.resolve(content);
  }),
}));

import { FullFileView } from "./FullFileView";

describe("FullFileView", () => {
  beforeEach(() => {
    content = { encoding: "utf8", content: "one\ntwo\n", truncated: false };
    rejects = [];
    contentLoads = [];
    Element.prototype.scrollIntoView = vi.fn();
  });

  it("renders utf8 content, highlights changed lines, and drops one trailing newline row", async () => {
    const { container } = render(
      <FullFileView repo="/r/a" path="src/a.ts" changedLines={new Set([2])} />,
    );

    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
    const rows = Array.from(container.querySelectorAll(".full-file__line .diff-content"));
    expect(rows).toHaveLength(2);
    expect(container.querySelectorAll(".full-file__line")).toHaveLength(2);
    expect(container.querySelector('[data-line="2"]')).toHaveClass("full-file__line--changed");
  });

  it("degrades binary/base64 content to a guarded placeholder", async () => {
    content = { encoding: "base64", content: "AAEC", truncated: false };
    render(<FullFileView repo="/r/a" path="bin.dat" changedLines={new Set()} />);

    expect(await screen.findByTestId("full-binary")).toHaveTextContent(
      "Binary file — cannot show full content.",
    );
    expect(screen.queryByText("AAEC")).not.toBeInTheDocument();
  });

  it("shows the truncated notice with the content returned by the backend", async () => {
    content = { encoding: "utf8", content: "visible", truncated: true };
    const { container } = render(
      <FullFileView repo="/r/a" path="large.txt" changedLines={new Set()} />,
    );

    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
    expect(container.querySelector(".full-file__line .diff-content")).toHaveTextContent("visible");
    expect(screen.getByTestId("full-truncated")).toHaveTextContent("File truncated");
  });

  it("ignores stale full-file content when the repo revision changes mid-load", async () => {
    let resolveOld: (value: FileContent) => void = () => {};
    let resolveFresh: (value: FileContent) => void = () => {};
    contentLoads = [
      new Promise<FileContent>((resolve) => {
        resolveOld = resolve;
      }),
      new Promise<FileContent>((resolve) => {
        resolveFresh = resolve;
      }),
    ];

    const { container, rerender } = render(
      <FullFileView repo="/r/a" path="src/a.ts" repoRevision={1} changedLines={new Set()} />,
    );
    rerender(
      <FullFileView repo="/r/a" path="src/a.ts" repoRevision={2} changedLines={new Set()} />,
    );

    await act(async () => {
      resolveOld({ encoding: "utf8", content: "old revision\n", truncated: false });
      resolveFresh({ encoding: "utf8", content: "fresh revision\n", truncated: false });
    });

    await waitFor(() => {
      expect(container.querySelector(".full-file__line .diff-content")).toHaveTextContent(
        "fresh revision",
      );
    });
    expect(
      Array.from(container.querySelectorAll(".full-file__line .diff-content")).some((node) =>
        node.textContent?.includes("old revision"),
      ),
    ).toBe(false);
  });

  it("shows an error state when file content cannot be loaded", async () => {
    rejects = [
      { category: "child_exit", message: "el agente WSL cerro stdout" },
      { category: "child_exit", message: "el agente WSL cerro stdout" },
      { category: "child_exit", message: "el agente WSL cerro stdout" },
    ];
    render(<FullFileView repo="/r/a" path="missing.ts" changedLines={new Set()} />);

    expect(await screen.findByTestId("full-error")).toHaveTextContent(
      "Could not load file: child_exit: el agente WSL cerro stdout",
    );
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
  });

  it("automatically retries a transient file-content failure", async () => {
    rejects = [{ category: "child_exit", message: "el agente WSL cerro stdout" }];
    render(<FullFileView repo="/r/a" path="src/a.ts" changedLines={new Set()} />);

    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
    expect(screen.queryByTestId("full-error")).not.toBeInTheDocument();
  });

  it("renders overview markers and highlights the matching full-file line", async () => {
    const { container } = render(
      <FullFileView
        repo="/r/a"
        path="src/a.ts"
        changedLines={new Set()}
        overviewMarkers={[{ line: 2, severity: "critical", label: "Possible secret" }]}
      />,
    );

    expect(await screen.findByTestId("overview-summary")).toHaveTextContent("1");
    expect(await screen.findByTestId("overview-marker-2-0")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Possible secret, línea 2" })).toBeInTheDocument();
    expect(container.querySelector('[data-line="2"]')).toHaveClass(
      "full-file__line--signal-critical",
    );
    const label = container.querySelector('[data-line="2"] .line-marker-label');
    expect(label).toHaveTextContent("Possible secret");
    expect(label).toHaveAttribute("title", "Possible secret · línea 2");
    expect(container.querySelector('[data-line="2"]')).toBeInTheDocument();
  });

  it("keeps full-file line highlights while showing one compact changed-line label", async () => {
    content = { encoding: "utf8", content: "one\ntwo\nthree\n", truncated: false };
    const { container } = render(
      <FullFileView
        repo="/r/a"
        path="src/a.ts"
        changedLines={new Set([1, 2, 3])}
        overviewMarkers={[
          {
            line: 1,
            severity: "info",
            label: "Changed lines",
            source: "hunk",
            showLabel: true,
          },
          {
            line: 2,
            severity: "info",
            label: "Changed line",
            source: "hunk",
            showLabel: false,
          },
          {
            line: 3,
            severity: "info",
            label: "Changed line",
            source: "hunk",
            showLabel: false,
          },
        ]}
      />,
    );

    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
    expect(container.querySelector('[data-line="1"]')).toHaveClass(
      "full-file__line--signal-critical",
    );
    expect(container.querySelector('[data-line="2"]')).toHaveClass(
      "full-file__line--signal-critical",
    );
    expect(container.querySelector('[data-line="3"]')).toHaveClass(
      "full-file__line--signal-critical",
    );
    const labels = Array.from(container.querySelectorAll(".line-marker-label--hunk"));
    expect(labels).toHaveLength(1);
    expect(labels[0]).toHaveTextContent("~Changed lines");
  });
});
