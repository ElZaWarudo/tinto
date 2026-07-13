// Workbench curation flows (Core, RDM-007): switch, create, add (folder pick or
// autodetect), remove. All target the ACTIVE workbench by name (no implicit
// active workbench in the backend mutations) and reload the snapshot after.

import { confirm, open } from "@tauri-apps/plugin-dialog";
import {
  addRepo,
  addWslRepo,
  autodetectReposUnder,
  createWorkbench,
  deleteWorkbench,
  fetchRepo,
  forgetRepo,
  getRepoFetchPreview,
  listWorkbenches,
  listWslDirectory,
  listWslDistros,
  removeRepo,
  removeWslRepo,
  renameWorkbench,
  setActiveWorkbench,
  updateRepo,
  type WslDirectoryListing,
} from "../bus/client";
import { reloadActiveWorkbench } from "../bus/connection";
import { busStore } from "../bus/store";
import type { WorkbenchConfig } from "../bus/contract";
import { forgetRecentWorkbench, markRecentWorkbench } from "./recentWorkbenches";

let workbenchMutationTail: Promise<void> = Promise.resolve();
let queuedWorkbenchMutations = 0;
let activeWorkbenchMayDiffer = false;
const createdAwaitingActivation = new Set<string>();
const deletedAwaitingPromotion = new Map<
  string,
  {
    currentActive: string | null;
    nextActive: string | null;
  }
>();

function enqueueWorkbenchMutation<T>(operation: () => Promise<T>): Promise<T> {
  queuedWorkbenchMutations += 1;
  const next = workbenchMutationTail.then(operation, operation);
  workbenchMutationTail = next.then(
    () => undefined,
    () => undefined,
  );
  return next.finally(() => {
    queuedWorkbenchMutations = Math.max(0, queuedWorkbenchMutations - 1);
  });
}

function partialMutationError(summary: string, cause: unknown): Error {
  const detail =
    cause instanceof Error && cause.message.trim()
      ? cause.message
      : typeof cause === "string" && cause.trim()
        ? cause
        : "Error desconocido.";
  return new Error(`${summary} ${detail} Reintenta para completar la operación.`);
}

async function recoverWorkbenchState(): Promise<void> {
  try {
    await reloadActiveWorkbench();
    activeWorkbenchMayDiffer = false;
  } catch {
    // The original partial-mutation error remains the actionable result.
  }
}

export async function switchWorkbench(name: string, current: string | null): Promise<void> {
  if (!name) return;
  const mustHonorQueuedIntent = queuedWorkbenchMutations > 0 || activeWorkbenchMayDiffer;
  await enqueueWorkbenchMutation(async () => {
    const liveCurrent = busStore.getState().config?.active ?? current;
    if (!mustHonorQueuedIntent && name === liveCurrent) return;
    await setActiveWorkbench(name);
    activeWorkbenchMayDiffer = true;
    await reloadActiveWorkbench();
    activeWorkbenchMayDiffer = busStore.getState().config?.active !== name;
    markRecentWorkbench(name);
  });
}

export async function createAndActivate(name: string): Promise<void> {
  const n = name.trim();
  if (!n) return;
  await enqueueWorkbenchMutation(async () => {
    const previousConfig = busStore.getState().config;
    const previous = previousConfig?.active;
    if (previous) markRecentWorkbench(previous);

    const alreadyKnown = Boolean(
      previousConfig?.workbenches?.some((workbench) => workbench.name === n),
    );
    let createdByThisFlow = createdAwaitingActivation.has(n);
    if (!alreadyKnown && !createdByThisFlow) {
      await createWorkbench(n);
      createdAwaitingActivation.add(n);
      createdByThisFlow = true;
    }

    try {
      await setActiveWorkbench(n);
      activeWorkbenchMayDiffer = true;
    } catch (error) {
      await recoverWorkbenchState();
      throw partialMutationError(
        createdByThisFlow
          ? `La workbench "${n}" se creó, pero no pudo activarse.`
          : `La workbench "${n}" existe, pero no pudo activarse.`,
        error,
      );
    }

    try {
      await reloadActiveWorkbench();
      activeWorkbenchMayDiffer = false;
    } catch (error) {
      throw partialMutationError(
        `La workbench "${n}" se creó y se activó, pero no pudo actualizarse la interfaz.`,
        error,
      );
    }
    if (createdByThisFlow) ensureCreatedWorkbenchVisible(previousConfig, n);
    createdAwaitingActivation.delete(n);
    markRecentWorkbench(n);
  });
}

