---
title: Core WSL read/watch path delivery plan
status: plan-review-passed
date: 2026-06-23
roadmap_item: RDM-004
source_requirements: docs/brainstorms/2026-06-23-004-core-wsl-read-watch-path.md
planning_status: planned
delivery_approach: hybrid
---

# Core WSL Read/Watch Path Delivery Plan

## Planning Source

Primary source: `docs/brainstorms/2026-06-23-004-core-wsl-read-watch-path.md`.

The plan assumes RDM-002 and RDM-003 are present locally: the protocol/launcher exists and Windows-only Ubuntu WSL repos can be configured but are not yet live.

## Scope Summary

Implement the runtime path that lets Tinto track local Windows repos and Ubuntu WSL repos together. Preserve the local backend for local repos. Route WSL repo state, reads, and watch-like updates through `tinto-agent`, mapping results back into existing bus DTOs and Tauri event names.

## Delivery Shape

Hybrid plan with two executable review units:

- RU1 proves the backend routing and read path.
- RU2 adds watch/subscription event forwarding and final contract/security verification.

This split keeps the riskiest bus boundary reviewable before adding a long-running WSL activity loop.

## Plan Units

### U1 - Protocol DTOs and Agent Read Handlers

- Extend `wsl_agent::protocol` beyond handshake with bounded request/response messages.
- Add request variants for repo summary, worktree diff, commit diff, commit log, blob, file content, and tree.
- Add safe agent errors that map to existing git/command categories where possible.
- Implement Linux-side handlers in `tinto-agent` using existing local git/file/tree logic or shared helpers where practical.
- Enforce repo allowlist inside the agent request layer.
- Add protocol and handler tests with local fixtures.

### U2 - Host Backend Selection and Read Command Routing

- Add a repo backend resolver that returns local or WSL identity for active workbench entries.
- Update bus mounting so local repos are canonicalized/watched locally and WSL repos are mounted as opaque Linux-path runtime entries only on Windows.
- Keep unsupported WSL entries on non-Windows.
- Route WSL read commands through the WSL backend and local read commands through existing code.
- Populate WSL `RepoDelta` in snapshots/retries using the agent.
- Add tests proving local + WSL coexistence and no WSL path reaches local canonicalization/watch paths.

### U3 - WSL Watch, Subscriptions, and Event Forwarding

- Add a bounded WSL monitor loop per active WSL repo or per active WSL distro.
- Emit existing `tinto://workbench-delta` and `tinto://fs-events` payloads for WSL activity.
- Refresh WSL subscribed diffs after subscription changes and WSL events.
- Keep `WatchingState` tied to local watcher availability; WSL failures update per-repo `RepoErrorState`.
- Add tests for mixed local/WSL subscriptions, retry, failure isolation, and frontend store compatibility.

### U4 - Contract, Absence, and Verification

- Update contract docs to describe Windows WSL live runtime behavior.
- Extend frontend contract/store tests only where additive WSL runtime payloads require coverage.
- Preserve non-Windows absence tests and Windows-only command gating.
- Record security review and Windows/Ubuntu manual smoke checklist gap for final release.

## Dependencies

- Requires: RDM-001 delivered.
- Requires locally: RDM-002 and RDM-003 implemented and review-passed.
- Blocks: RDM-005 policy for mutations/Gitleaks/Agent Console and RDM-006 packaging/recovery.

## Risks And Mitigations

- Risk: WSL repo paths accidentally canonicalize through Windows. Mitigation: source-aware resolver tests and local watcher call assertions.
- Risk: frontend contract drift. Mitigation: reuse existing DTOs, add JSON shape tests, and update docs only additively.
- Risk: unbounded agent I/O or event loops. Mitigation: message size limit, subscription cap reuse, polling throttle/debounce, and per-repo error mapping.
- Risk: Linux CI cannot run real WSL. Mitigation: mocked transport/agent tests plus final Windows/Ubuntu smoke before release.

## Verification Plan

- Rust: targeted `cargo test --lib wsl_agent`, `cargo test --lib bus -- --test-threads=1`, `cargo test --lib invoke_handler`, and targeted command tests.
- Frontend: targeted Vitest contract/store/workbench absence tests if DTO docs or frontend behavior changes.
- TypeScript: `npx tsc --noEmit`.
- Formatting: targeted Rust format check for changed files, targeted Prettier for changed TS/TSX/docs where applicable, and `git diff --check`.
- Security: focused review after RU2 because external process routing and filesystem reads through WSL are security-sensitive.
- Manual final-release smoke: Windows host with Ubuntu WSL, one local repo and one WSL repo in the same workbench, verify snapshot/diff/tree/file read/event refresh.

## Open Decisions

- None blocking for RDM-004.
- RDM-005 will decide WSL mutations, Gitleaks, media preview, and Agent Console behavior.
