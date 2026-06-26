import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const artifactName = "tinto-agent-linux-x86_64";
const dryRun = process.argv.includes("--dry-run");

function findAgentArtifacts(root) {
  const found = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    let entries;
    try {
      entries = fs.readdirSync(current, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(full);
      } else if (entry.isFile() && entry.name === artifactName) {
        const stat = fs.statSync(full);
        found.push({ file: full, mtimeMs: stat.mtimeMs });
      }
    }
  }
  return found.sort((a, b) => b.mtimeMs - a.mtimeMs);
}

function resolveAgentPath() {
  if (process.env.TINTO_WSL_AGENT_LINUX_BIN) {
    return path.resolve(process.env.TINTO_WSL_AGENT_LINUX_BIN);
  }

  for (const root of [
    path.join(repoRoot, ".ci-artifacts"),
    path.join(repoRoot, "src-tauri", "resources"),
  ]) {
    const [latest] = findAgentArtifacts(root);
    if (latest) return latest.file;
  }

  return null;
}

const env = { ...process.env };
const agentPath = resolveAgentPath();
if (agentPath) {
  env.TINTO_WSL_AGENT_LINUX_BIN = agentPath;
  env.TINTO_WSL_AGENT_ALLOW_DEV_SOURCE = "";
  console.log(`Using WSL agent: ${agentPath}`);
} else {
  env.TINTO_WSL_AGENT_ALLOW_DEV_SOURCE = "1";
  console.warn("No tinto-agent-linux-x86_64 artifact found; falling back to WSL source build.");
  console.warn("If WSL lacks GTK/Cairo dev packages, download the CI artifact first:");
  console.warn(
    "  gh run download <run-id> --name tinto-agent-linux-x86_64 --dir .ci-artifacts/<run-id>",
  );
}

if (process.platform !== "win32") {
  env.WEBKIT_DISABLE_COMPOSITING_MODE ??= "1";
}

const command = process.platform === "win32" ? "npx.cmd" : "npx";
const args = ["tauri", "dev"];

if (dryRun) {
  console.log(`${command} ${args.join(" ")}`);
  process.exit(0);
}

const child = spawn(command, args, {
  cwd: repoRoot,
  env,
  shell: process.platform === "win32",
  stdio: "inherit",
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code ?? 0);
  }
});
