import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createRef, useRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { FileOverviewRuler, type FileOverviewMarker } from "./FileOverviewRuler";

function makeBody(totalLines: number): {
  ref: React.RefObject<HTMLDivElement | null>;
  node: HTMLDivElement;
} {
  const ref = createRef<HTMLDivElement>();
  const node = document.createElement("div");
  for (let i = 1; i <= totalLines; i++) {
    const row = document.createElement("div");
    row.setAttribute("data-line", String(i));
    Object.defineProperty(row, "getBoundingClientRect", {
      value: () => ({
        top: i * 20,
        bottom: (i + 1) * 20,
        left: 0,
        right: 0,
        width: 100,
        height: 20,
        x: 0,
        y: i * 20,
        toJSON: () => "",
      }),
    });
    node.appendChild(row);
  }
  Object.defineProperty(node, "getBoundingClientRect", {
    value: () => ({
      top: 0,
      bottom: 600,
      left: 0,
      right: 0,
      width: 100,
      height: 600,
      x: 0,
      y: 0,
      toJSON: () => "",
    }),
  });
  Object.defineProperty(node, "clientHeight", {
    configurable: true,
    value: 600,
  });
  document.body.appendChild(node);
  (ref as React.MutableRefObject<HTMLDivElement | null>).current = node;
  return { ref, node };
}

