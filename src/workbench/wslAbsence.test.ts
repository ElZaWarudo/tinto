import { describe, expect, it } from "vitest";
import { isWindowsHost, setWindowsHostOverrideForTests } from "./platform";

const rawSources = import.meta.glob("../**/*.{ts,tsx}", {
  eager: true,
  import: "default",
  query: "?raw",
}) as Record<string, string>;

const FRONTEND_RUNTIME_SURFACES = Object.entries(rawSources)
  .filter(([file]) => !/(\.test\.(ts|tsx)|\.d\.ts)$/.test(file))
  .map(
    ([file, source]) =>
      [file.replace(/^\.\.\//, "src/").replace(/^\.\//, "src/workbench/"), source] as const,
  )
  .sort(([left], [right]) => left.localeCompare(right));

const WSL_ALLOWED_RUNTIME_FILES = new Set([
  "src/bus/client.ts",
  "src/bus/contract.ts",
  "src/bus/store.ts",
  "src/panels/agentAvailability.ts",
  "src/workbench/AddRepoDialog.tsx",
  "src/workbench/ManageWorkbenchesDialog.tsx",
  "src/workbench/MenuBar.tsx",
  "src/workbench/operations.ts",
  "src/workbench/platform.ts",
]);

describe("RDM-003 WSL absence and gating", () => {
  it.each(FRONTEND_RUNTIME_SURFACES)(
    "keeps WSL text isolated from visible runtime UI in %s",
    (file, source) => {
      if (!WSL_ALLOWED_RUNTIME_FILES.has(file)) {
        expect(source).not.toMatch(/\bwsl\b/i);
        expect(source).not.toMatch(/unsupported_repo_source/i);
      }
      expect(source).not.toMatch(/tinto-agent/i);
      expect(source).not.toMatch(/tinto_agent/i);
    },
  );

  it("platform gate is mockable for Windows and non-Windows tests", () => {
    setWindowsHostOverrideForTests(true);
    expect(isWindowsHost()).toBe(true);

    setWindowsHostOverrideForTests(false);
    expect(isWindowsHost()).toBe(false);

    setWindowsHostOverrideForTests(null);
  });
});
