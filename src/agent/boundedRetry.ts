const AGENT_RECOVERY_RETRY_DELAYS_MS = [120, 300, 700, 1_400] as const;

const NON_RETRYABLE_AGENT_ERROR_CATEGORIES = new Set([
  "acp_authentication_expired",
  "acp_limit_exceeded",
  "acp_version_unsupported",
  "agent_journal_failed",
  "attachment_not_found",
  "image_not_found",
  "invalid_input",
  "mcp_config_invalid",
  "missing_distro",
  "repository_not_found",
  "session_not_found",
  "workbench_not_active",
]);

interface AgentRecoveryRetryOptions {
  onRetry?: (nextAttempt: number, maxAttempts: number) => void;
  shouldRetry?: (error: unknown) => boolean;
  wait?: (delayMs: number) => Promise<void>;
}

function wait(delayMs: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, delayMs));
}

export async function retryAgentRecoveryOperation<T>(
  operation: () => Promise<T>,
  options: AgentRecoveryRetryOptions = {},
): Promise<T> {
  const shouldRetry = options.shouldRetry ?? isRetryableAgentRecoveryError;
  const waitForRetry = options.wait ?? wait;
  const maxAttempts = AGENT_RECOVERY_RETRY_DELAYS_MS.length + 1;
  let lastError: unknown;

  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    try {
      return await operation();
    } catch (error) {
      lastError = error;
      const delayMs = AGENT_RECOVERY_RETRY_DELAYS_MS[attempt - 1];
      if (delayMs === undefined || !shouldRetry(error)) break;
      options.onRetry?.(attempt + 1, maxAttempts);
      await waitForRetry(delayMs);
    }
  }

  throw lastError;
}

export function isRetryableAgentRecoveryError(error: unknown): boolean {
  const category = agentErrorCategory(error);
  if (!category) return true;
  if (NON_RETRYABLE_AGENT_ERROR_CATEGORIES.has(category)) return false;
  return !(
    category.startsWith("invalid_") ||
    category.startsWith("missing_") ||
    category.startsWith("unsupported_") ||
    category.endsWith("_not_found") ||
    category.includes("authentication") ||
    category.includes("permission_denied")
  );
}

export function agentErrorCategory(error: unknown): string | null {
  if (!error || typeof error !== "object" || !("category" in error)) return null;
  const category = (error as Record<string, unknown>).category;
  return typeof category === "string" ? category : null;
}
