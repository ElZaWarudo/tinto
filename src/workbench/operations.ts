// Workbench curation flows (Core, RDM-007): switch, create, add (folder pick or
// autodetect), remove. All target the ACTIVE workbench by name (no implicit
// active workbench in the backend mutations) and reload the snapshot after.

import { open } from "@tauri-apps/plugin-dialog";
import {
  addRepo,
  autodetectReposUnder,
  createWorkbench,
  removeRepo,
  setActiveWorkbench,
} from "../bus/client";
import { reloadActiveWorkbench } from "../bus/connection";
import { busStore } from "../bus/store";

export async function switchWorkbench(name: string, current: string | null): Promise<void> {
  if (!name || name === current) return;
  await setActiveWorkbench(name);
  busStore.reset(); // clear stale repos before the new snapshot lands
  await reloadActiveWorkbench();
}

export async function createAndActivate(name: string): Promise<void> {
  const n = name.trim();
  if (!n) return;
  await createWorkbench(n);
  await setActiveWorkbench(n);
  await reloadActiveWorkbench();
}

export async function addRepoFlow(active: string): Promise<void> {
  const picked = await open({ directory: true, title: "Add a repo" });
  if (typeof picked !== "string") return; // cancelled
  try {
    await addRepo(active, picked);
  } catch (e) {
    console.warn("tinto: add repo failed", e); // e.g. duplicate / not a git repo
  }
  await reloadActiveWorkbench();
}

export async function autodetectFlow(active: string): Promise<void> {
  const root = await open({ directory: true, title: "Auto-detect repos under…" });
  if (typeof root !== "string") return; // cancelled
  const found = await autodetectReposUnder(root);
  for (const p of found) {
    try {
      await addRepo(active, p);
    } catch {
      /* skip duplicates / invalid */
    }
  }
  await reloadActiveWorkbench();
}

export async function removeRepoFlow(
  active: string,
  path: string,
  confirmFn: (msg: string) => boolean = window.confirm.bind(window),
): Promise<boolean> {
  if (!confirmFn(`Remove ${path} from this workbench? Files are not deleted.`)) return false;
  await removeRepo(active, path);
  await reloadActiveWorkbench();
  return true;
}
