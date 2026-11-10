import { describe, expect, it } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { ALL_REPOS, qualityStore } from "../qol/state";
import { DashboardFilters } from "./DashboardFilters";

describe("DashboardFilters", () => {
  it("shows a repo filter set elsewhere and clears it", () => {
    qualityStore.reset();
    render(<DashboardFilters />);
    expect(screen.queryByTestId("qol-repo")).not.toBeInTheDocument();

    act(() => qualityStore.setFilters({ repo: "/r/api" }));
    fireEvent.click(screen.getByTestId("qol-repo"));

    expect(qualityStore.getState().filters.repo).toBe(ALL_REPOS);
    expect(screen.queryByTestId("qol-repo")).not.toBeInTheDocument();
  });
});
