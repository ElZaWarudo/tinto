// Starts Playwright MCP for a Tinto QA job: `node qa-browser.mjs <dir> <package>`.
// The server runs in the job's QA folder and does not see the client's
// workspace roots, so every file the browser saves, named or not, lands in
// that folder instead of the worktree. Roots also arrive as WSL paths when
// the agent runs in WSL, which the Windows side would misread.
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

const [outputDir, pkg] = process.argv.slice(2);
const args = ["-y", pkg, "--headless", "--isolated", "--output-dir"];
const options = { cwd: outputDir, stdio: ["pipe", "inherit", "inherit"] };
// On Windows `npx` is a script that only a shell resolves; the arguments
// are Tinto's own (package and job folder), quoted here.
const child =
  process.platform === "win32"
    ? spawn(`npx ${args.join(" ")} "${outputDir}"`, { ...options, shell: true })
    : spawn("npx", [...args, outputDir], options);
child.on("exit", (code) => process.exit(code ?? 1));

const lines = createInterface({ input: process.stdin });
lines.on("line", (line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    child.stdin.write(`${line}\n`);
    return;
  }
  if (message.method === "initialize") delete message.params?.capabilities?.roots;
  child.stdin.write(`${JSON.stringify(message)}\n`);
});
lines.on("close", () => child.stdin.end());
