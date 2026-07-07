// Per-repo file-tree cache. Trees are fetched on first project-explorer use,
// cached for the session, and served stale-while-revalidate. A cached tree stays
// on screen while a refresh runs, so re-opening a project never flashes a
// spinner.

import { useSyncExternalStore } from "react";
import { listRepoTree } from "../bus/client";
import type { RepoTree } from "../bus/contract";

export interface RepoTreeState {
  tree?: RepoTree; // last successfully loaded tree (kept across refreshes)
  loading: boolean; // a fetch is in flight
  error: boolean; // the most recent fetch failed (and no cached tree exists)
}

const EMPTY: RepoTreeState = { loading: false, error: false };

class RepoTreeStore {
  private state: Record<string, RepoTreeState> = {};
  private inflight = new Set<string>();
  private listeners = new Set<() => void>();

  getState = (): Record<string, RepoTreeState> => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  private set(repo: string, patch: Partial<RepoTreeState>) {
    this.state = { ...this.state, [repo]: { ...(this.state[repo] ?? EMPTY), ...patch } };
    this.listeners.forEach((l) => l());
  }

  get(repo: string): RepoTreeState {
    return this.state[repo] ?? EMPTY;
  }

  /** Fetch the tree (always), keeping any cached tree visible meanwhile. */
  refresh(repo: string): void {
    if (this.inflight.has(repo)) return;
    this.inflight.add(repo);
    this.set(repo, { loading: true });
    listRepoTree(repo)
      .then((tree) => this.set(repo, { tree, loading: false, error: false }))
      .catch(() => this.set(repo, { loading: false, error: !this.get(repo).tree }))
      .finally(() => this.inflight.delete(repo));
  }

  /** Fetch only if never loaded (used on project-explorer mount). */
  ensureLoaded(repo: string): void {
    const s = this.get(repo);
    if (s.tree || s.loading || this.inflight.has(repo)) return;
    this.refresh(repo);
  }

  /** Forget a repo's cached tree (e.g. it left the workbench). */
  drop(repo: string): void {
    if (!(repo in this.state)) return;
    const next = { ...this.state };
    delete next[repo];
    this.state = next;
    this.listeners.forEach((l) => l());
  }

  reset(): void {
    this.state = {};
    this.inflight.clear();
    this.listeners.forEach((l) => l());
  }
}

export const repoTreeStore = new RepoTreeStore();

/** Subscribe to one repo's cached tree, loading it on first use. */
export function useRepoTree(repo: string): RepoTreeState {
  const state = useSyncExternalStore(
    repoTreeStore.subscribe,
    () => repoTreeStore.getState()[repo] ?? EMPTY,
  );
  return state;
}