function ensureCreatedWorkbenchVisible(
  previousConfig: WorkbenchConfig | null | undefined,
  createdName: string,
): void {
  const state = busStore.getState();
  const current = state.config;
  const currentNames = new Set((current?.workbenches ?? []).map((w) => w.name));
  const previousNames = (previousConfig?.workbenches ?? []).map((w) => w.name);
  const hasCreated = currentNames.has(createdName);
  const hasPrevious = previousNames.every((name) => currentNames.has(name));
  if (current?.active === createdName && hasCreated && hasPrevious) return;

  const byName = new Map<string, WorkbenchConfig["workbenches"][number]>();
  for (const wb of previousConfig?.workbenches ?? []) byName.set(wb.name, wb);
  for (const wb of current?.workbenches ?? []) byName.set(wb.name, wb);
  if (!byName.has(createdName)) byName.set(createdName, { name: createdName, repos: [] });

  busStore.loadWorkbench(
    {
      version: current?.version ?? previousConfig?.version ?? 1,
      active: createdName,
      workbenches: Array.from(byName.values()),
    },
    [],
    state.watching,
  );
}

/** Pick a folder and add it as a repo. Resolves to the stored canonical path so
 * the caller can open the new project's tab, or null when the picker is cancelled.
 * Backend failures are surfaced so the caller can explain them. */
export async function addRepoFlow(active: string): Promise<string | null> {
  const picked = await open({ directory: true, title: "Añadir repositorio" });
  if (typeof picked !== "string") return null;
  const canonical = await addRepo(active, picked);
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
  const stored = await addWslRepo(active, input.distro, path, input.alias?.trim() || undefined);
  await reloadActiveWorkbench();
  return stored;
}

