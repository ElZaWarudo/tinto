import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";

import { removeWithRetry } from "./remove-with-retry.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const stateDir = mkdtempSync(join(tmpdir(), "tinto-e2e-"));
const homeDir = join(stateDir, "home");
const codexHomeDir = join(homeDir, ".codex");
const dataDir = join(stateDir, "data");
const runtimeDir = join(stateDir, "runtime");
const appTempDir = join(stateDir, "tmp");
const webviewDataDir = join(stateDir, "webview");
const artifactDir = resolve(repoRoot, "artifacts", "tauri-e2e");
const targetDir = resolve(repoRoot, "src-tauri", "target", "e2e-wdio");
const binaryName = process.platform === "win32" ? "tinto.exe" : "tinto";
const binaryPath = resolve(targetDir, "debug", binaryName);

mkdirSync(artifactDir, { recursive: true });
mkdirSync(codexHomeDir, { recursive: true });
mkdirSync(dataDir, { recursive: true });
mkdirSync(runtimeDir, { recursive: true, mode: 0o700 });
mkdirSync(appTempDir, { recursive: true });
mkdirSync(webviewDataDir, { recursive: true });
rmSync(resolve(artifactDir, "native-shell-failure.png"), { force: true });

const env = {
  ...process.env,
  CARGO_TARGET_DIR: targetDir,
  TINTO_E2E_ARTIFACT_DIR: artifactDir,
  TINTO_E2E_BINARY: binaryPath,
  TINTO_E2E_CONFIG_DIR: stateDir,
  TINTO_E2E_CODEX_HOME_DIR: codexHomeDir,
  TINTO_E2E_DATA_DIR: dataDir,
  TINTO_E2E_HOME_DIR: homeDir,
  TINTO_E2E_RUNTIME_DIR: runtimeDir,
  TINTO_E2E_TEMP_DIR: appTempDir,
  TINTO_E2E_WEBVIEW_DATA_DIR: webviewDataDir,
};

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    env,
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} exited with ${result.status ?? "no status"}`);
  }
}

async function removeIsolatedState() {
  const resolvedStateDir = resolve(stateDir);
  const resolvedTempDir = resolve(tmpdir());
  const isDirectTempChild = dirname(resolvedStateDir) === resolvedTempDir;
  const hasExpectedPrefix = basename(resolvedStateDir).startsWith("tinto-e2e-");
  if (!isDirectTempChild || !hasExpectedPrefix) {
    throw new Error(`Refusing to remove unexpected E2E state directory: ${resolvedStateDir}`);
  }
  await removeWithRetry({
    attempts: process.platform === "win32" ? 10 : 1,
    delay: () => delay(250),
    remove: () => rmSync(resolvedStateDir, { recursive: true, force: true }),
  });
}

function removeE2eExecutable() {
  const expectedDir = resolve(targetDir, "debug");
  const resolvedBinary = resolve(binaryPath);
  if (dirname(resolvedBinary) !== expectedDir || basename(resolvedBinary) !== binaryName) {
    throw new Error(`Refusing to remove unexpected E2E binary: ${resolvedBinary}`);
  }
  rmSync(resolvedBinary, { force: true });
  rmSync(resolve(expectedDir, "deps", binaryName), { force: true });
  if (process.platform === "win32") {
    rmSync(resolve(expectedDir, "tinto.pdb"), { force: true });
    rmSync(resolve(expectedDir, "deps", "tinto.pdb"), { force: true });
  }
}

function verifyIsolatedWrites() {
  const workbenchConfig = resolve(stateDir, "workbenches.toml");
  const persisted = readFileSync(workbenchConfig, "utf8");
  if (!persisted.includes("E2E aislado")) {
    throw new Error(`E2E workbench was not persisted under isolated state: ${workbenchConfig}`);
  }
  if (readdirSync(webviewDataDir).length === 0) {
    throw new Error(`E2E WebView profile was not created under: ${webviewDataDir}`);
  }
}

const tauriCli = resolve(repoRoot, "node_modules", "@tauri-apps", "cli", "tauri.js");
const wdioCli = resolve(repoRoot, "node_modules", "@wdio", "cli", "bin", "wdio.js");

let runFailure;
try {
  run(process.execPath, [
    tauriCli,
    "build",
    "--debug",
    "--features",
    "e2e-wdio",
    "--config",
    "src-tauri/tauri.e2e.conf.json",
    "--no-bundle",
    "--ci",
  ]);
  run(process.execPath, [wdioCli, "run", "wdio.conf.ts"]);
  verifyIsolatedWrites();
} catch (error) {
  runFailure = error;
}

const cleanupFailures = [];
try {
  removeE2eExecutable();
} catch (error) {
  cleanupFailures.push(error);
}
try {
  await removeIsolatedState();
} catch (error) {
  cleanupFailures.push(error);
}

if (cleanupFailures.length > 0) {
  const cleanupError = new AggregateError(cleanupFailures, "Tauri E2E cleanup failed");
  if (runFailure) {
    console.error(cleanupError);
  } else {
    runFailure = cleanupError;
  }
}

if (runFailure) {
  throw runFailure;
}
