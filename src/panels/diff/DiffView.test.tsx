import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, within } from "@testing-library/react";

// Keep Shiki out of jsdom: the highlighter never loads, so DiffView renders the
// plain-text fallback — which is exactly what AE3 / large-file fallback assert.
vi.mock("./highlight", () => ({
  MAX_HIGHLIGHT_BYTES: 64,
  languageFromPath: () => "typescript",
  loadHighlighter: () => Promise.resolve(null),
  highlightLine: () => null,
}));

import { DiffView } from "./DiffView";
import { MAX_RENDERED_DIFF_LINE_CHARS, MAX_RENDERED_DIFF_LINES } from "./limits";
import type { FileDiff } from "../../bus/contract";

const sample: FileDiff = {
  path: "src/a.ts",
  old_path: null,
  is_binary: false,
  hunks: [
    {
      old_start: 12,
      new_start: 12,
      lines: [
        { kind: "Context", content: "ctx", old_lineno: 12, new_lineno: 12 },
        { kind: "Removed", content: "old line", old_lineno: 13, new_lineno: null },
        { kind: "Added", content: "new line", old_lineno: null, new_lineno: 13 },
      ],
    },
  ],
};

describe("DiffView", () => {
  beforeEach(() => {
    Element.prototype.scrollIntoView = vi.fn();
  });

  it("renders inline rows with correct per-side line numbers (AE1)", () => {
    render(<DiffView diff={sample} mode="inline" />);
    expect(screen.getByTestId("diff-view")).toHaveClass("diff-view--inline");
    expect(screen.getByTestId("hunk-header")).toHaveTextContent("@@ -12 +12 @@");
    expect(screen.getByText("old line").closest(".diff-line")).toHaveClass("diff-line--removed");
    expect(screen.getByText("new line").closest(".diff-line")).toHaveClass("diff-line--added");
  });

  it("renders side-by-side with old left / new right (AE2)", () => {
    render(<DiffView diff={sample} mode="side-by-side" />);
    const oldCol = screen.getByTestId("diff-old");
    const newCol = screen.getByTestId("diff-new");
    // Old column has context + removed, not the added line.
    expect(within(oldCol).getByText("old line")).toBeInTheDocument();
    expect(within(oldCol).queryByText("new line")).not.toBeInTheDocument();
    // New column has context + added, not the removed line.
    expect(within(newCol).getByText("new line")).toBeInTheDocument();
    expect(within(newCol).queryByText("old line")).not.toBeInTheDocument();
    // Context appears on both sides.
    expect(within(oldCol).getByText("ctx")).toBeInTheDocument();
    expect(within(newCol).getByText("ctx")).toBeInTheDocument();
  });

  it("shows a binary placeholder instead of a diff (AE7)", () => {
    render(<DiffView diff={{ ...sample, is_binary: true }} mode="inline" />);
    expect(screen.getByTestId("diff-binary")).toBeInTheDocument();
    expect(screen.queryByTestId("diff-view")).not.toBeInTheDocument();
  });

  it("disables highlighting and shows a notice for an oversized file (AE13)", () => {
    // The mocked cap is 64 bytes; this content exceeds it.
    const big: FileDiff = {
      ...sample,
      hunks: [
        {
          old_start: 1,
          new_start: 1,
          lines: [{ kind: "Added", content: "x".repeat(100), old_lineno: null, new_lineno: 1 }],
        },
      ],
    };
    render(<DiffView diff={big} mode="inline" />);
    expect(screen.getByTestId("diff-large")).toHaveTextContent("Comparación extensa");
    expect(screen.getByText("x".repeat(100))).toBeInTheDocument(); // still rendered, plain
  });

  it("shortens very long diff lines without hiding the rest of the hunk", () => {
    const longContent = `prefix-${"x".repeat(MAX_RENDERED_DIFF_LINE_CHARS + 10)}`;
    const diff: FileDiff = {
      ...sample,
      path: "skills-lock.json",
      hunks: [
        {
          old_start: 1,
          new_start: 1,
          lines: [
            { kind: "Added", content: longContent, old_lineno: null, new_lineno: 1 },
            { kind: "Added", content: "after-long-line", old_lineno: null, new_lineno: 2 },
          ],
        },
      ],
    };

    render(<DiffView diff={diff} mode="inline" />);

    expect(screen.getByTestId("diff-long-lines")).toHaveTextContent("1 línea extensa");
    expect(screen.getByTestId("diff-line-truncated")).toHaveTextContent("caracteres ocultos");
    expect(screen.getByText("after-long-line")).toBeInTheDocument();
  });

  it("caps rendered rows for very large hunks to keep the view responsive", () => {
    const lines = Array.from({ length: MAX_RENDERED_DIFF_LINES + 5 }, (_, index) => ({
      kind: "Added" as const,
      content: `line-${index}`,
      old_lineno: null,
      new_lineno: index + 1,
    }));
    const big: FileDiff = {
      ...sample,
      path: "skills-lock.json",
      hunks: [{ old_start: 1, new_start: 1, lines }],
    };

    const { container } = render(<DiffView diff={big} mode="inline" />);

    expect(screen.getByTestId("diff-render-capped")).toHaveTextContent(
      `primeras ${MAX_RENDERED_DIFF_LINES.toLocaleString()}`,
    );
    expect(screen.getByText(`line-${MAX_RENDERED_DIFF_LINES - 1}`)).toBeInTheDocument();
    expect(screen.queryByText(`line-${MAX_RENDERED_DIFF_LINES + 4}`)).not.toBeInTheDocument();
    expect(container.querySelectorAll(".diff-line")).toHaveLength(MAX_RENDERED_DIFF_LINES);
  }, 10000);

  it("renders overview markers and highlights the matching added line", () => {
    const { container } = render(
      <DiffView
        diff={sample}
        mode="inline"
        overviewMarkers={[{ line: 13, severity: "critical", label: "Possible secret" }]}
        overviewTotalLines={20}
      />,
    );

    expect(screen.getByTestId("overview-summary")).toHaveTextContent("1");
    expect(screen.getByTestId("overview-marker-13-0")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Possible secret, línea 13" })).toBeInTheDocument();
    expect(screen.getByText("new line").closest(".diff-line")).toHaveClass(
      "diff-line--signal-critical",
    );
    const label = container.querySelector('[data-new-line="13"] .line-marker-label');
    expect(label).toHaveTextContent("Possible secret");
    expect(label).toHaveAttribute("title", "Possible secret · línea 13");
  });
});
