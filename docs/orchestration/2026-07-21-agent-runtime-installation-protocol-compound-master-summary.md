---
title: Agent runtime installation protocol - Compound Master summary
status: completed
date: 2026-07-21
roadmap_item: RDM-023
origin_plan: docs/plans/2026-07-21-023-feat-agent-runtime-installation-protocol-plan.md
work_package: docs/work-packages/RDM-023-agent-runtime-installation-protocol/2026-07-21-023-agent-runtime-installation-protocol-work-package.md
---

# Agent runtime installation protocol - Compound Master summary

## Outcome

RDM-023 is implemented and locally verified. An unavailable Claude Code, Codex, Kimi Code or OpenCode provider can now produce an exact short-lived installation preview for the registered local or WSL runtime. Explicit confirmation executes only the compiled shell-free npm recipe, verifies the binary in that same runtime and continues the original launch at most once. Decline, cancellation, stale authorization and every terminal failure create no session.

No real package installation, elevation, branch creation, commit, push, PR, Jira mutation, deployment or release action was performed.

## Delivered surfaces

- Rust recipe catalog, Windows `node.exe` plus associated `npm-cli.js` launcher, WSL argv-safe launcher, prerequisite/version checks and permission guidance.
- Single-use in-memory attempt ledger with entropy, TTL, bounded capacity, atomic claim, runtime drift rejection and a linearized cancel-versus-launch boundary.
- Bounded process runner with minimal environment, output draining/caps/redaction, timeout and descendant cleanup.
- Same-runtime `--version` verification and reuse of the existing internal session-start path.
- Additive Rust/TypeScript bus types and `prepare_agent_install`, `confirm_agent_install`, `cancel_agent_install` commands.
- Accessible React consent/cancel/status flow, targeted readiness invalidation and no frontend replay of the backend-started session.
- Fake-only local and WSL process/runtime evidence plus safe operator smoke documentation.

## Review and security result

- Simplification: one dead attempt field removed; existing process/job and availability patterns reused; no new dependency or generic framework added.
- Code review: cancellation ordering, terminal preview invalidation, process-tree cleanup and fake evidence gaps were fixed. No open P0-P2 finding remains.
- Security Sentinel: pass. Recipes are compiled, IPC confirmation accepts only an opaque attempt ID, no shell/elevation/remote script is shipped, credential-bearing environment variables are excluded and sensitive output patterns are redacted.
- Agent-native observation: installation consent remains intentionally human-driven. The backend commands are shared primitives, but no autonomous agent tool bypasses the consent ceremony.

## Verification evidence

- `npm run contract:check`: pass.
- `npm run format:check`: pass.
- `npm run lint`: pass.
- Focused frontend/contract tests: 55/55 pass.
- Full frontend tests: 711/711 pass using one fork worker and 30-second test/hook timeouts because the default pool produced unrelated timing flakes on this host.
- `npm run build`: pass.
- Focused Rust protocol tests: 14/14 pass.
- `cargo fmt --check`: pass.
- `cargo clippy --all-targets -- -D warnings`: pass.
- Full Rust tests: 408/412 passed in one run; the four failures were existing ACP/bus timing/order tests and each passed when rerun alone (4/4). Every RDM-023 test passed in the combined run.
- `cargo build`: pass.
- `git diff --check`: pass.
- Security searches for shell/elevation/remote-script/env/persistence paths: pass with only expected direct `Command` calls and test canaries.

## Residual risk

- The four npm recipes intentionally resolve the providers' current releases, so upstream package-manager supply-chain risk remains. Current official sources were rechecked on 2026-07-21; future recipe changes require another reviewed revision.
- A real installation was deliberately not tested. The manual smoke requires separate explicit consent naming one provider and one target runtime.
- The repository's default full-test concurrency is flaky on this Windows host. Isolated reruns cleared every failure; CI should remain the authoritative cross-platform concurrency signal.

## Delivery boundary

The working tree remains on `develop` with local uncommitted changes. A later Release Marshal run may create the reviewed branch/commit/PR sequence only after explicit shipping authorization.
