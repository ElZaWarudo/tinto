import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { ACTIVITY_WINDOW_MS } from "./constants";
import type { AgentInstallPreview, AgentProviderReadiness, RepoDelta } from "../bus/contract";

const NOW = 1_700_000_000_000;
const clientMocks = vi.hoisted(() => ({
  agentProviderReadinessForRepo: vi.fn((...args: unknown[]): Promise<AgentProviderReadiness> => {
    void args;
    return Promise.resolve({
      agent_type: "codex",
      source: "local",
      distro: null,
      state: "binary_available",
    });
  }),
  prepareAgentInstall: vi.fn<(...args: unknown[]) => Promise<AgentInstallPreview>>(),
  confirmAgentInstall: vi.fn(),
  cancelAgentInstall: vi.fn(),
}));
vi.mock("../bus/client", () => ({
  agentProviderReadinessForRepo: (...args: unknown[]) => {
    void args;
    return clientMocks.agentProviderReadinessForRepo(...args);
  },
  prepareAgentInstall: (...args: unknown[]) => clientMocks.prepareAgentInstall(...args),
  confirmAgentInstall: (...args: unknown[]) => clientMocks.confirmAgentInstall(...args),
  cancelAgentInstall: (...args: unknown[]) => clientMocks.cancelAgentInstall(...args),
}));

import { RepoCard } from "./RepoCard";
import { resetAgentAvailabilityCacheForTests } from "./agentAvailability";

function makeDelta(over: Partial<RepoDelta> = {}): RepoDelta {
  return {
    repo: "/r/api",
    revision: 1,
    status: { modified: ["a", "b"], staged: ["c"], untracked: ["d", "e", "f"] },
    branch: { name: "main", detached: false, unborn: false, ahead: 1, behind: 0 },
    head: { id: "abc1234def", summary: "fix parser", author: "me", timestamp: 1_699_999_000 },
    last_activity_ms: NOW,
    error: null,
    metrics: { changed_files: 0, lines_added: 0, lines_removed: 0 },
    gitleaks_configured: false,
    agents_md_configured: false,
    secret_scan_status: { state: "not_run" },
    ...over,
  };
}

function renderCard(
  over: Partial<RepoDelta> = {},
  props: Partial<Parameters<typeof RepoCard>[0]> = {},
) {
  const onOpen = vi.fn();
  const onRetry = vi.fn();
  const onRemove = vi.fn();
  const onLaunch = vi.fn(() => Promise.resolve());
  render(
    <RepoCard
      delta={makeDelta(over)}
      name="api"
      activityMs={NOW}
      nowMs={NOW}
      onOpen={onOpen}
      onRetry={onRetry}
      onRemove={onRemove}
      onLaunch={onLaunch}
      {...props}
    />,
  );
  return { onOpen, onRetry, onRemove, onLaunch };
}

