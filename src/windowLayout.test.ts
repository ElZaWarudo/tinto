import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import tauriConfig from "../src-tauri/tauri.conf.json";

const appStyles = readFileSync("src/App.css", "utf8");

describe("window layout constraints", () => {
  it("keeps the native window large enough for its application chrome", () => {
    expect(tauriConfig.app.windows[0]).toMatchObject({
      minWidth: 640,
      minHeight: 480,
    });
  });

  it("reflows dashboard rows before the default window width becomes cramped", () => {
    expect(appStyles).toContain("@container dashboard (max-width: 820px)");
  });

  it("reserves enough wide-layout space for metrics and repository signals", () => {
    expect(appStyles).toMatch(
      /52px minmax\(170px, 1fr\) minmax\(96px, 0\.45fr\) minmax\(116px, 0\.75fr\)\s+minmax\(110px, 1\.1fr\) minmax\(190px, 0\.9fr\)/,
    );
    expect(appStyles).toMatch(/\.repo-card__metrics small \{[^}]*font-size: 10px;/s);
  });
});
