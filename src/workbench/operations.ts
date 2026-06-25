// Workbench curation flows (Core, RDM-007): switch, create, add (folder pick or
// autodetect), remove. All target the ACTIVE workbench by name (no implicit
// active workbench in the backend mutations) and reload the snapshot after.

import { confirm, open } from "@tauri-apps/plugin-dialog";
import {
  addRepo,
  addWslRepo,
  autodetectReposUnder,
  createWorkbench,
  forgetRepo,
  listWslDirectory,
  listWslDistros,
  removeRepo,
  removeWslRepo,
  setActiveWorkbench,
  updateRepo,
  type WslDirectoryListing,
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
  distro: string;
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

export async function listWslDistrosFlow(): Promise<string[]> {
  try {
    return await listWslDistros();
  } catch (e) {
    console.warn("tinto: list WSL distros failed", e);
    return [];
  }
}

export async function listWslDirectoryFlow(
  distro: string,
  path?: string | null,
): Promise<WslDirectoryListing | null> {
  try {
    return await listWslDirectory(distro, path ?? null);
  } catch (e) {
    console.warn("tinto: list WSL directory failed", e);
    return null;
  }
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

export async function removeRepoFlow(active: string, path: string): Promise<boolean> {
  const message = `Remove ${path} from this workbench?\nFiles are not deleted.`;
  let ok: boolean;
  try {
    ok = await confirm(message, {
      title: "Remove repo",
      kind: "warning",
      okLabel: "Remove",
      cancelLabel: "Cancel",
    });
  } catch {
    // If the Tauri dialog is unavailable (e.g. permission/capability mismatch),
    // fall back to the browser confirm so the user can still remove the repo.
    ok = window.confirm(message);
  }
  if (!ok) return false;
  const entry = findRepoEntry(active, path);
  // Repo missing from the active workbench's config (e.g. it was already removed
  // from the Dashboard, or the directory was deleted and the panel is still
  // mounted as the "no longer accessible" view). The bus snapshot may still
  // hold it, so explicitly tell the backend to drop it.
  if (!entry) {
    try {
      await forgetRepo(path);
    } catch (e) {
      console.warn("tinto: forget repo failed", e);
    }
    try {
      await reloadActiveWorkbench();
    } catch (e) {
      console.warn("tinto: reload after forget failed", e);
    }
    return true;
  }
  // Use the path stored in the config, not the bus key. When the repo directory
  // was deleted, the backend matches the stored canonical path (canonicalize now
  // fails), so the exact stored string is the one that will actually remove the
  // stale entry.
  const storedPath = entry.path;
  try {
    if (entry.source === "wsl" && entry.distro) {
      await removeWslRepo(active, entry.distro, storedPath);
    } else {
      await removeRepo(active, storedPath);
    }
  } catch (e) {
    console.warn("tinto: remove repo failed", e); // e.g. workbench changed mid-flight
    return false;
  }
  await reloadActiveWorkbench();
  return true;
}

function findRepoEntry(
  active: string,
  path: string,
): { source?: string; distro?: string | null } | null {
  const config = busStore.getState().config;
  if (!config) return null;
  // The config can arrive with `workbenches` missing in some edge paths (e.g. a
  // partial snapshot from the backend during first-run recovery — see the
  // "does not crash when config is missing workbenches" guard in MenuBar).
  // Treat a missing/empty list the same as a config without the entry.
  const wb = (config.workbenches ?? []).find((w) => w.name === active);
  if (!wb) return null;
  const normalized = normalizeRepoPath(path);
  return (
    wb.repos.find((r) => normalizeRepoPath(r.path) === normalized) ?? null
  );
}

/** Normalize a repo path for comparison across the bus key and workbench config.
 *  This only does string normalization; true filesystem canonicalization lives
 *  on the Rust side. */
function normalizeRepoPath(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/$/, "");
}

export async function updateRepoFsWatch(
  active: string,
  path: string,
  patterns: string[],
): Promise<void> {
  await updateRepo(active, path, { fsWatch: patterns });
  await reloadActiveWorkbench();
}
