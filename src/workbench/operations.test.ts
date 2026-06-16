import { describe, it, expect, vi, beforeEach } from "vitest";

const client = vi.hoisted(() => ({
  addRepo: vi.fn(),
  autodetectReposUnder: vi.fn(),
  createWorkbench: vi.fn(),
  removeRepo: vi.fn(),
  setActiveWorkbench: vi.fn(),
}));
vi.mock("../bus/client", () => client);

const openMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openMock }));

const reloadMock = vi.hoisted(() => vi.fn());
vi.mock("../bus/connection", () => ({ reloadActiveWorkbench: reloadMock }));

import {
  switchWorkbench,
  createAndActivate,
  addRepoFlow,
  autodetectFlow,
  removeRepoFlow,
} from "./operations";
import { busStore } from "../bus/store";

describe("workbench operations", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client.addRepo.mockResolvedValue(undefined);
    client.setActiveWorkbench.mockResolvedValue(undefined);
    client.createWorkbench.mockResolvedValue(undefined);
    client.removeRepo.mockResolvedValue(undefined);
    client.autodetectReposUnder.mockResolvedValue([]);
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
    await createAndActivate("  Side  ");
    expect(client.createWorkbench).toHaveBeenCalledWith("Side");
    expect(client.setActiveWorkbench).toHaveBeenCalledWith("Side");
    expect(reloadMock).toHaveBeenCalled();

    vi.clearAllMocks();
    await createAndActivate("   ");
    expect(client.createWorkbench).not.toHaveBeenCalled();
  });

  it("addRepoFlow adds the picked folder and returns its canonical path; cancel is a no-op", async () => {
    openMock.mockResolvedValueOnce("/picked/repo");
    client.addRepo.mockResolvedValueOnce("/canon/picked/repo");
    await expect(addRepoFlow("Work")).resolves.toBe("/canon/picked/repo");
    expect(client.addRepo).toHaveBeenCalledWith("Work", "/picked/repo");
    expect(reloadMock).toHaveBeenCalled();

    vi.clearAllMocks();
    openMock.mockResolvedValueOnce(null); // cancelled
    await addRepoFlow("Work");
    expect(client.addRepo).not.toHaveBeenCalled();
    expect(reloadMock).not.toHaveBeenCalled();
  });

  it("addRepoFlow swallows a failed add (resolves null) but still reloads", async () => {
    openMock.mockResolvedValueOnce("/dup");
    client.addRepo.mockRejectedValueOnce(new Error("duplicate"));
    await expect(addRepoFlow("Work")).resolves.toBeNull();
    expect(reloadMock).toHaveBeenCalled();
  });

  it("autodetectFlow adds every detected repo", async () => {
    openMock.mockResolvedValueOnce("/root");
    client.autodetectReposUnder.mockResolvedValueOnce(["/root/a", "/root/b"]);
    await autodetectFlow("Work");
    expect(client.addRepo).toHaveBeenCalledWith("Work", "/root/a");
    expect(client.addRepo).toHaveBeenCalledWith("Work", "/root/b");
    expect(reloadMock).toHaveBeenCalled();
  });

  it("removeRepoFlow confirms then removes; declines are a no-op", async () => {
    const ok = await removeRepoFlow("Work", "/r/a", () => true);
    expect(ok).toBe(true);
    expect(client.removeRepo).toHaveBeenCalledWith("Work", "/r/a");

    vi.clearAllMocks();
    const no = await removeRepoFlow("Work", "/r/a", () => false);
    expect(no).toBe(false);
    expect(client.removeRepo).not.toHaveBeenCalled();
  });
});
