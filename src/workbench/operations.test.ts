import { describe, it, expect, vi, beforeEach } from "vitest";
import { act } from "@testing-library/react";

const client = vi.hoisted(() => ({
  addRepo: vi.fn(),
  addWslRepo: vi.fn(),
  autodetectReposUnder: vi.fn(),
  createWorkbench: vi.fn(),
  deleteWorkbench: vi.fn(),
  fetchRepo: vi.fn(),
  forgetRepo: vi.fn(),
  getRepoFetchPreview: vi.fn(),
  listWorkbenches: vi.fn(),
  listWslDirectory: vi.fn(),
  listWslDistros: vi.fn(),
  removeRepo: vi.fn(),
  removeRepoEntry: vi.fn(),
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
  fetchRepoFlow,
} from "./operations";
import { busStore } from "../bus/store";
import type { WorkbenchConfig } from "../bus/contract";

function confirmReloadedActive(active: string): void {
  const config = busStore.getState().config;
  if (!config) throw new Error("Expected a loaded workbench config");
  busStore.setConfig({ ...config, active });
}

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
    client.fetchRepo.mockResolvedValue({ remote: "origin", host: "github.com", fetched_at_ms: 1 });
    client.forgetRepo.mockResolvedValue(undefined);
    client.getRepoFetchPreview.mockResolvedValue({
      remote: "origin",
      host: "github.com",
      sanitized_url: "https://github.com/acme/repo.git",
    });
    client.listWorkbenches.mockResolvedValue({ version: 1, active: null, workbenches: [] });
    client.removeRepo.mockResolvedValue(undefined);
    client.removeRepoEntry.mockResolvedValue(true);
    client.removeWslRepo.mockResolvedValue(undefined);
    client.renameWorkbench.mockResolvedValue(undefined);
    client.autodetectReposUnder.mockResolvedValue([]);
    dialogMock.confirm.mockResolvedValue(true);
    reloadMock.mockResolvedValue(undefined);
  });

  it("switchWorkbench activates, reloads, and no-ops once the active workbench is confirmed", async () => {
    const resetSpy = vi.spyOn(busStore, "reset");
    busStore.setConfig({
      version: 1,
      active: "Work",
      workbenches: [
        { name: "Work", repos: [] },
        { name: "Other", repos: [] },
      ],
    });
    reloadMock.mockImplementationOnce(async () => confirmReloadedActive("Other"));
    await switchWorkbench("Other", "Work");
    expect(client.setActiveWorkbench).toHaveBeenCalledWith("Other");
    expect(resetSpy).not.toHaveBeenCalled();
    expect(reloadMock).toHaveBeenCalled();
    expect(busStore.getState().config?.active).toBe("Other");

    vi.clearAllMocks();
    await switchWorkbench("Other", "Other"); // same
    await switchWorkbench("", "Work"); // empty
    expect(client.setActiveWorkbench).not.toHaveBeenCalled();
  });

  it("serializes concurrent switches in request order", async () => {
    busStore.setConfig({
      version: 1,
      active: "Work",
      workbenches: [
        { name: "Work", repos: [] },
        { name: "B", repos: [] },
        { name: "C", repos: [] },
      ],
    });
    let releaseFirst!: () => void;
    const firstActivation = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    client.setActiveWorkbench
      .mockImplementationOnce(() => firstActivation)
      .mockResolvedValueOnce(undefined);
    reloadMock
      .mockImplementationOnce(async () => confirmReloadedActive("B"))
      .mockImplementationOnce(async () => confirmReloadedActive("C"));

    const switchToB = switchWorkbench("B", "Work");
    const switchToC = switchWorkbench("C", "Work");
    await Promise.resolve();

    expect(client.setActiveWorkbench).toHaveBeenCalledTimes(1);
    expect(client.setActiveWorkbench).toHaveBeenNthCalledWith(1, "B");
    expect(reloadMock).not.toHaveBeenCalled();

    releaseFirst();
    await Promise.all([switchToB, switchToC]);

    expect(client.setActiveWorkbench).toHaveBeenNthCalledWith(2, "C");
    expect(reloadMock).toHaveBeenCalledTimes(2);
  });

  it("preserves the latest intent when switching away and immediately back", async () => {
    busStore.setConfig({
      version: 1,
      active: "A",
      workbenches: [
        { name: "A", repos: [] },
        { name: "B", repos: [] },
      ],
    });
    let releaseFirst!: () => void;
    client.setActiveWorkbench
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            releaseFirst = resolve;
          }),
      )
      .mockResolvedValueOnce(undefined);
    reloadMock
      .mockImplementationOnce(async () => confirmReloadedActive("B"))
      .mockImplementationOnce(async () => confirmReloadedActive("A"));

    const switchToB = switchWorkbench("B", "A");
    const switchBackToA = switchWorkbench("A", "A");
    await Promise.resolve();

    expect(client.setActiveWorkbench).toHaveBeenCalledTimes(1);
    expect(client.setActiveWorkbench).toHaveBeenNthCalledWith(1, "B");

    releaseFirst();
    await Promise.all([switchToB, switchBackToA]);

    expect(client.setActiveWorkbench).toHaveBeenNthCalledWith(2, "A");
    expect(reloadMock).toHaveBeenCalledTimes(2);
  });

  it("preserves the return intent when a reload does not confirm the prior switch", async () => {
    busStore.setConfig({
      version: 1,
      active: "A",
      workbenches: [
        { name: "A", repos: [] },
        { name: "B", repos: [] },
      ],
    });
    reloadMock
      .mockImplementationOnce(async () => busStore.setConfigError("reload failed"))
      .mockImplementationOnce(async () => confirmReloadedActive("A"));

    await switchWorkbench("B", "A");
    expect(busStore.getState().config?.active).toBe("A");
    expect(busStore.getState().configStatus).toBe("error");

    await switchWorkbench("A", "A");

    expect(client.setActiveWorkbench).toHaveBeenNthCalledWith(1, "B");
    expect(client.setActiveWorkbench).toHaveBeenNthCalledWith(2, "A");
    expect(reloadMock).toHaveBeenCalledTimes(2);
    expect(busStore.getState().configStatus).toBe("ready");
  });

  it("fetchRepoFlow previews, confirms host, fetches, and reloads", async () => {
    const fetched = await fetchRepoFlow("/r/api");

    expect(fetched).toBe(true);
    expect(client.getRepoFetchPreview).toHaveBeenCalledWith("/r/api");
    expect(dialogMock.confirm).toHaveBeenCalledWith(
      expect.stringContaining("Host de destino: github.com"),
      { title: "Actualizar referencias remotas", kind: "warning" },
    );
    expect(client.fetchRepo).toHaveBeenCalledWith("/r/api", "origin", "github.com", true);
    expect(reloadMock).toHaveBeenCalled();
  });

  it("fetchRepoFlow does not fetch when confirmation is declined", async () => {
    dialogMock.confirm.mockResolvedValueOnce(false);

    await expect(fetchRepoFlow("/r/api")).resolves.toBe(false);

    expect(client.fetchRepo).not.toHaveBeenCalled();
    expect(reloadMock).not.toHaveBeenCalled();
  });

  it("createAndActivate trims, creates, activates, reloads; ignores blank", async () => {
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

  it("retries activation after a partial create without creating the workbench twice", async () => {
    busStore.setConfig({
      version: 1,
      active: "Work",
      workbenches: [{ name: "Work", repos: [] }],
    });
    client.setActiveWorkbench
      .mockRejectedValueOnce(new Error("activation unavailable"))
      .mockResolvedValueOnce(undefined);

    await expect(createAndActivate("Side")).rejects.toThrow(/se cre/);
    expect(client.createWorkbench).toHaveBeenCalledTimes(1);
    expect(client.setActiveWorkbench).toHaveBeenCalledTimes(1);
    expect(reloadMock).toHaveBeenCalledTimes(1);

    await expect(createAndActivate("Side")).resolves.toBeUndefined();

    expect(client.createWorkbench).toHaveBeenCalledTimes(1);
    expect(client.setActiveWorkbench).toHaveBeenCalledTimes(2);
    expect(client.setActiveWorkbench).toHaveBeenLastCalledWith("Side");
    expect(reloadMock).toHaveBeenCalledTimes(2);
    expect(busStore.getState().config?.active).toBe("Side");
  });

  it("retries the refresh after create and activation without creating twice", async () => {
    busStore.setConfig({
      version: 1,
      active: "Work",
      workbenches: [{ name: "Work", repos: [] }],
    });
    reloadMock
      .mockRejectedValueOnce(new Error("snapshot unavailable"))
      .mockResolvedValue(undefined);

    await expect(createAndActivate("Draft")).rejects.toThrow(/actualizarse/);
    await expect(createAndActivate("Draft")).resolves.toBeUndefined();

    expect(client.createWorkbench).toHaveBeenCalledTimes(1);
    expect(client.setActiveWorkbench).toHaveBeenCalledTimes(2);
    expect(reloadMock).toHaveBeenCalledTimes(2);
    expect(busStore.getState().config?.active).toBe("Draft");
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

  it("addRepoFlow surfaces a failed add and does not pretend the reload succeeded", async () => {
    dialogMock.open.mockResolvedValueOnce("/dup");
    client.addRepo.mockRejectedValueOnce(new Error("duplicate"));
    await expect(addRepoFlow("Work")).rejects.toThrow("duplicate");
    expect(reloadMock).not.toHaveBeenCalled();
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

  it("autodetectFlow reports added and skipped repos", async () => {
    dialogMock.open.mockResolvedValueOnce("/root");
    client.autodetectReposUnder.mockResolvedValueOnce(["/root/a", "/root/b"]);
    client.addRepo.mockResolvedValueOnce("/root/a").mockRejectedValueOnce(new Error("duplicate"));
    await expect(autodetectFlow("Work")).resolves.toEqual({ found: 2, added: 1, failed: 1 });
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
    expect(client.removeRepoEntry).toHaveBeenCalledWith("Work", "/r/a");

    vi.clearAllMocks();
    dialogMock.confirm.mockResolvedValueOnce(false);
    const no = await removeRepoFlow("Work", "/r/a");
    expect(no).toBe(false);
    expect(client.removeRepoEntry).not.toHaveBeenCalled();
  });

  it("removeRepoFlow persists removal when Windows supplies a non-prefixed path", async () => {
    const storedPath = String.raw`\\?\C:\Users\User\Documents\personal\tinto`;
    const visiblePath = String.raw`C:\Users\User\Documents\personal\tinto`;
    act(() =>
      busStore.setConfig({
        version: 1,
        active: "Work",
        workbenches: [{ name: "Work", repos: [{ path: storedPath, alias: null, fs_watch: [] }] }],
      }),
    );
    dialogMock.confirm.mockResolvedValueOnce(true);

    const removed = await removeRepoFlow("Work", visiblePath);

    expect(removed).toBe(true);
    expect(client.removeRepoEntry).toHaveBeenCalledWith("Work", visiblePath);
    expect(client.forgetRepo).not.toHaveBeenCalled();
    expect(reloadMock).toHaveBeenCalledOnce();
  });

  it("removeRepoFlow forgets an orphan repo from the bus when it is not in the workbench", async () => {
    // The "no longer accessible" panel view is rendered when the repo is absent
    // from the active workbench's config; clicking Remove must still close the
    // panel and drop the repo from the live bus snapshot.
    act(() => busStore.resetAll());
    client.removeRepoEntry.mockResolvedValueOnce(false);
    dialogMock.confirm.mockResolvedValueOnce(true);
    const ok = await removeRepoFlow("Work", "/r/orphan");
    expect(ok).toBe(true);
    expect(client.forgetRepo).toHaveBeenCalledWith("/r/orphan");
    expect(client.removeRepoEntry).toHaveBeenCalledWith("Work", "/r/orphan");
    expect(reloadMock).toHaveBeenCalledOnce();
  });

  it("removeRepoFlow does not throw when the config is missing workbenches", async () => {
    // The "no longer accessible" panel view can be visible while the active
    // workbench's config arrives with `workbenches` missing (partial snapshot
    // during first-run recovery). Removal must still delegate to the backend
    // rather than inspecting the partial frontend shape.
    act(() => {
      busStore.setConfig({ version: 1, active: "Work" } as WorkbenchConfig);
    });
    client.removeRepoEntry.mockResolvedValueOnce(false);
    dialogMock.confirm.mockResolvedValueOnce(true);
    const ok = await removeRepoFlow("Work", "/r/orphan");
    expect(ok).toBe(true);
    expect(client.forgetRepo).toHaveBeenCalledWith("/r/orphan");
    expect(client.removeRepoEntry).toHaveBeenCalledWith("Work", "/r/orphan");
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
    expect(client.removeRepoEntry).toHaveBeenCalledWith("Work", "/r/a");
    confirmSpy.mockRestore();
  });

  it("removeRepoFlow delegates WSL repo removal to the unified backend command", async () => {
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
    expect(client.removeRepoEntry).toHaveBeenCalledWith("Work", "/home/me/repo");
  });

  it("removeRepoFlow delegates stale WSL identity resolution to the backend", async () => {
    act(() => busStore.resetAll());
    dialogMock.confirm.mockResolvedValueOnce(true);

    const ok = await removeRepoFlow("Work", "/home/teb");

    expect(ok).toBe(true);
    expect(client.removeRepoEntry).toHaveBeenCalledWith("Work", "/home/teb");
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
    await deleteWorkbenchFlow("Side");

    expect(client.deleteWorkbench).toHaveBeenCalledWith("Side");
    expect(client.setActiveWorkbench).toHaveBeenCalledWith("Work"); // first remaining
    expect(reloadMock).toHaveBeenCalled();

    const mru = JSON.parse(localStorage.getItem("tinto:recent-workbenches:v1") ?? "[]");
    expect(mru).not.toContain("Side");
  });

  it("retries promotion after a partial delete without deleting the workbench twice", async () => {
    act(() =>
      busStore.setConfig({
        version: 1,
        active: "Side",
        workbenches: [
          { name: "Work", repos: [] },
          { name: "Side", repos: [] },
        ],
      }),
    );
    client.setActiveWorkbench
      .mockRejectedValueOnce(new Error("promotion unavailable"))
      .mockResolvedValueOnce(undefined);

    await expect(deleteWorkbenchFlow("Side")).rejects.toThrow(/se elimin/);
    expect(client.deleteWorkbench).toHaveBeenCalledTimes(1);
    expect(client.setActiveWorkbench).toHaveBeenCalledTimes(1);
    expect(reloadMock).toHaveBeenCalledTimes(1);

    await expect(deleteWorkbenchFlow("Side")).resolves.toBeUndefined();

    expect(client.deleteWorkbench).toHaveBeenCalledTimes(1);
    expect(client.setActiveWorkbench).toHaveBeenCalledTimes(2);
    expect(client.setActiveWorkbench).toHaveBeenLastCalledWith("Work");
    expect(reloadMock).toHaveBeenCalledTimes(2);
  });

  it("retries the refresh after delete and promotion without deleting twice", async () => {
    act(() =>
      busStore.setConfig({
        version: 1,
        active: "Temporary",
        workbenches: [
          { name: "Work", repos: [] },
          { name: "Temporary", repos: [] },
        ],
      }),
    );
    reloadMock
      .mockRejectedValueOnce(new Error("snapshot unavailable"))
      .mockResolvedValue(undefined);

    await expect(deleteWorkbenchFlow("Temporary")).rejects.toThrow(/actualizarse/);
    await expect(deleteWorkbenchFlow("Temporary")).resolves.toBeUndefined();

    expect(client.deleteWorkbench).toHaveBeenCalledTimes(1);
    expect(client.setActiveWorkbench).toHaveBeenCalledTimes(2);
    expect(client.setActiveWorkbench).toHaveBeenLastCalledWith("Work");
    expect(reloadMock).toHaveBeenCalledTimes(2);
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
