import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { FileOverviewRuler, type FileOverviewMarker } from "./FileOverviewRuler";

function renderInBody(markers: FileOverviewMarker[] = []) {
  return render(
    <div className="file-view__body">
      {Array.from({ length: 10 }, (_, index) => (
        <div key={index} data-line={index + 1}>
          line {index + 1}
        </div>
      ))}
      <FileOverviewRuler markers={markers} totalLines={10} targetAttribute="data-line" />
    </div>,
  );
}

describe("FileOverviewRuler", () => {
  beforeEach(() => {
    Element.prototype.scrollIntoView = vi.fn();
    window.requestAnimationFrame = (callback) => {
      callback(0);
      return 1;
    };
    window.cancelAnimationFrame = vi.fn();
  });

  it("renders a persistent overview when there are no markers", () => {
    renderInBody();

    expect(screen.getByTestId("file-overview-ruler")).toBeInTheDocument();
    expect(screen.getByRole("slider")).toHaveAttribute("aria-valuemax", "10");
    expect(screen.getByTestId("overview-summary")).toHaveTextContent("0");
  });

  it("renders stacked marker ids with source-specific labels", () => {
    renderInBody([
      { line: 4, severity: "critical", label: "Possible secret", source: "alert" },
      { line: 4, severity: "info", label: "Changed line", source: "hunk" },
      { line: 7, severity: "warning", label: "Search result", source: "search" },
    ]);

    expect(screen.getByTestId("overview-marker-4-0")).toHaveTextContent("Alert L4");
    expect(screen.getByTestId("overview-marker-4-1")).toHaveTextContent("Change L4");
    expect(screen.getByTestId("overview-marker-7-2")).toHaveTextContent("Search L7");
  });

  it("jumps when clicking a marker", () => {
    renderInBody([{ line: 6, severity: "critical", label: "Possible secret" }]);

    fireEvent.click(screen.getByTestId("overview-marker-6-0"));

    expect(Element.prototype.scrollIntoView).toHaveBeenCalledWith({
      block: "center",
      inline: "nearest",
    });
  });

  it("supports keyboard navigation on the track", () => {
    renderInBody();

    fireEvent.keyDown(screen.getByRole("slider"), { key: "End" });

    expect(Element.prototype.scrollIntoView).toHaveBeenCalled();
  });
});
