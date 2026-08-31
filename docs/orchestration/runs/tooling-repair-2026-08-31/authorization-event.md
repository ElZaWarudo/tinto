# User authorization

Source: attached request; SHA-256: a76acbb1964d782069006f1b81186ada61a06458ed898c186e153f714accb0f1

$krt-swarm-seneschal

Mode: autonomous-team-flow

Fix the Tinto, Pumarejo, and orchestration findings that prevent reliable completion of ICook. Continue through implementation, regression testing, review, and a verified handoffâ€”not just planning.

Repositories:
- C:\Users\User\Documents\personal\tinto
- C:\Users\User\Documents\personal\pumarejo

Source report:
C:\Users\User\Documents\personal\ICook\docs\tinto-pumarejo-dogfood-report.md

Read the entire report, linked evidence, applicable AGENTS.md files, and existing Seneschal queue/blocker/reconciliation artifacts before acting. Treat the report as the finding inventory; do not assume the previous chat summary is complete or current.

WORKFLOW AND AUTHORIZATION

Use the canonical Seneschal workflow without replacing its internal protocol. Create and validate the repair documentation packet before implementation.

I authorize documentation, executable queue state, bounded worker dispatch, code/test/configuration changes in Tinto and Pumarejo, verification, review, reconciliation, and focused local commits through krt-release-marshal. Record an explicit user-authorized documentation gate bypass; do not fabricate a review receipt.

For this tooling-repair task, normal repository filesystem, shell, and coding-agent tools are permitted. This is an explicit exception to the earlier Tinto-only implementation restriction, limited to repairing Tinto/Pumarejo and updating repair evidence.

Every GUI interaction must still go through Pumarejo. Do not load or use computer-use, browser-control, pyautogui, WinAppDriver, PowerShell UI automation, or another GUI fallback.

Do not implement ICook features in this task. ICook changes are limited to the dogfood report, repair handoff/evidence, and any strictly necessary supported development-only Pumarejo integration correction. Record integration corrections separately.

Jira: skip; provider: none.
No remote pushes, PRs, releases, deployments, credential changes, or destructive Git operations are authorized.

VERSION AND STATE SAFETY

Inspect both repositories and preserve existing user changes. Fetch and identify current GitHub revisions; do not silently discard local work or assume the previously verified Tinto revision is still latest. Record exactly which revisions are tested and explain any divergence.

REPAIR PRIORITIES

Reproduce and resolve every actionable tooling/orchestration finding in the report, prioritizing:
- Tinto sessions that remain indefinitely in Enviando or exhaust reconnect attempts.
- Fresh agent sessions that fail to start or immediately archive.
- Incorrect running/idle/archived status and missing actionable errors.
- Duplicate or ambiguously attributed tasks, tabs, dispatches, and results.
- Context-pressure feedback, safe resumption, and history preservation.
- Pumarejo SESSION_CREATE_FAILED during ICook launch.
- Diagnostics disappearing after launch failure as SESSION_NOT_ACTIVE.
- Semantic snapshot/screenshot stalls and unintended session teardown.
- White/blank WebViews, lifecycle recovery, and cleanupPending artifacts.
- Dialog, keyboard, resize, and other advertised automation capability gaps.

These are investigation leads, not predetermined root causes. Distinguish actual defects from unsupported capabilities, environmental blockers, and incorrect client usage.

The previous recovery also observed five exhausted reconnect attempts and failed ICook launches after supported Pumarejo initialization. Locate durable evidence for these observations and append missing findings accurately. Mark evidence gaps explicitly.

EXECUTION

Give each worker bounded ownership, exclusions, acceptance criteria, and focused tests. Keep shared protocol/lifecycle edits coordinated. Allow workers to finish autonomously and reconcile their results.

Prefer root-cause fixes and small regression tests. Do not hide failures by relaxing checks, marking unsupported behavior passed, or indefinitely increasing timeouts.

Keep the dogfood report updated as work occurs. Preserve finding IDs and original observations. Each resolved finding needs root cause, changed files, regression coverage, fresh reproduction evidence, and final status.

ACCEPTANCE

Run supported Pumarejo doctor/preflight checks and demonstrate:
- Tinto launch, semantic snapshot, click/type, keyboard, resize, screenshot, supported dialogs, and clean close/reopen.
- Exactly one attributable task per successful dispatch.
- Distinguishable concurrent workers, correct message routing and result attribution.
- Interrupting one worker without affecting siblings.
- Truthful completion/blocker state.
- History preservation and safe resumption after restart.
- ICook launching through the authenticated, development-only Tauri integration.
- Useful retained diagnostics after an intentional safe launch failure.
- Cleanup of owned processes.

Do not mark unsupported or blocked cases passed. Continue independent repairs when one unit blocks.

Write:
C:\Users\User\Documents\personal\ICook\docs\tinto-pumarejo-repair-handoff.md

The handoff must include tested revisions, local commit hashes, finding-by-finding status, commands/results, evidence paths, remaining blockers, supported launch commands, and whether ICook completion can safely resume.

Stop only when all in-scope findings are fixed and verified or precisely blocked with no safe independent work remaining.

This is an explicit one-run documentation gate bypass. No generated document approval or review receipt is asserted. Local commits are authorized only through Release Marshal. All external mutation classes are denied.
