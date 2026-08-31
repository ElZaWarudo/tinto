import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

// Root-owned bounded diagnostic runner. No shell, arbitrary command input, or
// ICook command is accepted. Evidence is not a worker certification.
const tinto = 'C:/Users/User/Documents/personal/tinto';
const pumarejo = 'C:/Users/User/Documents/personal/pumarejo';
const accepted = 'C:/Users/User/Documents/personal/tinto-hardening-worktrees/icook-hardening-2026-08-31/integration-accepted';
const evidence = join(tinto, 'docs/orchestration/runs/icook-hardening-2026-08-31/checks');
const cases = {
  'primary-focused': [tinto, process.execPath, ['node_modules/vitest/vitest.mjs', 'run', 'src/workbench/operations.test.ts', 'src/panels/terminal/ConsoleDockPanel.test.tsx', 'src/panels/terminal/AgentConversationTab.test.tsx', 'src/panels/RepoCard.test.tsx', 'src/agent/sessionStore.test.ts']],
  'primary-full-rust': [tinto, 'C:/Users/User/.cargo/bin/cargo.exe', ['+1.97.1-x86_64-pc-windows-msvc', 'test', '--offline', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--lib', '--', '--test-threads=1']],
  'doctor-final': [tinto, process.execPath, [join(pumarejo, 'dist/cli/index.js'), 'doctor', '--project', tinto, '--json']],
  'accepted-focused': [accepted, process.execPath, ['node_modules/vitest/vitest.mjs', 'run', 'src/workbench/operations.test.ts', 'src/panels/terminal/ConsoleDockPanel.test.tsx', 'src/panels/terminal/AgentConversationTab.test.tsx', 'src/panels/RepoCard.test.tsx', 'src/agent/sessionStore.test.ts']],
  'accepted-full-frontend': [accepted, process.execPath, ['node_modules/vitest/vitest.mjs', 'run', '--maxWorkers=2']],
  'accepted-full-rust': [accepted, 'C:/Users/User/.cargo/bin/cargo.exe', ['+1.97.1-x86_64-pc-windows-msvc', 'test', '--offline', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--target-dir', 'C:/Users/User/Documents/personal/tinto/src-tauri/target', '--lib', '--', '--test-threads=1']],
  'accepted-build-frontend': [accepted, process.execPath, ['C:/nvm4w/nodejs/node_modules/npm/bin/npm-cli.js', 'run', 'build']],
  'accepted-build-rust': [accepted, 'C:/Users/User/.cargo/bin/cargo.exe', ['+1.97.1-x86_64-pc-windows-msvc', 'build', '--offline', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--target-dir', 'C:/Users/User/Documents/personal/tinto/src-tauri/target']],
  'accepted-contract': [accepted, process.execPath, ['scripts/generate-bus-contract.mjs', '--check']],
  'accepted-lint': [accepted, process.execPath, ['node_modules/eslint/bin/eslint.js', 'src/panels/terminal/ConsoleDockPanel.tsx', 'src/panels/terminal/ConsoleDockPanel.test.tsx']],
  'lint-baseline': [tinto, process.execPath, ['node_modules/eslint/bin/eslint.js', 'src/panels/terminal/ConsoleDockPanel.tsx', 'src/panels/terminal/ConsoleDockPanel.test.tsx']],
  'wsl-root-focused': ['C:/Users/User/Documents/personal/tinto-hardening-worktrees/icook-hardening-2026-08-31/wsl-root', 'C:/Users/User/.cargo/bin/cargo.exe', ['+1.97.1-x86_64-pc-windows-msvc', 'test', '--offline', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--target-dir', 'C:/Users/User/Documents/personal/tinto/src-tauri/target', '--lib', 'wsl_agent::launcher::tests', '--', '--test-threads=1']],
  'title-root-red': ['C:/Users/User/Documents/personal/tinto-hardening-worktrees/icook-hardening-2026-08-31/title-root', process.execPath, ['node_modules/vitest/vitest.mjs', 'run', 'src/panels/terminal/ConsoleDockPanel.test.tsx']],
  'title-root-green': ['C:/Users/User/Documents/personal/tinto-hardening-worktrees/icook-hardening-2026-08-31/title-root', process.execPath, ['node_modules/vitest/vitest.mjs', 'run', 'src/panels/terminal/ConsoleDockPanel.test.tsx']],
  'frontend-full-baseline-bounded': [tinto, process.execPath, ['node_modules/vitest/vitest.mjs', 'run', '--maxWorkers=2']],
  'frontend-full-baseline': [tinto, process.execPath, ['node_modules/vitest/vitest.mjs', 'run']],
  'build-baseline': [tinto, process.execPath, ['C:/nvm4w/nodejs/node_modules/npm/bin/npm-cli.js', 'run', 'build']],
  'isolated-init-preview': ['C:/Users/User/Documents/personal/tinto-hardening-worktrees/icook-hardening-2026-08-31/integration', process.execPath, [join(pumarejo, 'dist/cli/index.js'), 'init', '--project', 'C:/Users/User/Documents/personal/tinto-hardening-worktrees/icook-hardening-2026-08-31/integration', '--dry-run']],
  'frontend-baseline': [tinto, process.execPath, ['node_modules/vitest/vitest.mjs', 'run', 'src/workbench/operations.test.ts', 'src/panels/terminal/ConsoleDockPanel.test.tsx', 'src/panels/terminal/AgentConversationTab.test.tsx', 'src/panels/RepoCard.test.tsx']],
  'observation-baseline': [pumarejo, process.execPath, ['node_modules/vitest/vitest.mjs', 'run', 'tests/unit/snapshot-browser.test.ts', 'tests/unit/snapshot.test.ts']],
  'doctor': [tinto, process.execPath, [join(pumarejo, 'dist/cli/index.js'), 'doctor', '--project', tinto, '--json']],
  'init-preview': [tinto, process.execPath, [join(pumarejo, 'dist/cli/index.js'), 'init', '--project', tinto, '--dry-run']],
  'scanner-version': [tinto, 'C:/Users/User/AppData/Local/tinto/addons/gitleaks/bin/gitleaks.exe', ['version']],
  'scanner-diagnostic': [tinto, 'C:/Users/User/AppData/Local/tinto/addons/gitleaks/bin/gitleaks.exe', ['--config', join(tinto, '.gitleaks.toml'), 'dir', '--no-banner', '--redact=100', '--exit-code', '0', '--timeout', '8', '--report-format', 'json', '--report-path', 'C:/Users/User/AppData/Local/Temp/tinto-hardening-gitleaks-20260831.json', tinto]],
  'worker-materialization': [tinto, 'python', ['C:/Users/User/.agents/skills/krt-swarm-seneschal/scripts/materialize_worker_contract.py', '--input', 'docs/orchestration/runs/icook-hardening-2026-08-31/wsl-draft.json', '--repo-root', 'C:/Users/User/Documents/personal/tinto-hardening-worktrees/icook-hardening-2026-08-31/wsl-discovery', '--output', 'docs/orchestration/runs/icook-hardening-2026-08-31/wsl-worker-contract.json']],
  'rtk-availability': [tinto, 'rtk', ['--version']],
};
const name = process.argv[2];
if (!Object.hasOwn(cases, name)) throw new Error('Unknown fixed diagnostic');
const [cwd, executable, args] = cases[name];
const started_at = new Date().toISOString();
const result = spawnSync(executable, args, { cwd, encoding: 'utf8', windowsHide: true, timeout: 180000, maxBuffer: 4 * 1024 * 1024 });
mkdirSync(evidence, { recursive: true });
const record = { name, cwd, executable, args, started_at, finished_at: new Date().toISOString(), exit_code: result.status, signal: result.signal, error: result.error?.code ?? null, stdout: result.stdout ?? '', stderr: result.stderr ?? '' };
writeFileSync(join(evidence, `${name}.json`), JSON.stringify(record, null, 2));
console.log(JSON.stringify(record));
process.exitCode = result.status ?? 1;
