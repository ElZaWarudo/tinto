# ICook tooling hardening — local repairs, native acceptance blocked

This is the result of the current explicit local authorization, not a release or
an approval receipt. No commits, pushes, PRs, Jira writes, destructive cleanup,
ICook commands on the host, or ICook product edits occurred.

## Finding disposition

| Finding | Confirmed result | Remaining closure gate |
| --- | --- | --- |
| TIN-HARD-01 | WSL subprocess stderr was discarded. Both transports now drain it immediately into a bounded 4096-byte tail and expose only fixed diagnostic categories and observable exit status. 22 focused tests pass. | The original Ubuntu child-exit cause and recovery remain unknown. Reproduce in an integrity-verified provider, with project/session attribution; prove one bounded ICook command or an actionable terminal error. |
| TIN-HARD-02 | No safe native onboarding route established. The public dialog API advertises detect/accept/cancel but no folder-path selection field; earlier unknown coverage is not proof of a Tinto dialog bug. No speculative change. | Restore verified integration, then prove select/cancel, parent-control recovery and persistent native ICook identity in the same controlled instance. |
| TIN-HARD-03 | A Tinto-launched Agent's Tinto scope was expected, not a confirmed Full Access defect. File refresh/readability do not establish Agent execution or write readiness. | Bind an actual ICook workspace; through its managed terminal capture scope, Node/Python resolution, ownership diagnostics and exact bounded-command exit. Do not add Git trust exceptions. |
| TIN-HARD-04 | Archived title restoration ignored the persisted first user message when the live timeline was absent. Exact full-session-ID fallback now preserves live precedence; regression reproduced red then green (33 title tests). | Fresh interrupted-session restart must retain identity, partial output, disposition and last completed turn without dispatching an archive or blindly retrying an uncertain send. |
| TIN-HARD-05 | Current observation regression suite passes 33 tests. A fresh public status snapshot is bounded (8 nodes, 276 visited, no truncation), but it is not the required large transcript. Existing scoped/truncation support did not justify a speculative code change. | On verified integration, use a representative long archive and prove explicit truncation, scoped final-output recovery and observation-timeout/session-loss distinction. High-context measurements remain separate. |
| TIN-HARD-06 | Gitleaks 8.30.1 exits 1 with permission warnings and a context deadline at the unchanged 8-second budget. New fixed timeout/permission categories remove unsupported configuration blame; generic/ambiguous errors remain generic. Nine focused tests pass. | Full scanning remains degraded, never certified by the basic detector or partial report. Resolve retained-artifact custody/permissions through a supported flow, rerun the unchanged scanner and verify the UI natively. |

No finding is claimed end-to-end verified closed. Three local defects are repaired
and reviewed; the remaining native closure work is precisely blocked.

## Verification and boundaries

The accepted snapshot (`integration-accepted`, index tree
`636249baf40436a2373c3ededf49a3cc3ada62d9`) contains only four code files:

- `src/panels/terminal/ConsoleDockPanel.tsx`
- `src/panels/terminal/ConsoleDockPanel.test.tsx`
- `src-tauri/src/wsl_agent/launcher.rs`
- `src-tauri/src/bus/secret_scan.rs`

It passes 94 focused frontend tests, all 478 Rust library tests, both production
frontend and development Rust builds, and generated bus-contract verification.
All four code files were applied to the primary checkout with matching SHA-256
bytes, followed by independent primary checks captured in `checks/primary-*.json`.
Those primary checks pass all 94 focused frontend and 478 Rust library tests.
The pre-existing primary Cargo.toml/Cargo.lock provider-path edits were preserved.

Changed-file lint fails on the existing `setQuickLaunchAvailability` effect
(baseline line 312, accepted line 315). Full frontend baseline and accepted runs
both expose the same WSL text-policy assertion and then exceed the 180-second
bound. The frontend aggregate is **failed**, not green or waived. No lingering
Vitest process was found afterward. Exact executable/args/exits/stdout/stderr
are in `checks/`; canonical snapshot fingerprints and registry retain failures.

