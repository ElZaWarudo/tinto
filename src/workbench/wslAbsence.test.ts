import { describe, expect, it } from "vitest";

const rawSources = import.meta.glob("../**/*.{ts,tsx}", {
  eager: true,
  import: "default",
  query: "?raw",
}) as Record<string, string>;

const FRONTEND_RUNTIME_SURFACES = Object.entries(rawSources)
  .filter(([file]) => !/(\.test\.(ts|tsx)|\.d\.ts)$/.test(file))
  .map(([file, source]) => [file.replace(/^\.\.\//, "src/"), source] as const)
  .sort(([left], [right]) => left.localeCompare(right));

describe("RDM-001 WSL absence", () => {
  it.each(FRONTEND_RUNTIME_SURFACES)(
    "does not expose WSL UI, wrappers, settings, or empty states in %s",
    (_file, source) => {
      expect(source).not.toMatch(/\bwsl\b/i);
      expect(source).not.toMatch(/unsupported_repo_source/i);
      expect(source).not.toMatch(/tinto-agent/i);
      expect(source).not.toMatch(/tinto_agent/i);
    },
  );
});