export async function fetchRepoFlow(repo: string): Promise<boolean> {
  const preview = await getRepoFetchPreview(repo);
  const ok = await confirm(
    `¿Actualizar ${preview.remote} desde ${preview.sanitized_url}?\n\nHost de destino: ${preview.host}\n\nEsta acción sólo actualiza las referencias remotas; no modifica el árbol de trabajo ni el índice.`,
    { title: "Actualizar referencias remotas", kind: "warning" },
  );
  if (!ok) return false;
  await fetchRepo(repo, preview.remote, preview.host, true);
  await reloadActiveWorkbench();
  return true;
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

export interface AutodetectResult {
  found: number;
  added: number;
  failed: number;
}

export async function autodetectFlow(active: string): Promise<AutodetectResult | null> {
  const root = await open({ directory: true, title: "Detectar repositorios automáticamente…" });
  if (typeof root !== "string") return null; // cancelled
  const found = await autodetectReposUnder(root);
  let added = 0;
  let failed = 0;
  for (const p of found) {
    try {
      await addRepo(active, p);
      added += 1;
    } catch {
      failed += 1;
    }
  }
  await reloadActiveWorkbench();
  return { found: found.length, added, failed };
}

export async function removeRepoFlow(active: string, path: string): Promise<boolean> {
  const message = `¿Quitar ${path} de esta workbench?\nLos archivos no se eliminarán.`;
  let ok: boolean;
  try {
    ok = await confirm(message, {
      title: "Quitar repositorio",
      kind: "warning",
      okLabel: "Quitar",
      cancelLabel: "Cancelar",
    });
  } catch {
    // If the Tauri dialog is unavailable (e.g. permission/capability mismatch),
    // fall back to the browser confirm so the user can still remove the repo.
    ok = window.confirm(message);
  }
  if (!ok) return false;
  const entry = findRepoEntry(active, path) ?? (await refreshAndFindRepoEntry(active, path));
  // Repo missing from the active workbench's config (e.g. it was already removed
  // from the Dashboard, or the directory was deleted and the panel is still
  // mounted as the "no longer accessible" view). The bus snapshot may still
  // hold it, so explicitly tell the backend to drop it.
  if (!entry) {
    try {
      await forgetRepo(path);
      await reloadActiveWorkbench();
    } catch (e) {
      throw commandFlowError(e, "No se pudo quitar el repositorio huérfano.");
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
    throw commandFlowError(e, "No se pudo quitar el repositorio de la workbench.");
  }
  await reloadActiveWorkbench();
  return true;
}

function commandFlowError(error: unknown, fallback: string): Error {
  if (error instanceof Error && error.message.trim()) return error;
  if (typeof error === "string" && error.trim()) return new Error(error);
  return new Error(fallback);
}

function findRepoEntry(
  active: string,
  path: string,
): { path: string; source?: string; distro?: string | null } | null {
  const config = busStore.getState().config;
  if (!config) return null;
  return findRepoEntryInConfig(config, active, path);
}

async function refreshAndFindRepoEntry(
  active: string,
  path: string,
): Promise<{ path: string; source?: string; distro?: string | null } | null> {
  try {
    const config = await listWorkbenches();
    busStore.setConfig(config);
    return findRepoEntryInConfig(config, active, path);
  } catch (e) {
    console.warn("tinto: refresh config before remove failed", e);
    return null;
  }
}

function findRepoEntryInConfig(
  config: WorkbenchConfig,
  active: string,
  path: string,
): { path: string; source?: string; distro?: string | null } | null {
  // The config can arrive with `workbenches` missing in some edge paths (e.g. a
  // partial snapshot from the backend during first-run recovery — see the
  // "does not crash when config is missing workbenches" guard in MenuBar).
  // Treat a missing/empty list the same as a config without the entry.
  const wb = (config.workbenches ?? []).find((w) => w.name === active);
  if (!wb) return null;
  const normalized = normalizeRepoPath(path);
  return wb.repos.find((r) => normalizeRepoPath(r.path) === normalized) ?? null;
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
  const config = busStore.getState().config;
  if (config) {
    busStore.setConfig({
      ...config,
      workbenches: (config.workbenches ?? []).map((workbench) =>
        workbench.name !== active
          ? workbench
          : {
              ...workbench,
              repos: workbench.repos.map((repo) =>
                normalizeRepoPath(repo.path) === normalizeRepoPath(path)
                  ? { ...repo, fs_watch: patterns }
                  : repo,
              ),
            },
      ),
    });
  }
  await reloadActiveWorkbench();
}

/** Rename a workbench. `from` is the current name (must match the active
 *  config); `to` is the desired new name. No-op on empty/identical inputs. */
export async function renameWorkbenchFlow(from: string, to: string): Promise<void> {
  const next = to.trim();
  if (!next || next === from) return;
  await enqueueWorkbenchMutation(async () => {
    const activeBeforeRename = busStore.getState().config?.active;
    await renameWorkbench(from, next);
    if (activeBeforeRename === from) activeWorkbenchMayDiffer = true;
    const config = busStore.getState().config;
    if (config) {
      busStore.setConfig({
        ...config,
        active: config.active === from ? next : config.active,
        workbenches: (config.workbenches ?? []).map((workbench) =>
          workbench.name === from ? { ...workbench, name: next } : workbench,
        ),
      });
    }
    await reloadActiveWorkbench();
    activeWorkbenchMayDiffer = false;
    // The MRU entry moves under the new name; drop the old one.
    forgetRecentWorkbench(from);
    markRecentWorkbench(next);
  });
}

/** Pick the next active workbench after `name` is removed. If `name` was not
 *  active, returns the current active. Otherwise prefers the first remaining
 *  workbench, or null when none remain. Pure — does not mutate anything. */
export function pickNextActiveAfterRemove(
  current: string | null,
  removed: string,
  remaining: readonly string[],
): string | null {
  if (current !== removed) return current;
  return remaining[0] ?? null;
}

/** Delete a workbench. If it was the active one, switches to the first
 *  remaining workbench (or stays without an active one when nothing is left).
 *  Repos in the deleted workbench are dropped from the config; repos that
 *  also live in other workbenches are NOT touched. The deleted name is
 *  removed from the MRU list. */
export async function deleteWorkbenchFlow(name: string): Promise<void> {
  await enqueueWorkbenchMutation(async () => {
    let pending = deletedAwaitingPromotion.get(name);
    if (!pending) {
      const config = busStore.getState().config;
      const currentActive = config?.active ?? null;
      const remaining = (config?.workbenches ?? [])
        .map((workbench) => workbench.name)
        .filter((existing) => existing !== name);
      pending = {
        currentActive,
        nextActive: pickNextActiveAfterRemove(currentActive, name, remaining),
      };
      await deleteWorkbench(name);
      if (pending.currentActive === name) activeWorkbenchMayDiffer = true;
      deletedAwaitingPromotion.set(name, pending);
    }

    try {
      if (pending.nextActive !== null && pending.nextActive !== pending.currentActive) {
        await setActiveWorkbench(pending.nextActive);
        activeWorkbenchMayDiffer = true;
      }
    } catch (error) {
      await recoverWorkbenchState();
      const recoveredConfig = busStore.getState().config;
      const recovered =
        recoveredConfig !== null &&
        recoveredConfig.active === pending.nextActive &&
        !(recoveredConfig.workbenches ?? []).some((workbench) => workbench.name === name);
      if (!recovered) {
        throw partialMutationError(
          `La workbench "${name}" se eliminó, pero no pudo activarse la siguiente.`,
          error,
        );
      }
    }

    try {
      await reloadActiveWorkbench();
      activeWorkbenchMayDiffer = false;
    } catch (error) {
      throw partialMutationError(
        `La workbench "${name}" se eliminó, pero no pudo actualizarse la interfaz.`,
        error,
      );
    }
    deletedAwaitingPromotion.delete(name);
    forgetRecentWorkbench(name);
  });
}