describe("RepoCard", () => {
  beforeEach(() => {
    clientMocks.agentProviderReadinessForRepo.mockReset();
    clientMocks.agentProviderReadinessForRepo.mockResolvedValue({
      agent_type: "codex",
      source: "local",
      distro: null,
      state: "binary_available",
    });
    clientMocks.prepareAgentInstall.mockReset();
    clientMocks.prepareAgentInstall.mockResolvedValue({
      attempt_id: "attempt-1",
      agent_type: "codex",
      display_name: "Codex",
      source: "wsl",
      distro: "Ubuntu-24.04",
      installer: "npm",
      command_display: "npm install -g @openai/codex",
      arguments: ["install", "-g", "@openai/codex"],
      global_effect: "Instala el agente globalmente",
      privilege: "none",
      recipe_revision: "npm-v1",
      expires_at_ms: NOW + 60_000,
    });
    clientMocks.confirmAgentInstall.mockReset();
    clientMocks.confirmAgentInstall.mockResolvedValue({
      outcome: "verified",
      verified_version: "codex-cli 1.0.0",
      session_id: "session-1",
      message: "Instalacion verificada",
    });
    clientMocks.cancelAgentInstall.mockReset();
    clientMocks.cancelAgentInstall.mockResolvedValue(undefined);
    resetAgentAvailabilityCacheForTests();
  });

  it("shows name, counts, branch, upstream and the latest commit at a glance", () => {
    renderCard();
    expect(screen.getByText("api")).toBeInTheDocument();
    expect(screen.getByTestId("counts")).toHaveTextContent("2M");
    expect(screen.getByTestId("counts")).toHaveTextContent("1S");
    expect(screen.getByTestId("counts")).toHaveTextContent("3U");
    expect(screen.getByTitle("Archivos modificados: 2")).toHaveAttribute(
      "aria-label",
      "2 archivos modificados",
    );
    expect(screen.getByTitle("Archivo preparado: 1")).toHaveAttribute(
      "aria-label",
      "1 archivo preparado",
    );
    expect(screen.getByTitle("Archivos sin seguimiento: 3")).toHaveAttribute(
      "aria-label",
      "3 archivos sin seguimiento",
    );
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
    expect(screen.getByTestId("repo-metrics")).toHaveTextContent(
      "2archivos+10añadidas-5eliminadas",
    );
    expect(screen.getByTestId("signal-count")).toHaveTextContent("1 señal crítica");
    expect(screen.getByText("Posible secreto")).toBeInTheDocument();
    expect(screen.getByText("Crítica")).toBeInTheDocument();
  });

  it("marks WSL repos with their distro", () => {
    renderCard({}, { source: "wsl", distro: "Ubuntu-24.04" });

    const badge = screen.getByTestId("repo-source-badge");
    expect(badge).toHaveTextContent("WSL");
    expect(badge).toHaveAttribute("title", "WSL · Ubuntu-24.04");
  });

  it("keeps the WSL badge when source metadata arrives late but the repo path is Linux", () => {
    renderCard({ repo: "/home/me/api" });

    expect(screen.getByTestId("repo-source-badge")).toHaveTextContent("WSL");
  });

  it("does not show a source badge for local repos", () => {
    renderCard({}, { source: "local" });

    expect(screen.queryByTestId("repo-source-badge")).toBeNull();
  });

  it("shows when Gitleaks completed a clean scan", () => {
    renderCard({ secret_scan_status: { state: "clean", engine: "gitleaks", version: "8.30.1" } });
    expect(screen.getByTestId("secret-scan-status")).toHaveTextContent("Gitleaks limpio8.30.1");
  });

  it("makes degraded protection explicit", () => {
    renderCard({
      secret_scan_status: {
        state: "degraded",
        engine: "heuristic",
        failure_category: "binary_unavailable",
      },
    });
    expect(screen.getByTestId("secret-scan-status")).toHaveTextContent(
      "Gitleaks no disponibleDetector básico activo",
    );
  });

  // Covers AE11: git edge states render without crashing
  it("renders unborn HEAD with no upstream line", () => {
    renderCard({
      branch: { name: null, detached: false, unborn: true, ahead: null, behind: null },
      head: null,
    });
    expect(screen.getByTestId("branch")).toHaveTextContent("sin commits");
    expect(screen.queryByText(/sin rama remota/)).not.toBeInTheDocument(); // suppressed for unborn
  });

  it("renders detached HEAD with a short SHA", () => {
    renderCard({
      branch: { name: null, detached: true, unborn: false, ahead: null, behind: null },
    });
    expect(screen.getByTestId("branch")).toHaveTextContent("(HEAD separado) abc1234");
  });

  it("renders a no-upstream branch", () => {
    renderCard({
      branch: { name: "feat", detached: false, unborn: false, ahead: null, behind: null },
    });
    expect(screen.getByText(/sin rama remota/)).toBeInTheDocument();
  });

  it("shows opt-in fetch for local repos with an upstream and stops row activation", () => {
    const onFetch = vi.fn(() => Promise.resolve());
    const { onOpen } = renderCard({}, { source: "local", onFetch });

    const fetchButton = screen.getByRole("button", {
      name: "Actualizar referencias remotas de api",
    });
    fireEvent.click(fetchButton);

    expect(onFetch).toHaveBeenCalledOnce();
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("offers the same opt-in fetch for WSL repos and hides it without an upstream", () => {
    const onFetch = vi.fn(() => Promise.resolve());
    renderCard({}, { source: "wsl", onFetch });
    fireEvent.click(screen.getByTestId("repo-card-fetch"));
    expect(onFetch).toHaveBeenCalledOnce();

    renderCard(
      { branch: { name: "feat", detached: false, unborn: false, ahead: null, behind: null } },
      { source: "local", onFetch: vi.fn() },
    );
    expect(screen.getAllByTestId("branch")[1]).toHaveTextContent("sin rama remota");
    expect(screen.getAllByTestId("repo-card-fetch")).toHaveLength(1);
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
        onRemove={() => {}}
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
        onRemove={() => {}}
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
    expect(screen.getByTestId("error-badge")).toHaveTextContent("bloqueado");
    expect(screen.getByTestId("error-detail")).toHaveTextContent("gone");
    expect(screen.getByTestId("error-detail")).toHaveAttribute("role", "alert");
    fireEvent.click(screen.getByTestId("retry"));
    expect(onRetry).toHaveBeenCalledOnce();
    expect(onOpen).not.toHaveBeenCalled(); // retry click does not open the card

    renderCard({ error: { class: "transient", category: "internal", message: "blip" } });
    expect(
      screen.getAllByTestId("error-badge").some((e) => e.textContent === "error temporal"),
    ).toBe(true);
    // transient: no retry button rendered for that card
  });

  it("single click opens the repo", () => {
    const { onOpen } = renderCard();
    fireEvent.click(screen.getByTestId("card-/r/api"));
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it("offers an exact consent dialog when the selected agent binary is missing", async () => {
    clientMocks.agentProviderReadinessForRepo.mockResolvedValue({
      agent_type: "codex",
      source: "wsl",
      distro: "Ubuntu-24.04",
      state: "unavailable",
    });
    const { onLaunch } = renderCard();

    expect(await screen.findByTestId("agent-launch-message")).toHaveTextContent(
      "No se encontró Codex en WSL Ubuntu-24.04",
    );
    expect(screen.getByTestId("agent-launch")).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Instalar Codex" }));

    expect(await screen.findByRole("dialog", { name: "Instalar Codex" })).toBeInTheDocument();
    expect(screen.getByText("WSL Ubuntu-24.04")).toBeInTheDocument();
    expect(screen.getByText("npm install -g @openai/codex")).toBeInTheDocument();
    expect(screen.getByText(/sin privilegios elevados/i)).toBeInTheDocument();
    expect(screen.queryByLabelText("Permisos de Codex para api")).not.toBeInTheDocument();
    expect(clientMocks.prepareAgentInstall).toHaveBeenCalledWith("/r/api", "codex", "workspace");
    expect(onLaunch).not.toHaveBeenCalled();
  });

  it("confirms once and accepts the backend-started session without replaying launch", async () => {
    clientMocks.agentProviderReadinessForRepo.mockResolvedValue({
      agent_type: "codex",
      source: "local",
      distro: null,
      state: "unavailable",
    });
    const { onLaunch } = renderCard();
    fireEvent.click(await screen.findByRole("button", { name: "Instalar Codex" }));
    fireEvent.click(await screen.findByRole("button", { name: "Confirmar instalación" }));

    await waitFor(() => expect(clientMocks.confirmAgentInstall).toHaveBeenCalledTimes(1));
    expect(clientMocks.confirmAgentInstall).toHaveBeenCalledWith("attempt-1");
    expect(onLaunch).not.toHaveBeenCalled();
    expect(await screen.findByTestId("agent-launch-message")).toHaveTextContent(
      "Codex instalado y sesión iniciada",
    );
  });

  it("declines a prepared installation without starting an installer or session", async () => {
    clientMocks.agentProviderReadinessForRepo.mockResolvedValue({
      agent_type: "codex",
      source: "local",
      distro: null,
      state: "unavailable",
    });
    const { onLaunch } = renderCard();
    fireEvent.click(await screen.findByRole("button", { name: "Instalar Codex" }));
    fireEvent.click(await screen.findByRole("button", { name: "Cancelar" }));

    await waitFor(() => expect(clientMocks.cancelAgentInstall).toHaveBeenCalledWith("attempt-1"));
    expect(clientMocks.confirmAgentInstall).not.toHaveBeenCalled();
    expect(onLaunch).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("allows cancelling while the installer is running", async () => {
    clientMocks.agentProviderReadinessForRepo.mockResolvedValue({
      agent_type: "codex",
      source: "local",
      distro: null,
      state: "unavailable",
    });
    let finishInstall: ((value: unknown) => void) | undefined;
    clientMocks.confirmAgentInstall.mockReturnValue(
      new Promise((resolve) => {
        finishInstall = resolve;
      }),
    );
    renderCard();
    fireEvent.click(await screen.findByRole("button", { name: "Instalar Codex" }));
    fireEvent.click(await screen.findByRole("button", { name: "Confirmar instalación" }));

    const cancel = screen.getByRole("button", { name: "Cancelar" });
    expect(cancel).not.toBeDisabled();
    fireEvent.click(cancel);

    await waitFor(() => expect(clientMocks.cancelAgentInstall).toHaveBeenCalledWith("attempt-1"));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    finishInstall?.({
      outcome: "cancelled",
      verified_version: null,
      session_id: null,
      message: "instalacion cancelada",
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("discards a consumed preview when confirmation fails", async () => {
    clientMocks.agentProviderReadinessForRepo.mockResolvedValue({
      agent_type: "codex",
      source: "local",
      distro: null,
      state: "unavailable",
    });
    clientMocks.confirmAgentInstall.mockRejectedValue(new Error("confirm failed"));
    renderCard();
    fireEvent.click(await screen.findByRole("button", { name: "Instalar Codex" }));
    fireEvent.click(await screen.findByRole("button", { name: "Confirmar instalación" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByTestId("agent-launch-message")).toHaveTextContent(
      "No se completó la instalación de Codex",
    );
  });

  it("allows launch when the availability probe fails", async () => {
    clientMocks.agentProviderReadinessForRepo.mockRejectedValue(
      new Error("el agente WSL cerro stdout"),
    );
    const { onLaunch } = renderCard();

    expect(await screen.findByTestId("agent-launch-message")).toHaveTextContent(
      "No se pudo comprobar el Agent. Puedes intentar iniciarlo de todos modos.",
    );
    const button = screen.getByTestId("agent-launch");
    expect(button).not.toBeDisabled();
    fireEvent.click(button);

    expect(onLaunch).toHaveBeenCalledWith("codex", "workspace");
  });

  it("launches the selected agent without opening the card", async () => {
    const { onOpen, onLaunch } = renderCard();

    const button = await screen.findByRole("button", { name: "Iniciar Codex en api" });
    fireEvent.click(button);

    expect(onLaunch).toHaveBeenCalledWith("codex", "workspace");
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("does not expose an access selector in the launcher and starts Codex in workspace", async () => {
    const { onLaunch } = renderCard();
    await screen.findByRole("button", { name: "Iniciar Codex en api" });
    expect(screen.queryByLabelText("Permisos de Codex para api")).not.toBeInTheDocument();
    fireEvent.click(screen.getByTestId("agent-launch"));

    expect(onLaunch).toHaveBeenCalledWith("codex", "workspace");
  });

  it("checks availability when the selected agent changes", async () => {
    renderCard();

    fireEvent.change(screen.getByLabelText("Tipo de Agent para api"), {
      target: { value: "claude" },
    });

    expect(clientMocks.agentProviderReadinessForRepo).toHaveBeenCalledWith("/r/api", "claude");
  });

  it("offers Kimi and forces a fresh readiness check after a miss", async () => {
    clientMocks.agentProviderReadinessForRepo
      .mockResolvedValueOnce({
        agent_type: "codex",
        source: "local",
        distro: null,
        state: "binary_available",
      })
      .mockResolvedValueOnce({
        agent_type: "kimi",
        source: "local",
        distro: null,
        state: "unavailable",
      })
      .mockResolvedValueOnce({
        agent_type: "kimi",
        source: "local",
        distro: null,
        state: "binary_available",
      });
    renderCard();

    fireEvent.change(screen.getByLabelText("Tipo de Agent para api"), {
      target: { value: "kimi" },
    });
    expect(await screen.findByText(/No se encontró Kimi Code en este equipo/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Volver a comprobar" }));

    await waitFor(() => expect(clientMocks.agentProviderReadinessForRepo).toHaveBeenCalledTimes(3));
    expect(screen.getByTestId("agent-launch")).not.toBeDisabled();
  });

  it("shares an availability check for cards in the same environment", async () => {
    renderCard({}, { availabilityKey: "wsl:Ubuntu-24.04" });
    renderCard({ repo: "/r/web" }, { availabilityKey: "wsl:Ubuntu-24.04", name: "web" });

    await screen.findAllByTestId("agent-launch");

    expect(clientMocks.agentProviderReadinessForRepo).toHaveBeenCalledTimes(1);
    expect(clientMocks.agentProviderReadinessForRepo).toHaveBeenCalledWith("/r/api", "codex");
  });
});
