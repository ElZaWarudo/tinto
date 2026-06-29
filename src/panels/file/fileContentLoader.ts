import { getFileContent } from "../../bus/client";
import type { FileContent } from "../../bus/contract";

const RETRY_DELAYS_MS = [120, 300];

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

export async function loadFileContentWithRetry(repo: string, path: string): Promise<FileContent> {
  let lastError: unknown;
  for (let attempt = 0; attempt <= RETRY_DELAYS_MS.length; attempt += 1) {
    try {
      return await getFileContent(repo, path);
    } catch (cause) {
      lastError = cause;
      const retryDelay = RETRY_DELAYS_MS[attempt];
      if (retryDelay === undefined) break;
      await delay(retryDelay);
    }
  }
  throw lastError;
}

export function fileLoadErrorMessage(cause: unknown): string {
  if (cause && typeof cause === "object" && "message" in cause) {
    const error = cause as Record<string, unknown>;
    const message = String(error.message);
    const category = typeof error.category === "string" ? error.category : null;
    return category ? `${category}: ${message}` : message;
  }
  return String(cause);
}
