---
artifact_contract: ce-unified-plan/v1
artifact_readiness: requirements-only
---
# Tooling repair for reliable ICook completion

Actors: the user, Tinto agent sessions and their child workers, Pumarejo MCP clients, and the repair supervisor. Source inventory: ICook/docs/tinto-pumarejo-dogfood-report.md, read in full before this packet.

The goal is reliable, attributable completion and recovery through the supported authenticated development automation boundary. A running process, finished command, archived conversation, and completed agent turn are distinct states. Historical report observations are retained; every new disposition identifies current evidence.

Scope: reproduce and repair actionable Tinto session startup, permission propagation, turn status, reconnect, attribution, context and history defects; Pumarejo launch, retained diagnostics, bounded observations, cleanup and advertised capabilities; correct orchestration evidence and supported ICook development integration only.

Non-goals: ICook features, Android toolchain installation, provider credential changes, product redesign, releases, pushes, PRs, deployment, destructive Git, or any GUI system except Pumarejo. Preserve all pre-existing user changes and historical findings. Jira is skipped, provider none.

Success: focused regressions and independent review for each changed behavior; fresh Pumarejo doctor, launch, semantic snapshot, click/type/key/resize/screenshot, supported dialog, close/reopen; attributable dispatch/concurrency/interrupt/result routing; truthful status; restart preservation; ICook authenticated development launch; safe intentional failure with retained diagnostics and owned-process cleanup. Unsupported/blocked cases are never passed. Every unresolved case must have precise evidence and no remaining safe independent repair path.

Decisions: explicit user bypass permits downstream actions after packet validation; gate remains in_review and approval fields null. Use two independently isolated repository ownership domains, serialize edits within each lifecycle domain. Root owns GUI checks and report/state. Deep workers use read-only discovery then manifested implementation. Review all control-flow changes, with security review for sensitive diagnostics/process custody. Verification logs are root-owned outside implementation worktrees.

Escalation: blockers are recorded without asking during autonomous flow. Never infer a new protocol or relax a timeout to hide failure. Missing historical raw logs remain an evidence gap, while fresh defects can be independently reproduced. No later-stage feature scope is authorized.
