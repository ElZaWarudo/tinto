import { agentProviderReadinessForRepo } from "../bus/client";
import type { AgentProviderReadiness, RepoSource } from "../bus/contract";

const AVAILABILITY_TTL_MS = 10_000;

interface CachedAvailability {
  expiresAt: number;
  promise: Promise<AgentProviderReadiness>;
}

const availabilityCache = new Map<string, CachedAvailability>();

export function agentAvailabilityKey(
  source?: RepoSource | null,
  distro?: string | null,
): string {
  if (source === "wsl" && distro) return `wsl:${distro}`;
  return "host";
}

export function checkAgentAvailabilityForRepo(
  repo: string,
  environmentKey: string,
  agentType: string,
  options: { force?: boolean } = {},
): Promise<AgentProviderReadiness> {
  const cacheKey = `${environmentKey}:${agentType}`;
  const now = Date.now();
  const cached = availabilityCache.get(cacheKey);
  if (!options.force && cached && cached.expiresAt > now) {
    return cached.promise;
  }

  const promise = agentProviderReadinessForRepo(repo, agentType)
    .then((readiness) => {
      const current = availabilityCache.get(cacheKey);
      if (current?.promise === promise)
        current.expiresAt = Date.now() + AVAILABILITY_TTL_MS;
      return readiness;
    })
    .catch((error) => {
      if (availabilityCache.get(cacheKey)?.promise === promise) {
        availabilityCache.delete(cacheKey);
      }
      throw error;
    });
  availabilityCache.set(cacheKey, {
    // Pending probes remain shared; only a settled result has a TTL.
    expiresAt: Infinity,
    promise,
  });
  return promise;
}

export function resetAgentAvailabilityCacheForTests(): void {
  availabilityCache.clear();
}

export function invalidateAgentAvailability(
  environmentKey: string,
  agentType: string,
): void {
  availabilityCache.delete(`${environmentKey}:${agentType}`);
}
