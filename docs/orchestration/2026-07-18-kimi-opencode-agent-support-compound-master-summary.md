---
title: Compound Master Summary - Kimi Code and OpenCode agent support
status: completed
date: 2026-07-18
initiative: kimi-opencode-agent-support
roadmap_item: RDM-022
production_posture: prototype
verification_status: passed
---

# Compound Master Summary - Kimi Code and OpenCode agent support

## Outcome

RDM-022 is completed. Tinto now recognizes Kimi Code and OpenCode, reports provider readiness against the repository's local or WSL execution source, and runs both through one typed, bounded ACP v1 supervisor where the provider and platform satisfy the negotiated and containment requirements. PTY compatibility remains explicit and recoverable before a valid provider session; failures after readiness never replay a turn through PTY.

The complete functional acceptance contract R1-R23 and AE1-AE11 is satisfied. The authoritative one-row-per-criterion ledger is in `docs/orchestration/compound-master-state.md`.

## Delivered Capability

- Kimi Code: allowlisted, selectable and labelled throughout Agents; local ACP v1 supports typed prompts/updates/completion/cancel, opaque session IDs, provider-owned authentication, native load or ContextBridge, negotiated images/model/mode controls and exact backend-authoritative permissions.
- OpenCode: uses the same supervisor and wire core. Its descriptor forces `--cwd`, loopback, `--port 0`, no mDNS and an ephemeral child-only password. By the approved 2026-07-19 policy, the observed `127.0.0.1:4096` no longer blocks ACP; real process or protocol failures before session readiness still degrade visibly to retryable PTY.
- Recovery and lifecycle: six explicit ACP states, source-aware guidance/recheck, confirmed idle PTY-to-ACP retry, one stable output reader across generations, transcript/checkpoint/session preservation, bounded cancellation and descendant reaping.
- Safety: typed official schema, exact generation/session/turn/request/method correlation, bounded frames/queues/pending work/text/stderr/writers, deny-safe permission tombstones, environment allowlists, sanitized persistence and rejection of unadvertised file/terminal reverse methods.
- Existing behavior: Codex app-server, Claude, generic PTY, journal, checkpoints, host context and WSL source routing remain on their established architecture.

## Artifact Handoff

- Roadmap: `docs/roadmaps/2026-07-18-009-kimi-opencode-agent-support-roadmap.md`.
- Product contract/plan: `docs/plans/2026-07-18-022-feat-kimi-opencode-agent-support-plan.md`.
- Work package and verification table: `docs/work-packages/RDM-022-kimi-opencode-agent-support/2026-07-18-022-kimi-opencode-acp-work-package.md`.
- Bus contract: `docs/contracts/bus-contract.md`.
- Six-cell platform and real-probe evidence: `docs/manual-smoke/2026-07-18-kimi-opencode-agent-support.md`.
- Acceptance ledger and resumable state: `docs/orchestration/compound-master-state.md`.

## Verification

| Surface | Result |
|---|---|
| Rust tests | PASS — 397/397. |
| Rust build/check/format | PASS. |
| ACP/UI focused tests | PASS — TerminalPanel + bus contract 127/127; WSL gate 97/97. |
| Exact `npm test` | PASS — 52 files, 691/691. |
| Contract generation/check | PASS. |
| ESLint and Prettier | PASS after final documentation reconciliation. |
| `npm run build` | PASS — TypeScript and Vite production build. |
| Clippy with warnings denied | PASS after four mechanical lint corrections. |
| Windows process cleanup | PASS — descendant reaping test. |
| Code review and security review | PASS — no open P0-P2. |

## Platform Truth

- Real isolated probes used Kimi Code 0.27.0 and OpenCode 1.18.3 without user credentials.
- The Kimi probe could not exercise an authenticated provider session; deterministic official-schema fixtures provide the structured conformance evidence without claiming an authenticated manual smoke.
- The OpenCode probe observed an ACP v1 handshake but `session/new` timed out, so native ACP is eligible without claiming that the real smoke reached session readiness.
- Linux-native and Ubuntu WSL runners were unavailable. All six R18 cells are still complete with exact prerequisites or explicit permitted limitations; unsupported cells are not labelled as structured support.
- Kimi/OpenCode WSL execution stays PTY-only in this cut because structured distro process-group cleanup is not verified.

## Closure Status

The strict Verification Contract is satisfied. No implementation, review, security or repository-wide gate blocker remains, and there is no package-local next action.

No branch, commit, push, pull request, Jira mutation, deployment or release was performed.
