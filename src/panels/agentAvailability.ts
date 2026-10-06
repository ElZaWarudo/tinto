import { agentCliUpdateStatusForRepo, agentProviderReadinessForRepo } from "../bus/client";
import type { AgentCliUpdateStatus, AgentProviderReadiness, RepoSource } from "../bus/contract";

const AVAILABILITY_TTL_MS = 10_000;
// Update checks query the npm registry, so they are kept much longer.
const CLI_UPDATE_TTL_MS = 60 * 60 * 1000;

interface CachedProbe<T> {
  expiresAt: number;
  promise: Promise<T>;
}

const availabilityCache = new Map<string, CachedProbe<AgentProviderReadiness>>();
const cliUpdateCache = new Map<string, CachedProbe<AgentCliUpdateStatus>>();

export function agentAvailabilityKey(source?: RepoSource | null, distro?: string | null): string {
  if (source === "wsl" && distro) return `wsl:${distro}`;
  return "host";
}

function cachedProbe<T>(
  cache: Map<string, CachedProbe<T>>,
  cacheKey: string,
  ttlMs: number,
  force: boolean | undefined,
  load: () => Promise<T>,
): Promise<T> {
  const now = Date.now();
  const cached = cache.get(cacheKey);
  if (!force && cached && cached.expiresAt > now) {
    return cached.promise;
  }

  let loading: Promise<T>;
  try {
    loading = load();
  } catch (error) {
    loading = Promise.reject(error);
  }
  const promise = loading
    .then((result) => {
      const current = cache.get(cacheKey);
      if (current?.promise === promise) current.expiresAt = Date.now() + ttlMs;
      return result;
    })
    .catch((error) => {
      if (cache.get(cacheKey)?.promise === promise) {
        cache.delete(cacheKey);
      }
      throw error;
    });
  cache.set(cacheKey, {
    // Pending probes remain shared; only a settled result has a TTL.
    expiresAt: Infinity,
    promise,
  });
  return promise;
}

export function checkAgentAvailabilityForRepo(
  repo: string,
  environmentKey: string,
  agentType: string,
  options: { force?: boolean } = {},
): Promise<AgentProviderReadiness> {
  return cachedProbe(
    availabilityCache,
    `${environmentKey}:${agentType}`,
    AVAILABILITY_TTL_MS,
    options.force,
    () => agentProviderReadinessForRepo(repo, agentType),
  );
}

export function checkAgentCliUpdateForRepo(
  repo: string,
  environmentKey: string,
  agentType: string,
  options: { force?: boolean } = {},
): Promise<AgentCliUpdateStatus> {
  return cachedProbe(
    cliUpdateCache,
    `${environmentKey}:${agentType}`,
    CLI_UPDATE_TTL_MS,
    options.force,
    () => agentCliUpdateStatusForRepo(repo, agentType),
  );
}

export function resetAgentAvailabilityCacheForTests(): void {
  availabilityCache.clear();
  cliUpdateCache.clear();
}

export function invalidateAgentAvailability(environmentKey: string, agentType: string): void {
  availabilityCache.delete(`${environmentKey}:${agentType}`);
  cliUpdateCache.delete(`${environmentKey}:${agentType}`);
}
