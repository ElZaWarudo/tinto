import { mkdirSync } from "node:fs";
import { parse, resolve } from "node:path";
import type { Options } from "@wdio/types";

function requiredEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is required. Run npm run test:e2e:tauri.`);
  }
  return value;
}

const appBinaryPath = requiredEnv("TINTO_E2E_BINARY");
const configDir = requiredEnv("TINTO_E2E_CONFIG_DIR");
const dataDir = requiredEnv("TINTO_E2E_DATA_DIR");
const homeDir = requiredEnv("TINTO_E2E_HOME_DIR");
const runtimeDir = requiredEnv("TINTO_E2E_RUNTIME_DIR");
const tempDir = requiredEnv("TINTO_E2E_TEMP_DIR");
const webviewDataDir = requiredEnv("TINTO_E2E_WEBVIEW_DATA_DIR");
const codexHomeDir = requiredEnv("TINTO_E2E_CODEX_HOME_DIR");
const homeRoot = parse(homeDir).root;
const windowsHome =
  process.platform === "win32"
    ? {
        HOMEDRIVE: homeRoot.slice(0, 2),
        HOMEPATH: homeDir.slice(2),
      }
    : {};
const appEnv = {
  ...windowsHome,
  APPDATA: configDir,
  CODEX_HOME: codexHomeDir,
  HOME: homeDir,
  LOCALAPPDATA: dataDir,
  TEMP: tempDir,
  TINTO_E2E_CONFIG_DIR: configDir,
  TINTO_E2E_DATA_DIR: dataDir,
  TINTO_E2E_HOME_DIR: homeDir,
  TINTO_E2E_WEBDRIVER: "1",
  TINTO_E2E_WEBVIEW_DATA_DIR: webviewDataDir,
  TMP: tempDir,
  TMPDIR: tempDir,
  USERPROFILE: homeDir,
  XDG_CACHE_HOME: resolve(dataDir, "cache"),
  XDG_CONFIG_HOME: configDir,
  XDG_DATA_HOME: dataDir,
  XDG_RUNTIME_DIR: runtimeDir,
  XDG_STATE_HOME: resolve(dataDir, "state"),
};

const artifactDir = resolve(process.env.TINTO_E2E_ARTIFACT_DIR ?? "artifacts/tauri-e2e");
mkdirSync(artifactDir, { recursive: true });

export const config: Options.Testrunner = {
  runner: "local",
  specs: ["./e2e/**/*.e2e.ts"],
  maxInstances: 1,
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: appBinaryPath,
      },
    },
  ],
  logLevel: "info",
  bail: 0,
  baseUrl: "tauri://localhost",
  waitforTimeout: 15_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 1,
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath,
        captureBackendLogs: false,
        captureFrontendLogs: false,
        driverProvider: "embedded",
        embeddedPort: 4445,
        env: appEnv,
      },
    ],
  ],
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 60_000,
  },
  before: async () => {
    await browser.setWindowSize(1280, 800);
  },
  afterTest: async (_test, _context, result) => {
    if (!result.passed) {
      try {
        await browser.saveScreenshot(resolve(artifactDir, "native-shell-failure.png"));
      } catch (error) {
        console.warn("Could not capture the failed Tauri E2E session", error);
      }
    }
  },
};
