import { describe, it, expect, vi, beforeEach } from "vitest";
import { act } from "@testing-library/react";

const client = vi.hoisted(() => ({
  addRepo: vi.fn(),
  addWslRepo: vi.fn(),
  autodetectReposUnder: vi.fn(),
  createWorkbench: vi.fn(),
  deleteWorkbench: vi.fn(),
  forgetRepo: vi.fn(),
  listWorkbenches: vi.fn(),
  listWslDirectory: vi.fn(),
  listWslDistros: vi.fn(),
  removeRepo: vi.fn(),
  removeWslRepo: vi.fn(),
  renameWorkbench: vi.fn(),
  setActiveWorkbench: vi.fn(),
}));
vi.mock("../bus/client", () => client);

const dialogMock = vi.hoisted(() => ({ open: vi.fn(), confirm: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialogMock);

const reloadMock = vi.hoisted(() => vi.fn());
vi.mock("../bus/connection", () => ({ reloadActiveWorkbench: reloadMock }));

import {
  switchWorkbench,
  createAndActivate,
  addRepoFlow,
  addWslRepoFlow,
  autodetectFlow,
  listWslDirectoryFlow,
  listWslDistrosFlow,
  normalizeWslLinuxPath,
  pickNextActiveAfterRemove,
  removeRepoFlow,
  renameWorkbenchFlow,
  deleteWorkbenchFlow,
} from "./operations";
import { busStore } from "../bus/store";
import type { WorkbenchConfig } from "../bus/contract";

describe("workbench operations", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    busStore.resetAll();
    client.addRepo.mockResolvedValue(undefined);
    client.addWslRepo.mockResolvedValue("/home/me/repo");
    client.listWslDistros.mockResolvedValue(["Ubuntu-24.04"]);
    client.listWslDirectory.mockResolvedValue({
      path: "/home/me",
      is_git_repo: false,
      entries: [{ name: "repo", path: "/home/me/repo" }],
    });
    client.setActiveWorkbench.mockResolvedValue(undefined);
    client.createWorkbench.mockResolvedValue(undefined);
    client.deleteWorkbench.mockResolvedValue(undefined);
    client.forgetRepo.mockResolvedValue(undefined);
    client.listWorkbenches.mockResolvedValue({ version: 1, active: null, workbenches: [] });
    client.removeRepo.mockResolvedValue(undefined);
    client.removeWslRepo.mockResolvedValue(undefined);
    client.renameWorkbench.mockResolvedValue(undefined);
    client.autodetectReposUnder.mockResolvedValue([]);
    dialogMock.confirm.mockResolvedValue(true);
    reloadMock.mockResolvedValue(undefined);
  });

  it("switchWorkbench activates, resets, and reloads; no-op on same/empty", async () => {
    const resetSpy = vi.spyOn(busStore, "reset");
    await switchWorkbench("Other", "Work");
    expect(client.setActiveWorkbench).toHaveBeenCalledWith("Other");
    expect(resetSpy).toHaveBeenCalled();
    expect(reloadMock).toHaveBeenCalled();

    vi.clearAllMocks();
    await switchWorkbench("Work", "Work"); // same
    await switchWorkbench("", "Work"); // empty
    expect(client.setActiveWorkbench).not.toHaveBeenCalled();
  });

  it("createAndActivate trims, creates, activates, reloads; ignores blank", async () => {
    const resetSpy = vi.spyOn(busStore, "reset");
    busStore.setConfig({
      version: 1,
      active: "Work",
      workbenches: [
        { name: "Work", repos: [] },
        { name: "Client", repos: [] },
      ],
    });
    await createAndActivate("  Side  ");
    expect(client.createWorkbench).toHaveBeenCalledWith("Side");
    expect(client.setActiveWorkbench).toHaveBeenCalledWith("Side");
    expect(resetSpy).toHaveBeenCalled();
    expect(reloadMock).toHaveBeenCalled();
    expect(busStore.getState().config).toEqual({
      version: 1,
      active: "Side",
      workbenches: [
        { name: "Work", repos: [] },
        { name: "Client", repos: [] },
        { name: "Side", repos: [] },
      ],
    });
    expect(JSON.parse(localStorage.getItem("tinto:recent-workbenches:v1") ?? "[]")).toEqual([
      "Side",
      "Work",
    ]);

    vi.clearAllMocks();
    await createAndActivate("   ");
    expect(client.createWorkbench).not.toHaveBeenCalled();
  });

  it("addRepoFlow adds the picked folder and returns its canonical path; cancel is a no-op", async () => {
    dialogMock.open.mockResolvedValueOnce("/picked/repo");
    client.addRepo.mockResolvedValueOnce("/canon/picked/repo");
    await expect(addRepoFlow("Work")).resolves.toBe("/canon/picked/repo");
    expect(client.addRepo).toHaveBeenCalledWith("Work", "/picked/repo");
    expect(reloadMock).toHaveBeenCalled();

    vi.clearAllMocks();
    dialogMock.open.mockResolvedValueOnce(null); // cancelled
    await addRepoFlow("Work");
    expect(client.addRepo).not.toHaveBeenCalled();
    expect(reloadMock).not.toHaveBeenCalled();
  });

  it("addRepoFlow swallows a failed add (resolves null) but still reloads", async () => {
    dialogMock.open.mockResolvedValueOnce("/dup");
    client.addRepo.mockRejectedValueOnce(new Error("duplicate"));
    await expect(addRepoFlow("Work")).resolves.toBeNull();
    expect(reloadMock).toHaveBeenCalled();
  });

  it("normalizes WSL Linux paths and rejects Windows/relative forms", () => {
    expect(normalizeWslLinuxPath(" /home/me//repo/ ")).toBe("/home/me/repo");
    expect(normalizeWslLinuxPath("relative/repo")).toBeNull();
    expect(normalizeWslLinuxPath("C:\\repo")).toBeNull();
    expect(normalizeWslLinuxPath("\\\\wsl$\\Ubuntu\\repo")).toBeNull();
    expect(normalizeWslLinuxPath("/home/../repo")).toBeNull();
  });

  it("addWslRepoFlow invokes the isolated WSL wrapper and reloads", async () => {
    await expect(
      addWslRepoFlow("Work", {
        distro: "Ubuntu-24.04",
        path: " /home/me//repo/ ",
        alias: " API ",
      }),
    ).resolves.toBe("/home/me/repo");

    expect(client.addWslRepo).toHaveBeenCalledWith("Work", "Ubuntu-24.04", "/home/me/repo", "API");
    expect(reloadMock).toHaveBeenCalled();
  });

  it("addWslRepoFlow surfaces backend failures so the UI can explain them", async () => {
    client.addWslRepo.mockRejectedValueOnce(new Error("unsupported_wsl_distro"));

    await expect(
      addWslRepoFlow("Work", {
        distro: "Ubuntu",
        path: "/home/me/repo",
      }),
    ).rejects.toThrow("unsupported_wsl_distro");

    expect(reloadMock).not.toHaveBeenCalled();
  });

  it("addWslRepoFlow rejects invalid input before invoking backend", async () => {
    await expect(
      addWslRepoFlow("Work", { distro: "Ubuntu", path: "relative/repo" }),
    ).resolves.toBeNull();

    expect(client.addWslRepo).not.toHaveBeenCalled();
    expect(reloadMock).not.toHaveBeenCalled();
  });

  it("lists WSL distros and directories through backend wrappers", async () => {
    await expect(listWslDistrosFlow()).resolves.toEqual(["Ubuntu-24.04"]);
    await expect(listWslDirectoryFlow("Ubuntu-24.04", "/home/me")).resolves.toEqual({
      path: "/home/me",
      is_git_repo: false,
      entries: [{ name: "repo", path: "/home/me/repo" }],
    });
    expect(client.listWslDistros).toHaveBeenCalledOnce();
    expect(client.listWslDirectory).toHaveBeenCalledWith("Ubuntu-24.04", "/home/me");
  });

  it("autodetectFlow adds every detected repo", async () => {
    dialogMock.open.mockResolvedValueOnce("/root");
    client.autodetectReposUnder.mockResolvedValueOnce(["/root/a", "/root/b"]);
    await autodetectFlow("Work");
    expect(client.addRepo).toHaveBeenCalledWith("Work", "/root/a");
    expect(client.addRepo).toHaveBeenCalledWith("Work", "/root/b");
    expect(reloadMock).toHaveBeenCalled();
  });

  it("removeRepoFlow confirms then removes; declines are a no-op", async () => {
    act(() =>
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [{ path: "/r/a", alias: null, fs_watch: [] }] }],
      }),
    );
    dialogMock.confirm.mockResolvedValueOnce(true);
    const ok = await removeRepoFlow("Work", "/r/a");
    expect(ok).toBe(true);
    expect(client.removeRepo).toHaveBeenCalledWith("Work", "/r/a");

    vi.clearAllMocks();
    dialogMock.confirm.mockResolvedValueOnce(false);
    const no = await removeRepoFlow("Work", "/r/a");
    expect(no).toBe(false);
    expect(client.removeRepo).not.toHaveBeenCalled();
  });

  it("removeRepoFlow forgets an orphan repo from the bus when it is not in the workbench", async () => {
    // The "no longer accessible" panel view is rendered when the repo is absent
    // from the active workbench's config; clicking Remove must still close the
    // panel and drop the repo from the live bus snapshot.
    act(() => busStore.resetAll());
    dialogMock.confirm.mockResolvedValueOnce(true);
    const ok = await removeRepoFlow("Work", "/r/orphan");
    expect(ok).toBe(true);
    expect(client.forgetRepo).toHaveBeenCalledWith("/r/orphan");
    expect(client.removeRepo).not.toHaveBeenCalled();
    expect(client.removeWslRepo).not.toHaveBeenCalled();
    expect(reloadMock).toHaveBeenCalledOnce();
  });

  it("removeRepoFlow does not throw when the config is missing workbenches", async () => {
    // The "no longer accessible" panel view can be visible while the active
    // workbench's config arrives with `workbenches` missing (partial snapshot
    // during first-run recovery). MenuBar already tolerates this; findRepoEntry
    // must do the same so the Remove click does not surface an unhandled
    // TypeError into the console.
    act(() => {
      busStore.setConfig({ version: 1, active: "Work" } as WorkbenchConfig);
    });
    dialogMock.confirm.mockResolvedValueOnce(true);
    const ok = await removeRepoFlow("Work", "/r/orphan");
    expect(ok).toBe(true);
    expect(client.forgetRepo).toHaveBeenCalledWith("/r/orphan");
    expect(client.removeRepo).not.toHaveBeenCalled();
    expect(client.removeWslRepo).not.toHaveBeenCalled();
    expect(reloadMock).toHaveBeenCalledOnce();
  });

  it("removeRepoFlow falls back to window.confirm when the Tauri dialog fails", async () => {
    act(() =>
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [{ path: "/r/a", alias: null, fs_watch: [] }] }],
      }),
    );
    dialogMock.confirm.mockRejectedValueOnce(new Error("permission denied"));
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const ok = await removeRepoFlow("Work", "/r/a");
    expect(ok).toBe(true);
    expect(confirmSpy).toHaveBeenCalled();
    expect(client.removeRepo).toHaveBeenCalledWith("Work", "/r/a");
    confirmSpy.mockRestore();
  });

  it("removeRepoFlow routes WSL repos through removeWslRepo", async () => {
    act(() =>
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [
          {
            name: "Work",
            repos: [
              {
                path: "/home/me/repo",
                alias: null,
                fs_watch: [],
                source: "wsl",
                distro: "Ubuntu-24.04",
              },
            ],
          },
        ],
      }),
    );
    dialogMock.confirm.mockResolvedValueOnce(true);
    const ok = await removeRepoFlow("Work", "/home/me/repo");
    expect(ok).toBe(true);
    expect(client.removeWslRepo).toHaveBeenCalledWith("Work", "Ubuntu-24.04", "/home/me/repo");
    expect(client.removeRepo).not.toHaveBeenCalled();
  });

  it("removeRepoFlow refreshes config before treating a WSL repo as an orphan", async () => {
    act(() => busStore.resetAll());
    client.listWorkbenches.mockResolvedValueOnce({
      version: 1,
      active: "Work",
      workbenches: [
        {
          name: "Work",
          repos: [
            {
              path: "/home/teb",
              alias: null,
              fs_watch: [],
              source: "wsl",
              distro: "Ubuntu",
            },
          ],
        },
      ],
    });
    dialogMock.confirm.mockResolvedValueOnce(true);

    const ok = await removeRepoFlow("Work", "/home/teb");

    expect(ok).toBe(true);
    expect(client.listWorkbenches).toHaveBeenCalledOnce();
    expect(client.removeWslRepo).toHaveBeenCalledWith("Work", "Ubuntu", "/home/teb");
    expect(client.forgetRepo).not.toHaveBeenCalled();
  });

  it("renameWorkbenchFlow trims, calls the backend, reloads, updates MRU", async () => {
    localStorage.setItem(
      "tinto:recent-workbenches:v1",
      JSON.stringify(["Work", "Side", "Client X"]),
    );
    await renameWorkbenchFlow("Work", "  Job  ");
    expect(client.renameWorkbench).toHaveBeenCalledWith("Work", "Job");
    expect(reloadMock).toHaveBeenCalled();

    const mru = JSON.parse(localStorage.getItem("tinto:recent-workbenches:v1") ?? "[]");
    expect(mru).toContain("Job");
    expect(mru).not.toContain("Work");
  });

  it("renameWorkbenchFlow no-ops on empty input or unchanged name", async () => {
    await renameWorkbenchFlow("Work", "   ");
    await renameWorkbenchFlow("Work", "Work");
    expect(client.renameWorkbench).not.toHaveBeenCalled();
    expect(reloadMock).not.toHaveBeenCalled();
  });

  it("pickNextActiveAfterRemove prefers the first remaining when the removed was active", () => {
    expect(pickNextActiveAfterRemove("Work", "Work", ["Side", "Client X"])).toBe("Side");
    expect(pickNextActiveAfterRemove("Work", "Work", [])).toBeNull();
  });

  it("pickNextActiveAfterRemove keeps the current active when a different one is removed", () => {
    expect(pickNextActiveAfterRemove("Work", "Side", ["Work", "Client X"])).toBe("Work");
  });

  it("deleteWorkbenchFlow removes the workbench, switches to the first remaining, reloads", async () => {
    act(() =>
      busStore.setConfig({
        version: 1,
        active: "Side",
        workbenches: [
          { name: "Work", repos: [] },
          { name: "Side", repos: [] },
          { name: "Client X", repos: [] },
        ],
      }),
    );
    localStorage.setItem(
      "tinto:recent-workbenches:v1",
      JSON.stringify(["Work", "Side", "Client X"]),
    );
    const resetSpy = vi.spyOn(busStore, "reset");

    await deleteWorkbenchFlow("Side");

    expect(client.deleteWorkbench).toHaveBeenCalledWith("Side");
    expect(client.setActiveWorkbench).toHaveBeenCalledWith("Work"); // first remaining
    expect(resetSpy).toHaveBeenCalled();
    expect(reloadMock).toHaveBeenCalled();

    const mru = JSON.parse(localStorage.getItem("tinto:recent-workbenches:v1") ?? "[]");
    expect(mru).not.toContain("Side");
  });

  it("deleteWorkbenchFlow of a non-active workbench does not change the active", async () => {
    act(() =>
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [
          { name: "Work", repos: [] },
          { name: "Side", repos: [] },
        ],
      }),
    );

    await deleteWorkbenchFlow("Side");

    expect(client.deleteWorkbench).toHaveBeenCalledWith("Side");
    expect(client.setActiveWorkbench).not.toHaveBeenCalled();
  });

  it("deleteWorkbenchFlow of the only remaining workbench does not promote anyone", async () => {
    act(() =>
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [] }],
      }),
    );

    await deleteWorkbenchFlow("Work");

    expect(client.deleteWorkbench).toHaveBeenCalledWith("Work");
    expect(client.setActiveWorkbench).not.toHaveBeenCalled();
  });
});
