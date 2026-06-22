import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ACTIVITY_WINDOW_MS } from "./constants";
import type { RepoDelta } from "../bus/contract";

const NOW = 1_700_000_000_000;
const clientMocks = vi.hoisted(() => ({
  agentBinaryAvailable: vi.fn((...args: unknown[]) => {
    void args;
    return Promise.resolve(true);
  }),
  createRepoGitleaksConfig: vi.fn((repo: string) => {
    void repo;
    return Promise.resolve();
  }),
}));
vi.mock("../bus/client", () => ({
  agentBinaryAvailable: (...args: unknown[]) => {
    void args;
    return clientMocks.agentBinaryAvailable(...args);
  },
  createRepoGitleaksConfig: (repo: string) => clientMocks.createRepoGitleaksConfig(repo),
}));

import { RepoCard } from "./RepoCard";

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
  const onLaunch = vi.fn(() => Promise.resolve());
  render(
    <RepoCard
      delta={makeDelta(over)}
      name="api"
      activityMs={NOW}
      nowMs={NOW}
      onOpen={onOpen}
      onRetry={onRetry}
      onLaunch={onLaunch}
      {...props}
    />,
  );
  return { onOpen, onRetry, onLaunch };
}

describe("RepoCard", () => {
  beforeEach(() => {
    clientMocks.agentBinaryAvailable.mockReset();
    clientMocks.agentBinaryAvailable.mockResolvedValue(true);
    clientMocks.createRepoGitleaksConfig.mockReset();
    clientMocks.createRepoGitleaksConfig.mockResolvedValue(undefined);
  });

  it("shows name, counts, branch, upstream and the latest commit at a glance", () => {
    renderCard();
    expect(screen.getByText("api")).toBeInTheDocument();
    expect(screen.getByTestId("counts")).toHaveTextContent("2M");
    expect(screen.getByTestId("counts")).toHaveTextContent("1S");
    expect(screen.getByTestId("counts")).toHaveTextContent("3U");
    expect(screen.getByTestId("branch")).toHaveTextContent("main");
    // Everything is visible without an expand toggle.
    expect(screen.getByText(/fix parser/)).toBeInTheDocument();
    expect(screen.getByText(/↑1 ↓0/)).toBeInTheDocument();
  });

  it("shows passive metrics and the signal count", () => {
    renderCard({
      metrics: { changed_files: 2, lines_added: 10, lines_removed: 5 },
      signals: [
        {
          kind: "possible_secret",
          severity: "critical",
          path: "a",
          message: "Possible secret detected",
        },
      ],
    });
    expect(screen.getByTestId("repo-metrics")).toHaveTextContent("2 files · +10 -5");
    expect(screen.getByTestId("signal-count")).toHaveTextContent("1 signal");
    expect(screen.getByText(/Possible secret/)).toBeInTheDocument();
  });

  it("shows a per-repo Gitleaks config notice when the repo has no local config", () => {
    renderCard({ gitleaks_configured: false });
    expect(screen.getByTestId("gitleaks-config-notice-compact")).toHaveTextContent(
      "Gitleaks sin configuración local",
    );
    expect(screen.getByText(/\.gitleaks\.toml/)).toBeInTheDocument();
  });

  it("configures Gitleaks directly for the repo from the notice", async () => {
    renderCard({ gitleaks_configured: false });

    fireEvent.click(screen.getByText("Configurar"));

    expect(clientMocks.createRepoGitleaksConfig).toHaveBeenCalledWith("/r/api");
    expect(await screen.findByText("Configurado")).toBeInTheDocument();
  });

  // Covers AE11: git edge states render without crashing
  it("renders unborn HEAD with no upstream line", () => {
    renderCard({
      branch: { name: null, detached: false, unborn: true, ahead: null, behind: null },
      head: null,
    });
    expect(screen.getByTestId("branch")).toHaveTextContent("no commits yet");
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
        onLaunch={() => {}}
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
        onLaunch={() => {}}
      />,
    );
    expect(screen.getByTestId("activity")).not.toHaveClass("activity-dot--active");
  });

  // Covers AE9: terminal error shows a working retry
  it("shows a terminal error badge + retry; transient error shows no retry", () => {
    const { onOpen, onRetry } = renderCard({
      error: { class: "terminal", category: "repo-removed", message: "gone" },
    });
    expect(screen.getByTestId("error-badge")).toHaveTextContent("terminal");
    expect(screen.getByTestId("error-detail")).toHaveTextContent("gone");
    fireEvent.click(screen.getByTestId("retry"));
    expect(onRetry).toHaveBeenCalledOnce();
    expect(onOpen).not.toHaveBeenCalled(); // retry click does not open the card

    renderCard({ error: { class: "transient", category: "internal", message: "blip" } });
    expect(screen.getAllByTestId("error-badge").some((e) => e.textContent === "transient")).toBe(
      true,
    );
    // transient: no retry button rendered for that card
  });

  it("single click opens the repo", () => {
    const { onOpen } = renderCard();
    fireEvent.click(screen.getByTestId("card-/r/api"));
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it("disables launch when the selected agent binary is missing", async () => {
    clientMocks.agentBinaryAvailable.mockResolvedValue(false);
    const { onLaunch } = renderCard();

    expect(await screen.findByTestId("agent-launch-message")).toHaveTextContent("Codex not found");
    expect(screen.getByTestId("agent-launch")).toBeDisabled();
    fireEvent.click(screen.getByTestId("agent-launch"));
    expect(onLaunch).not.toHaveBeenCalled();
  });

  it("launches the selected agent without opening the card", async () => {
    const { onOpen, onLaunch } = renderCard();

    const button = await screen.findByTestId("agent-launch");
    fireEvent.click(button);

    expect(onLaunch).toHaveBeenCalledWith("codex");
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("checks availability when the selected agent changes", async () => {
    renderCard();

    fireEvent.change(screen.getByLabelText("agent type"), { target: { value: "claude" } });

    expect(clientMocks.agentBinaryAvailable).toHaveBeenCalledWith("claude");
  });
});
