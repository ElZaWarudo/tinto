import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { FileOverviewRuler } from "./FileOverviewRuler";

describe("useOverviewScrollSync", () => {
  beforeEach(() => {
    window.requestAnimationFrame = (callback) => {
      callback(0);
      return 1;
    };
    window.cancelAnimationFrame = () => undefined;
  });

  it("initializes the slider with the first visible line", () => {
    render(
      <div className="file-view__body">
        <div data-line="1">one</div>
        <div data-line="2">two</div>
        <FileOverviewRuler markers={[]} totalLines={2} targetAttribute="data-line" />
      </div>,
    );

    expect(screen.getByRole("slider")).toHaveAttribute("aria-valuenow", "1");
  });
});