All accepted diffs received independent review with no findings. WSL and scanner
also passed independent security review. No raw subprocess stderr, secret content
or secret-scan report was published in user diagnostics. Scanner flags, report
success admission, retry predicates, unknown-send behavior and scope enforcement
remain unchanged.

## Native barrier and exact next actions

Fresh public launch reached ready, was observed without dispatch, and public
close/status confirmed idle (`native/001` through `009`). Snapshot 006 and screenshot
007 are **pre-repair baseline observations**, not native verification of these
patches. Final doctor still reports `integration.debug-registration` error and
five residue entries even though the CLI itself exits 0.

Read-only attribution checks found intact owned edit blocks and staged bytes
matching their old manifest, unrelated `lib.rs` full-file drift, and a different
current Pumarejo provider bundle. Supported init/remove previews reject
`ALREADY_INTEGRATED_MODIFIED`. A fresh baseline worktree also rejects init because
committed attribution lacks its ignored manifest. Neither route establishes safe
fresh staging. We did not invent a manifest, rewrite hashes, strip attributed
blocks, or bypass provider integrity.

1. Pumarejo maintainer: provide a supported attribution-preserving migration or
   a matching integration snapshot with provenance; preserve unrelated edits and
   retained quarantines. This prerequisite needs authoritative integration data,
   not a guessed replacement. Re-run doctor and require semantic readiness.
2. Tinto runtime maintainer: use that verified public instance for TIN-HARD-01/-02/-03
   acceptance before any ICook implementation. No attributable bounded ICook
   command succeeded in this run.
3. Repeat native title/interruption/long-transcript/scanner cases with full session
   IDs and authoritative history. Do not resend an uncertain prompt blindly.
4. Custody maintainer: supply the identity-bound quarantine recovery/deletion
   capability or supported ownership remediation before attempting full scanning.
   No broad exclusions, raised timeout, recursive cleanup, or rule weakening.

Existing TR blockers remain: child interrupt capability/sibling continuity,
minimized-window restoration, custody cleanup, historical/high-context evidence,
baseline verification failures, and external orchestration compiler limitations.
Nothing in this campaign certifies those items closed.

## Workflow evidence

- Authority: `authorization.md`; canonical queue approvals remain null.
- Transition limitation: `transition-result.json` fails on historical unknown-unit
  blockers before the documentation gate. No validator changes or fake receipts.
- Accepted contracts: `title-root-contract.md`, `scanner-implementation-worker-contract.json`,
  `wsl-root-contract.md`. Root-direct replacements were separately contracted after
  rejecting the original title worker for command-contract violations and the
  original WSL candidate for success-race/deadlock risks and insufficient tests.
- Accepted reviews: `title-root-review.json`, `scanner-review.json`,
  `scanner-security.json`, `wsl-root-review.json`, `wsl-root-security.json`.
- Workspace mapping: `workspace-plan-06.json` and its compiled artifact. Rejected
  candidate patches and all worktrees remain available; old `integration` is
  diagnostic only. `integration-accepted` is the accepted snapshot.
- Aggregate evidence: `verification-registry.json`, `aggregate-*-fingerprint.json`,
  `aggregate-frontend/`, `aggregate-backend/`; final machine-readable result:
  `reconciliation.json`. Leaf claims were not substituted for root verification.
- Timing adapter incompatibility (`elapsed_budget_exhausted`) is retained; scanner
  timings use explicit supported metrics. Leaf trust remains self-reported. The
  scanner runtime audit's blanket missing-exit caveat is conservative and inaccurate
  for its final validator: numeric exit 0 is retained in root observation. Acceptance
  instead relies on root aggregate commands and their exact exits.

RTK v0.46.0 was added at the previously absent
`C:/Users/User/.cargo/bin/rtk.exe` after verifying the official release digest.
PATH, credentials and system settings were unchanged. Its retained download is in
`C:/Users/User/AppData/Local/Temp/tinto-hardening-rtk-20260831/`.
