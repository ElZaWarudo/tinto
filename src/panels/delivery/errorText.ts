/** A failure's message: Tauri commands reject with `{ category, message }`. */
export function errorText(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  if (cause && typeof cause === "object" && "message" in cause) return String(cause.message);
  return typeof cause === "string" ? cause : String(cause);
}
