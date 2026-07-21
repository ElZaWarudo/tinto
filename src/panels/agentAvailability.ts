import { agentProviderReadinessForRepo } from "../bus/client";
import type { AgentProviderReadiness, RepoSource } from "../bus/contract";

const AVAILABILITY_TTL_MS = 10_000;

interface CachedAvailability {
  expiresAt: number;
  promise: Promise<AgentProviderReadiness>;
}

const availabilityCache = new Map<string, CachedAvailability>();

export function agentAvailabilityKey(source?: RepoSource | null, distro?: string | null): string {
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

  const promise = agentProviderReadinessForRepo(repo, agentType).catch((error) => {
    availabilityCache.delete(cacheKey);
    throw error;
  });
  availabilityCache.set(cacheKey, {
    expiresAt: now + AVAILABILITY_TTL_MS,
    promise,
  });
  return promise;
}

export function resetAgentAvailabilityCacheForTests(): void {
  availabilityCache.clear();
}

export function invalidateAgentAvailability(environmentKey: string, agentType: string): void {
  availabilityCache.delete(`${environmentKey}:${agentType}`);
}
