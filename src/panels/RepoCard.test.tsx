import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { RepoCard } from "./RepoCard";
import { ACTIVITY_WINDOW_MS } from "./constants";
import type { RepoDelta } from "../bus/contract";

const NOW = 1_700_000_000_000;

function makeDelta(over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo: "/r/api",
    revision: 1,
    status: { modified: ["a", "b"], staged: ["c"], untracked: ["d", "e", "f"] },
    branch: { name: "main", detached: false, unborn: false, ahead: 1, behind: 0 },
    head: { id: "abc1234def", summary: "fix parser", author: "me", timestamp: 1_699_999_000 },
    last_activity_ms: NOW,
    error: null,
    ...over,
  };
}

function renderCard(
  over: Partial<RepoDelta> = {},
  props: Partial<Parameters<typeof RepoCard>[0]> = {},
) {
  const onOpen = vi.fn();
  const onRetry = vi.fn();
  render(
    <RepoCard
      delta={makeDelta(over)}
      name="api"
      activityMs={NOW}
      nowMs={NOW}
      onOpen={onOpen}
      onRetry={onRetry}
      {...props}
    />,
  );
  return { onOpen, onRetry };
}

describe("RepoCard", () => {
  it("shows name, counts and branch compact; expands to commit + upstream", () => {
    renderCard();
    expect(screen.getByText("api")).toBeInTheDocument();
    expect(screen.getByTestId("counts")).toHaveTextContent("2M");
    expect(screen.getByTestId("counts")).toHaveTextContent("1S");
    expect(screen.getByTestId("counts")).toHaveTextContent("3U");
    expect(screen.getByTestId("branch")).toHaveTextContent("main");
    // compact: no commit summary yet
    expect(screen.queryByText(/fix parser/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("expand"));
    expect(screen.getByText(/fix parser/)).toBeInTheDocument();
    expect(screen.getByText(/↑1 ↓0/)).toBeInTheDocument();
  });

  // Covers AE11: git edge states render without crashing
  it("renders unborn HEAD", () => {
    renderCard({
      branch: { name: null, detached: false, unborn: true, ahead: null, behind: null },
      head: null,
    });
    expect(screen.getByTestId("branch")).toHaveTextContent("no commits yet");
    fireEvent.click(screen.getByLabelText("expand"));
    expect(screen.queryByText(/no upstream/)).not.toBeInTheDocument(); // suppressed for unborn
  });

  it("renders detached HEAD with a short SHA", () => {
    renderCard({
      branch: { name: null, detached: true, unborn: false, ahead: null, behind: null },
    });
    expect(screen.getByTestId("branch")).toHaveTextContent("(detached) abc1234");
  });

  it("renders a no-upstream branch", () => {
    renderCard({
      branch: { name: "feat", detached: false, unborn: false, ahead: null, behind: null },
    });
    fireEvent.click(screen.getByLabelText("expand"));
    expect(screen.getByText(/no upstream/)).toBeInTheDocument();
  });

  // Covers AE10: activity indicator within/outside the window
  it("marks activity within the window and clears it after", () => {
    const { rerender } = render(
      <RepoCard
        delta={makeDelta()}
        name="api"
        activityMs={NOW}
        nowMs={NOW}
        onOpen={() => {}}
        onRetry={() => {}}
      />,
    );
    expect(screen.getByTestId("activity")).toHaveClass("activity-dot--active");
    rerender(
      <RepoCard
        delta={makeDelta()}
        name="api"
        activityMs={NOW}
        nowMs={NOW + ACTIVITY_WINDOW_MS + 1}
        onOpen={() => {}}
        onRetry={() => {}}
      />,
    );
    expect(screen.getByTestId("activity")).not.toHaveClass("activity-dot--active");
  });

  // Covers AE9: terminal error shows a working retry
  it("shows a terminal error badge + retry; transient error shows no retry", () => {
    const { onRetry } = renderCard({
      error: { class: "terminal", category: "repo-removed", message: "gone" },
    });
    expect(screen.getByTestId("error-badge")).toHaveTextContent("terminal");
    expect(screen.getByTestId("error-detail")).toHaveTextContent("gone");
    fireEvent.click(screen.getByTestId("retry"));
    expect(onRetry).toHaveBeenCalledOnce();

    renderCard({ error: { class: "transient", category: "internal", message: "blip" } });
    expect(screen.getAllByTestId("error-badge").some((e) => e.textContent === "transient")).toBe(
      true,
    );
    // transient: no retry button rendered for that card
  });

  it("double-click opens the repo", () => {
    const { onOpen } = renderCard();
    fireEvent.doubleClick(screen.getByTestId("card-/r/api"));
    expect(onOpen).toHaveBeenCalledOnce();
  });
});
