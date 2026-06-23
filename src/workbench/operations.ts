// Workbench curation flows (Core, RDM-007): switch, create, add (folder pick or
// autodetect), remove. All target the ACTIVE workbench by name (no implicit
// active workbench in the backend mutations) and reload the snapshot after.

import { open } from "@tauri-apps/plugin-dialog";
import {
  addRepo,
  addWslRepo,
  autodetectReposUnder,
  createWorkbench,
  removeRepo,
  setActiveWorkbench,
  updateRepo,
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

/** Pick a folder and add it as a repo. Resolves to the stored canonical path so
 * the caller can open the new project's tab, or null if cancelled / failed. */
export async function addRepoFlow(active: string): Promise<string | null> {
  const picked = await open({ directory: true, title: "Add a repo" });
  if (typeof picked !== "string") return null; // cancelled
  let canonical: string | null = null;
  try {
    canonical = await addRepo(active, picked);
  } catch (e) {
    console.warn("tinto: add repo failed", e); // e.g. duplicate / not a git repo
  }
  await reloadActiveWorkbench();
  return canonical;
}

export interface AddWslRepoInput {
  distro: "Ubuntu";
  path: string;
  alias?: string;
}

export function normalizeWslLinuxPath(path: string): string | null {
  const original = path.trim();
  if (!original || original.includes("\\") || original.startsWith("//")) return null;
  if (/^[A-Za-z]:/.test(original) || !original.startsWith("/")) return null;
  const parts: string[] = [];
  for (const part of original.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") return null;
    parts.push(part);
  }
  return parts.length ? `/${parts.join("/")}` : "/";
}

export async function addWslRepoFlow(
  active: string,
  input: AddWslRepoInput,
): Promise<string | null> {
  const path = normalizeWslLinuxPath(input.path);
  if (!path) return null;
  let stored: string | null = null;
  try {
    stored = await addWslRepo(active, input.distro, path, input.alias?.trim() || undefined);
  } catch (e) {
    console.warn("tinto: add WSL repo failed", e);
  }
  await reloadActiveWorkbench();
  return stored;
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
  try {
    await removeRepo(active, path);
  } catch (e) {
    console.warn("tinto: remove repo failed", e); // e.g. workbench changed mid-flight
    return false;
  }
  await reloadActiveWorkbench();
  return true;
}

export async function updateRepoFsWatch(
  active: string,
  path: string,
  patterns: string[],
): Promise<void> {
  await updateRepo(active, path, { fsWatch: patterns });
  await reloadActiveWorkbench();
}
