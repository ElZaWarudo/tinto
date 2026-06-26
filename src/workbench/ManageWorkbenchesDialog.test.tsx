import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";

const ops = vi.hoisted(() => ({
  createAndActivate: vi.fn(),
  deleteWorkbenchFlow: vi.fn(),
  renameWorkbenchFlow: vi.fn(),
  switchWorkbench: vi.fn(),
}));
vi.mock("./operations", () => ops);

const dialogMock = vi.hoisted(() => ({ confirm: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialogMock);

import { ManageWorkbenchesDialog } from "./ManageWorkbenchesDialog";
import type { WorkbenchConfig } from "../bus/contract";

const baseConfig: WorkbenchConfig = {
  version: 1,
  active: "Work",
  workbenches: [
    {
      name: "Work",
      repos: [
        { path: "/r/api", alias: "API", source: "local", distro: null, fs_watch: [] },
        { path: "/r/web", alias: null, source: "local", distro: null, fs_watch: [] },
        {
          path: "/home/me/code/service",
          alias: null,
          source: "wsl",
          distro: "Ubuntu-24.04",
          fs_watch: [],
        },
      ],
    },
    {
      name: "Side",
      repos: [{ path: "/r/lab", alias: null, source: "local", distro: null, fs_watch: [] }],
    },
    {
      name: "Client X",
      repos: [],
    },
  ],
};

describe("ManageWorkbenchesDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    ops.createAndActivate.mockResolvedValue(undefined);
    ops.deleteWorkbenchFlow.mockResolvedValue(undefined);
    ops.renameWorkbenchFlow.mockResolvedValue(undefined);
    ops.switchWorkbench.mockResolvedValue(undefined);
    dialogMock.confirm.mockResolvedValue(true);
  });

  it("renders every workbench sorted by recency and marks the active one", () => {
    // Pin MRU order: Client X first, Work second, Side last.
    localStorage.setItem(
      "tinto:recent-workbenches:v1",
      JSON.stringify(["Client X", "Work", "Side"]),
    );

    render(<ManageWorkbenchesDialog config={baseConfig} onClose={vi.fn()} />);

    const list = screen.getByTestId("manage-workbenches-list");
    const rows = within(list).getAllByTestId(/^manage-workbench-row-/);
    const names = rows.map((el) =>
      el.getAttribute("data-testid")!.replace("manage-workbench-row-", ""),
    );
    expect(names).toEqual(["Client X", "Work", "Side"]);

    expect(screen.getByTestId("manage-workbench-active-badge-Work")).toBeInTheDocument();
  });

  it("summarizes the active workbench before the list", () => {
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={vi.fn()} />);

    const summary = screen.getByTestId("manage-workbenches-active-summary");
    expect(within(summary).getByText("Work")).toBeInTheDocument();
    expect(within(summary).getByText("3 repos")).toBeInTheDocument();
    expect(within(summary).getByText("3 workbenches")).toBeInTheDocument();
    expect(within(summary).getByText("4 repos configurados")).toBeInTheDocument();
  });

  it("keeps showing the active workbench name when the workbench list is incomplete", () => {
    const partial = { version: 1, active: "Work" } as unknown as WorkbenchConfig;
    render(<ManageWorkbenchesDialog config={partial} onClose={vi.fn()} />);

    const summary = screen.getByTestId("manage-workbenches-active-summary");
    expect(within(summary).getByText("Work")).toBeInTheDocument();
    expect(within(summary).getByText("Esperando lista de repos")).toBeInTheDocument();
    expect(screen.queryByText("Sin workbench activa")).not.toBeInTheDocument();
  });

  it("lists recent workbenches when the config workbench list is incomplete", () => {
    localStorage.setItem("tinto:recent-workbenches:v1", JSON.stringify(["Side", "Work"]));
    const partial = { version: 1, active: "Work" } as unknown as WorkbenchConfig;

    render(<ManageWorkbenchesDialog config={partial} onClose={vi.fn()} />);

    expect(screen.getByTestId("manage-workbench-row-Work")).toBeInTheDocument();
    expect(screen.getByTestId("manage-workbench-row-Side")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("manage-workbench-toggle-Side"));
    expect(screen.getByTestId("manage-workbench-repos-Side")).toHaveTextContent(
      "Esperando lista de repos",
    );
  });

  it("expands the active workbench by default and hides the rest", () => {
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={vi.fn()} />);

    // Active (Work) is open, others closed.
    expect(screen.getByTestId("manage-workbench-repos-Work")).toBeInTheDocument();
    expect(screen.queryByTestId("manage-workbench-repos-Side")).not.toBeInTheDocument();
    expect(screen.queryByTestId("manage-workbench-repos-Client X")).not.toBeInTheDocument();
  });

  it("toggles a collapsed workbench open on header click and lists its repos", () => {
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={vi.fn()} />);

    fireEvent.click(screen.getByTestId("manage-workbench-toggle-Side"));
    const repos = screen.getByTestId("manage-workbench-repos-Side");
    expect(repos).toBeInTheDocument();
    // The local repo has no alias, so the label falls back to basename(/r/lab) → "lab".
    expect(within(repos).getByText("lab")).toBeInTheDocument();
  });

  it("renders an empty-state line for workbenches with no repos", () => {
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={vi.fn()} />);
    fireEvent.click(screen.getByTestId("manage-workbench-toggle-Client X"));
    expect(
      within(screen.getByTestId("manage-workbench-repos-Client X")).getByText("Sin repos."),
    ).toBeInTheDocument();
  });

  it("uses the alias when present, otherwise the basename/path", () => {
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={vi.fn()} />);
    expect(screen.getByText("API")).toBeInTheDocument();
    expect(screen.getByText("web")).toBeInTheDocument(); // basename of /r/web
    expect(screen.getByText("/r/api")).toBeInTheDocument(); // subtitle shows the raw path when alias is present
  });

  it("renders WSL repos with the same primary label shape as local repos", () => {
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={vi.fn()} />);
    const repos = screen.getByTestId("manage-workbench-repos-Work");
    expect(within(repos).getByText("service")).toBeInTheDocument();
    expect(within(repos).queryByText("Ubuntu-24.04:/home/me/code/service")).not.toBeInTheDocument();
    expect(within(repos).getByText("Ubuntu-24.04 · /home/me/code/service")).toBeInTheDocument();
  });

  it("activate button is disabled on the active workbench and calls switchWorkbench otherwise", async () => {
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={vi.fn()} />);

    const activateWork = screen.getByTestId("manage-workbench-activate-Work") as HTMLButtonElement;
    expect(activateWork.disabled).toBe(true);
    expect(activateWork.textContent).toBe("Activa");

    const activateSide = screen.getByTestId("manage-workbench-activate-Side");
    fireEvent.click(activateSide);
    expect(ops.switchWorkbench).toHaveBeenCalledWith("Side", "Work");
  });

  it("rename: clicking Renombrar opens an input; Enter commits; cancel on Escape", async () => {
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={vi.fn()} />);

    fireEvent.click(screen.getByTestId("manage-workbench-rename-Side"));
    const input = screen.getByTestId("manage-workbench-rename-input-Side") as HTMLInputElement;
    expect(input).toBeInTheDocument();
    expect(input.value).toBe("Side");

    fireEvent.change(input, { target: { value: "Hobby" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(ops.renameWorkbenchFlow).toHaveBeenCalledWith("Side", "Hobby");

    // Open a new rename, press Escape, expect the input to go away without calling the flow.
    fireEvent.click(screen.getByTestId("manage-workbench-rename-Work"));
    const input2 = screen.getByTestId("manage-workbench-rename-input-Work") as HTMLInputElement;
    fireEvent.keyDown(input2, { key: "Escape" });
    expect(screen.queryByTestId("manage-workbench-rename-input-Work")).not.toBeInTheDocument();
  });

  it("delete: confirm cancel aborts the flow; confirm OK runs the flow with a warning dialog", async () => {
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={vi.fn()} />);

    dialogMock.confirm.mockResolvedValueOnce(false);
    await act(async () => {
      fireEvent.click(screen.getByTestId("manage-workbench-delete-Side"));
      await Promise.resolve();
    });
    expect(ops.deleteWorkbenchFlow).not.toHaveBeenCalled();
    expect(dialogMock.confirm).toHaveBeenCalledWith(
      expect.stringContaining("Side"),
      expect.objectContaining({ kind: "warning" }),
    );

    dialogMock.confirm.mockResolvedValueOnce(true);
    await act(async () => {
      fireEvent.click(screen.getByTestId("manage-workbench-delete-Client X"));
      await Promise.resolve();
    });
    expect(ops.deleteWorkbenchFlow).toHaveBeenCalledWith("Client X");
  });

  it("create: typing a name and submitting calls createAndActivate and clears the input", async () => {
    const onCreated = vi.fn();
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={vi.fn()} onCreated={onCreated} />);

    const input = screen.getByTestId("manage-workbench-new-input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "  Sandbox  " } });
    fireEvent.click(screen.getByTestId("manage-workbench-new-submit"));

    expect(ops.createAndActivate).toHaveBeenCalledWith("Sandbox");
    // Input clears after a successful create.
    await act(async () => {
      await Promise.resolve();
    });
    expect((screen.getByTestId("manage-workbench-new-input") as HTMLInputElement).value).toBe("");
    expect(onCreated).toHaveBeenCalledWith("Sandbox");
  });

  it("Escape closes the modal; the backdrop click also closes it", () => {
    const onClose = vi.fn();
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={onClose} />);

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByTestId("manage-workbenches-backdrop"));
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("clicking inside the modal does not close it (stopPropagation)", () => {
    const onClose = vi.fn();
    render(<ManageWorkbenchesDialog config={baseConfig} onClose={onClose} />);

    fireEvent.click(screen.getByTestId("manage-workbenches-modal"));
    expect(onClose).not.toHaveBeenCalled();
  });

  // Regression: the live config can arrive without a `workbenches` array
  // (partial snapshot recovery, first-run races). The modal must not crash
  // and must render the create form so the user can recover.
  it("does not crash when the config is missing workbenches", () => {
    const partial = { version: 1, active: null } as unknown as WorkbenchConfig;
    expect(() =>
      render(<ManageWorkbenchesDialog config={partial} onClose={vi.fn()} />),
    ).not.toThrow();
    expect(screen.getByTestId("manage-workbench-new-input")).toBeInTheDocument();
  });
});
