import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import type { FileContent } from "../../bus/contract";

let content: FileContent = { encoding: "utf8", content: "one\ntwo\n", truncated: false };
let reject = false;

vi.mock("../../bus/client", () => ({
  getFileContent: vi.fn(() => {
    if (reject) return Promise.reject(new Error("boom"));
    return Promise.resolve(content);
  }),
}));

import { FullFileView } from "./FullFileView";

describe("FullFileView", () => {
  beforeEach(() => {
    content = { encoding: "utf8", content: "one\ntwo\n", truncated: false };
    reject = false;
    Element.prototype.scrollIntoView = vi.fn();
  });

  it("renders utf8 content, highlights changed lines, and drops one trailing newline row", async () => {
    const { container } = render(
      <FullFileView repo="/r/a" path="src/a.ts" changedLines={new Set([2])} />,
    );

    expect(await screen.findByTestId("full-file")).toBeInTheDocument();
    const rows = screen.getAllByText(/^(one|two)$/);
    expect(rows).toHaveLength(2);
    expect(container.querySelectorAll(".full-file__line")).toHaveLength(2);
    expect(screen.getByText("two").closest(".full-file__line")).toHaveClass(
      "full-file__line--changed",
    );
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
    render(<FullFileView repo="/r/a" path="large.txt" changedLines={new Set()} />);

    expect(await screen.findByText("visible")).toBeInTheDocument();
    expect(screen.getByTestId("full-truncated")).toHaveTextContent("File truncated");
  });

  it("shows an error state when file content cannot be loaded", async () => {
    reject = true;
    render(<FullFileView repo="/r/a" path="missing.ts" changedLines={new Set()} />);

    expect(await screen.findByTestId("full-error")).toHaveTextContent("Could not load file.");
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
    expect(screen.getByRole("button", { name: "Possible secret, line 2" })).toBeInTheDocument();
    expect(screen.getByText("two").closest(".full-file__line")).toHaveClass(
      "full-file__line--signal-critical",
    );
    expect(container.querySelector('[data-line="2"]')).toBeInTheDocument();
  });
});
