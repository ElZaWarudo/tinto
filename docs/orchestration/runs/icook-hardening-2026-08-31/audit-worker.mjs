import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

// Root-owned extraction of this campaign's runtime transcript. Never executes
// transcript text. Exit codes suppressed by a leaf remain unavailable.
const run = 'C:/Users/User/Documents/personal/tinto/docs/orchestration/runs/icook-hardening-2026-08-31';
const units = {
  history: { log: 'rollout-2026-08-31T12-29-11-01a0575d-aa66-7290-ba4c-ea240ef5644e.jsonl', contract: 'archived-title-v2-worker-contract.json', envelope: 'archived-title-v2-envelope.json', terminal: 'icook-hardening-title-terminal.json', actor: '/root/archived_title', workspace: 'history-implementer', reviews: ['archived-title-review.json'] },
  scanner: { log: 'rollout-2026-08-31T12-46-51-01a0576d-d5f1-79f3-b8c0-7be40620f430.jsonl', contract: 'scanner-implementation-worker-contract.json', envelope: 'scanner-implementation-envelope.json', terminal: 'icook-hardening-scanner-terminal.json', actor: '/root/scanner_implementation', workspace: 'scanner-implementer', reviews: ['scanner-review.json', 'scanner-security.json'] },
};
const unit = process.argv[2];
if (!Object.hasOwn(units, unit)) throw new Error('Unknown worker');
const config = units[unit];
const logPath = join('C:/Users/User/.codex/sessions/2026/08/31', config.log);
const records = readFileSync(logPath, 'utf8').trim().split('\n').map(JSON.parse);
const contract = JSON.parse(readFileSync(join(run, config.contract), 'utf8'));
const envelope = JSON.parse(readFileSync(join(run, config.envelope), 'utf8'));
const terminal = JSON.parse(readFileSync(join('C:/Users/User/AppData/Local/Temp', config.terminal), 'utf8'));
const events = [];
for (const record of records) {
  const item = record.payload;
  if (record.type !== 'response_item' || item?.type !== 'custom_tool_call') continue;
  const input = item.input ?? '';
  const commands = [...input.matchAll(/"(?:\\.|[^"\\])*"/g)]
    .flatMap(match => { try { const value = JSON.parse(match[0]); return /^(rtk |printf )/.test(value) ? [value] : []; } catch { return []; } });
  const output = records.find(candidate => candidate.payload?.call_id === item.call_id && candidate.payload?.type === 'custom_tool_call_output');
  if (commands.length || input.includes('tools.apply_patch')) events.push({ timestamp: record.timestamp, finished_at: output?.timestamp ?? null, call_id: item.call_id, commands, edit: input.includes('tools.apply_patch'), output: output?.payload.output ?? null });
}
const finalRecord = records.findLast(record => record.type === 'response_item' && record.payload?.role === 'assistant' && record.payload?.phase === 'final_answer');
const commandEntries = events.flatMap(event => event.commands.map(command => ({ command, kind: contract.commands.verification.focused.includes(command) ? 'verification' : contract.commands.exact.includes(command) ? 'exact' : 'read-only' })));
const started = Date.parse(records[0].timestamp);
const firstEdit = events.find(event => event.edit)?.timestamp;
const lastVerification = events.findLast(event => event.commands.some(command => contract.commands.verification.focused.includes(command)));
const validatorEvent = events.findLast(event => event.commands.includes(envelope.terminal_validation_command));
let validatorExit = null;
for (const item of validatorEvent?.output ?? []) {
  try { const value = JSON.parse(item.text); if (typeof value.exit_code === 'number') validatorExit = value.exit_code; } catch { /* Preserve missing exits. */ }
}
const observation = {
  schema_version: 1, profile: contract.profile, contract_hash: contract.contract_hash, worker_id: config.actor,
  started_at_ms: started, returned_at_ms: Date.parse(finalRecord.timestamp), first_change_at_ms: firstEdit ? Date.parse(firstEdit) : null,
  last_required_command_finished_at_ms: Date.parse(lastVerification.finished_at), phase_duration_ms: { discovery: 0, implementation: Date.parse(finalRecord.timestamp) - started },
  checkpoint_count: 0, interventions_sent: [], final: terminal,
  command_evidence: { trust: 'self-reported', commands: commandEntries },
  terminal_validation_command: envelope.terminal_validation_command,
  terminal_validation_exit_code: validatorExit,
  certifications: config.reviews.map(path => JSON.parse(readFileSync(join(run, path), 'utf8'))),
};
if (contract.profile === 'luna_xhigh') {
  const dispatch = records.find(record => record.type === 'response_item' && record.payload?.role === 'user');
  const acceptedAt = Date.parse(dispatch.timestamp);
  observation.checkpoint_count = 1;
  observation.checkpoint = { event: 'discovery_complete', discovery_complete_at_ms: acceptedAt, edit_path_found: true, planned_files: contract.owned_files, evidence_digest: 'edit src-tauri/src/bus/secret_scan.rs | symbol=scan_with_gitleaks, ScanFailure, tests; pattern=nonzero output discards stderr and returns ScanFailed with unsupported config blame; why=conservative static timeout/permission classification with opaque fallback preserves scanner enforcement. Root accepted this separate discovery checkpoint in initial dispatch; no new discovery occurred here.' };
  observation.discovery_returned_at_ms = acceptedAt;
  observation.implementation_started_at_ms = acceptedAt;
  observation.interventions_sent = ['dispatch_implementation'];
}
writeFileSync(join(run, `${unit}-runtime-audit.json`), JSON.stringify({ logPath, events, returned_payload: finalRecord.payload, caveat: 'Runtime captures command strings and outputs, but leaf discarded numerical command exits. Do not promote to audited exit evidence.' }, null, 2));
writeFileSync(join(run, `${unit}-partial-observation.json`), JSON.stringify(observation, null, 2));
const skill = 'C:/Users/User/.agents/skills/krt-swarm-seneschal/scripts';
const observed = spawnSync('python', [join(skill, 'capture_worker_observation.py'), '--repo-root', `C:/Users/User/Documents/personal/tinto-hardening-worktrees/icook-hardening-2026-08-31/${config.workspace}`, '--base-revision', '59f0cecafb41c08db6a4c18001fa24131da7003e', '--baseline-tree', 'e2541e203c7cf504d4a5e8f888dc48da89db8f1a', '--input', join(run, `${unit}-partial-observation.json`), '--output', join(run, `${unit}-root-observation.json`)], { encoding: 'utf8', windowsHide: true });
if (observed.status !== 0) throw new Error(observed.stderr);
const evaluated = spawnSync('python', [join(skill, 'evaluate_worker_run.py'), '--contract', join(run, config.contract), '--input', join(run, `${unit}-root-observation.json`)], { encoding: 'utf8', windowsHide: true });
if (evaluated.status !== 0) throw new Error(evaluated.stderr);
writeFileSync(join(run, `${unit}-evaluation.json`), evaluated.stdout);
console.log(JSON.stringify({ unit, events: events.length, commands: commandEntries.length, started_at: records[0].timestamp, returned_at: finalRecord.timestamp, numerical_exit_evidence: false }));