describe("FileOverviewRuler", () => {
  beforeEach(() => {
    Element.prototype.scrollIntoView = vi.fn();
  });

  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("renders the track even when there are no markers", () => {
    const { ref } = makeBody(50);
    render(
      <FileOverviewRuler
        markers={[]}
        totalLines={50}
        visibleLineCount={10}
        overviewLines={["const value = 1;"]}
        bodyRef={ref}
      />,
    );
    expect(screen.getByTestId("file-overview-ruler")).toBeInTheDocument();
    expect(screen.getByTestId("file-overview-ruler-track")).toBeInTheDocument();
    expect(screen.getByTestId("file-overview-ruler-caret")).toBeInTheDocument();
    expect(screen.getByTestId("file-overview-ruler-minimap")).toBeInTheDocument();
    expect(screen.getByText("const value = 1;")).toBeInTheDocument();
    expect(screen.getByTestId("file-overview-ruler-caret")).toHaveStyle({
      height: "40px",
      transform: "translate3d(0, 0px, 0)",
    });
  });

  it("pins the minimap height to the visible scroll container", async () => {
    const { ref } = makeBody(50);
    render(<FileOverviewRuler markers={[]} totalLines={50} bodyRef={ref} />);
    await waitFor(() => {
      expect(screen.getByTestId("file-overview-ruler")).toHaveStyle({
        "--file-overview-ruler-height": "588px",
        "--file-overview-ruler-track-height": "200px",
      });
    });
  });

  it("keeps small files compact instead of stretching them to the full viewport", async () => {
    const { ref } = makeBody(2);
    render(
      <FileOverviewRuler markers={[]} totalLines={2} bodyRef={ref} overviewLines={["a", "b"]} />,
    );
    await waitFor(() => {
      expect(screen.getByTestId("file-overview-ruler")).toHaveStyle({
        "--file-overview-ruler-track-height": "96px",
      });
    });
    expect(screen.getAllByText(/^(a|b)$/)).toHaveLength(2);
  });

  it("moves the viewport marker from continuous scroll progress", async () => {
    const { ref } = makeBody(100);
    render(
      <FileOverviewRuler
        markers={[]}
        totalLines={100}
        visibleLineCount={25}
        scrollProgress={0.5}
        bodyRef={ref}
      />,
    );
    await waitFor(() => {
      expect(screen.getByTestId("file-overview-ruler-caret")).toHaveStyle({
        height: "100px",
        transform: "translate3d(0, 150px, 0)",
      });
    });
  });

  it("samples very large files instead of rendering every minimap row", () => {
    const { ref } = makeBody(5000);
    const overviewLines = Array.from({ length: 5000 }, (_, index) => `line ${index + 1}`);
    render(
      <FileOverviewRuler
        markers={[]}
        totalLines={5000}
        bodyRef={ref}
        overviewLines={overviewLines}
      />,
    );
    expect(screen.getAllByText(/^line /)).toHaveLength(600);
    expect(screen.getByText("line 1")).toBeInTheDocument();
    expect(screen.getByText("line 5000")).toBeInTheDocument();
  });

  it("hides the caret when totalLines is zero", () => {
    const { ref } = makeBody(0);
    render(<FileOverviewRuler markers={[]} totalLines={0} bodyRef={ref} />);
    expect(screen.getByTestId("file-overview-ruler")).toHaveClass("file-overview-ruler--empty");
    expect(screen.queryByTestId("file-overview-ruler-caret")).toBeNull();
  });

  it("places markers proportionally to their line number", () => {
    const { ref } = makeBody(100);
    const markers: FileOverviewMarker[] = [
      { line: 1, severity: "critical", label: "Top" },
      { line: 50, severity: "warning", label: "Middle" },
      { line: 100, severity: "info", label: "Bottom" },
    ];
    render(<FileOverviewRuler markers={markers} totalLines={100} bodyRef={ref} />);
    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(3);
    expect(buttons[0]).toHaveStyle({ top: "0%" });
    expect(buttons[2]).toHaveStyle({ top: "100%" });
  });

  it("stacks two markers that fall on the same line", () => {
    const { ref } = makeBody(50);
    const markers: FileOverviewMarker[] = [
      { line: 10, severity: "critical", label: "Alert" },
      { line: 10, severity: "info", label: "Hunk", source: "hunk" },
    ];
    render(<FileOverviewRuler markers={markers} totalLines={50} bodyRef={ref} />);
    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(2);
    expect(buttons[0]).toHaveAttribute("data-source", "alert");
    expect(buttons[1]).toHaveAttribute("data-source", "hunk");
  });

  it("summarizes marker types in the minimap legend", () => {
    const { ref } = makeBody(50);
    const markers: FileOverviewMarker[] = [
      { line: 10, severity: "critical", label: "Possible secret" },
      { line: 12, severity: "critical", label: "Possible secret" },
      { line: 20, severity: "info", label: "Changed hunk", source: "hunk" },
    ];
    render(<FileOverviewRuler markers={markers} totalLines={50} bodyRef={ref} />);
    expect(screen.getByTestId("overview-summary")).toHaveAttribute(
      "title",
      "2 posibles secretos · 1 hunk",
    );
    expect(screen.getByLabelText("2 posibles secretos")).toBeInTheDocument();
    expect(screen.getByLabelText("1 hunk")).toBeInTheDocument();
  });

  it("jumps to the line on marker click and highlights the active marker", () => {
    const { ref, node } = makeBody(100);
    const markers: FileOverviewMarker[] = [{ line: 42, severity: "critical", label: "Secret" }];
    render(<FileOverviewRuler markers={markers} totalLines={100} bodyRef={ref} />);
    const button = screen.getByRole("button");
    fireEvent.click(button);
    const target = node.querySelector('[data-line="42"]') as HTMLElement | null;
    expect(target?.scrollIntoView).toHaveBeenCalled();
    expect(button).toHaveClass("file-overview-ruler__mark--active");
    expect(button).toHaveAttribute("data-marker-line", "42");
    expect(button).not.toHaveAttribute("data-line");
  });

  it("ignores rail marker attributes when resolving a jump target", () => {
    function EmbeddedRuler() {
      const ref = useRef<HTMLDivElement>(null);
      return (
        <div ref={ref}>
          <FileOverviewRuler
            markers={[{ line: 42, severity: "critical", label: "Secret" }]}
            totalLines={100}
            bodyRef={ref}
          />
          <div data-line="42">line 42</div>
        </div>
      );
    }

    render(<EmbeddedRuler />);
    const marker = screen.getByRole("button", { name: "Secret, línea 42" });
    const target = screen.getByText("line 42");
    marker.scrollIntoView = vi.fn();
    target.scrollIntoView = vi.fn();
    fireEvent.click(marker);
    expect(target.scrollIntoView).toHaveBeenCalled();
    expect(marker.scrollIntoView).not.toHaveBeenCalled();
  });

  it("jumps to the clicked line when the track itself is clicked", () => {
    const { ref, node } = makeBody(100);
    render(<FileOverviewRuler markers={[]} totalLines={100} bodyRef={ref} />);
    const track = screen.getByTestId("file-overview-ruler-track");
    Object.defineProperty(track, "getBoundingClientRect", {
      value: () => ({
        top: 0,
        bottom: 100,
        left: 0,
        right: 24,
        width: 24,
        height: 100,
        x: 0,
        y: 0,
        toJSON: () => "",
      }),
    });
    fireEvent.click(track, { clientY: 50 });
    const target = node.querySelector('[data-line="50"]') as HTMLElement | null;
    expect(target?.scrollIntoView).toHaveBeenCalled();
  });

  it("exposes a11y attributes and hides the caret from assistive tech", () => {
    const { ref } = makeBody(50);
    render(<FileOverviewRuler markers={[]} totalLines={50} bodyRef={ref} />);
    const track = screen.getByTestId("file-overview-ruler-track");
    expect(track).toHaveAttribute("role", "slider");
    expect(track).toHaveAttribute("aria-valuemin", "1");
    expect(track).toHaveAttribute("aria-valuemax", "50");
    expect(track).toHaveAttribute("tabindex", "0");
    const caret = screen.getByTestId("file-overview-ruler-caret");
    expect(caret).toHaveAttribute("aria-hidden", "true");
  });
});
