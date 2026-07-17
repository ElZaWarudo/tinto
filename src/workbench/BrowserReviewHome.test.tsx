import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BrowserReviewHome } from "./BrowserReviewHome";

describe("BrowserReviewHome", () => {
  it("explains browser-only limits and links both primary product intentions", () => {
    render(<BrowserReviewHome />);

    expect(
      screen.getByRole("heading", { name: "Elige una superficie para revisar" }),
    ).toBeVisible();
    expect(screen.getByText(/no ejecuta comandos Rust ni procesos de Agent/i)).toBeVisible();
    expect(screen.getByRole("link", { name: /Dashboard/ })).toHaveAttribute(
      "href",
      "/dashboard-review.html",
    );
    expect(screen.getByRole("link", { name: /Agents y Agent Lens/ })).toHaveAttribute(
      "href",
      "/agent-lens-restorable.html",
    );
    expect(screen.getByRole("link", { name: /Live Diff ruler/ })).toHaveAttribute(
      "href",
      "/demo.html",
    );
  });
});
