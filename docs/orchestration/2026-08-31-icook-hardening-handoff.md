# Tinto hardening handoff from the ICook continuation

Status: **open findings for a future hardening session; no repair dispatched**.
Captured 2026-08-31 while resuming ICook run `IC-RUN-2026-08-28-001`.
This is the canonical Tinto follow-up record for the user's request to track the gaps. Update it and the linked blocker entries when reproducing, fixing or closing an item.

The prior [tooling repair reconciliation](runs/tooling-repair-2026-08-31/reconciliation.json) remains historical. Tinto code revision `09ebd549aa3a5049bdd1c9cb17d687aa11093e40` plus documentation HEAD `59f0cecafb41c08db6a4c18001fa24131da7003e`, and Pumarejo `f0af7c65ee5f6a866e6d5cfbf31579231e4e9fec`, were the versions used for this continuation. No new repair or full-suite pass is claimed.

## Evidence and ownership

[Selected public MCP observations](evidence/icook-continuation-2026-08-31/observations.json) preserve exact excerpts, observation labels, original source paths and SHA256 hashes. They exclude the full ICook source transcript and user prompt. Raw originals and the full campaign are retained in the external [continuation report](C:/Users/User/.codex/visualizations/2026/08/31/01a056e3-6d9a-7911-bf89-2221dbabb68e/icook-continuation/continuation-report.md).

Every GUI action used Pumarejo's public MCP API. Tinto-managed Agents ran the ICook read-only commands. This handoff/evidence export is a host-side **Tinto documentation change**, explicitly requested by the user; it changes no ICook file or runtime permission.

Priority below is proposed triage priority. A suspected root cause is never treated as confirmed.

## TIN-HARD-01 — WSL provider failure leaves ICook unusable

**P1; observed failure; owner: Tinto WSL/runtime diagnostics, with environment triage.**
Tracker: `IC-HARD-BLK-2026-08-31-01` in [blockers](../swarm/blockers.yaml).

The saved WSL ICook project reports `Error permanente: el agente WSL cerro stdout`; files and history fail to load. One supported provider retry produces the same error. The message establishes provider termination, not why stdout closed. Do not classify it as an ICook compiler failure.

Evidence labels: `wsl-provider-failure`, `wsl-retry-unchanged`.

Final revalidation: the overview subsequently reported an ICook refresh with 308 changed files. This was new evidence worth checking, not proof of Agent readiness. One fresh start attempt in the saved WSL project then ended in `child_exit`, `BLOQUEADO`, and `Error: el agente WSL cerro stdout`. No prompt or implementation unit was sent. The other project's timeout is not attributed to ICook. Evidence: `wsl-file-refresh-is-not-agent-readiness`, `fresh-agent-launch-blocked`. A successful repository refresh must not conceal a failed execution provider.

Next reproduction: open that existing WSL project and capture the exact provider identity, launch/exit diagnostics and recovery transition. Separate distribution/transport failure from application failure.

Close only when the project either loads and executes one attributable bounded managed command, or reports an actionable causal error and an honest recovery state. A retry must not duplicate an Agent dispatch or leave a failed backend process attached.

## TIN-HARD-02 — native project registration cannot be completed through the supported GUI boundary

**P1; observed blocked workflow; owner: Tinto onboarding + Pumarejo native-dialog integration.**
Tracker: `IC-HARD-BLK-2026-08-31-02`.

`Repos → Agregar repositorio… → Elegir carpeta` stays at `Abriendo…` with controls disabled. Public discovery exposes only the main surface, dialog detection reports `provider_dialog_absent`, and coverage is unknown. A usable pending file-picker decision was not demonstrated. The provider's “absent” result is not proof that no inaccessible OS dialog existed.

Automatic repository discovery also remained `Buscando repositorios…` in the bounded observation. That is a stalled observation, not proof of an infinite loop.

The user offered manual recovery and replied that ICook had been opened. The following broad snapshot timed out; authoritative status then returned idle. A single supported relaunch succeeded, but fresh project snapshots still listed only `WSL Ubuntu-24.04:ICook`, not the Windows path. **The cause is unknown:** do not infer user error, a crash, or a separate-instance/profile bug from this sequence.

Evidence labels: `native-picker-surface-discovery`, `native-picker-dialog-detection`, `native-picker-coverage`, `native-picker-stuck`, `automatic-discovery-pending`, `owned-session-became-idle`, `relaunch-ready`, `icook-still-wsl-only`.

Close only when the same provider-controlled instance can select and cancel a folder through an advertised supported path, recovers its controls on cancellation/failure, visibly records the native project identity, and retains it after a clean restart. Automatic discovery needs a bounded result/error and a usable cancel/recovery path. Verify instance/project identity explicitly when handing control between a human and automation.

## TIN-HARD-03 — availability does not establish a usable managed project runtime

**P1 for this delivery path; observed environment/scope gap, not a demonstrated sandbox defect.**
Tracker: `IC-HARD-BLK-2026-08-31-03`.

A native Tinto Agent really ran read-only commands. Its cwd/write root was Tinto and its identity was `AIR103/CodexSandboxOffline`. ICook docs/source were readable, but all four Git queries returned 128 due to dubious ownership. `node`, `python` and `py` were absent from the managed PATH. The attempted doctor process returned 1 because Node was not found; doctor itself did not run. Cargo was available.

The installed host Node was found at `C:/nvm4w/nodejs/node.exe` and a follow-up asked the Agent to check that executable without changing permissions. No terminal outcome for that check was recovered, so this handoff does not claim the explicit executable worked or failed.

**Workspace restrictions were expected for an Agent launched in Tinto.** Read access does not prove write access. This evidence does not establish a new Full Access propagation bug, nor authorize disabling the sandbox or adding Git trust exceptions.

Evidence: `native-managed-preflight`.

Close only after a native **ICook** Agent visibly binds to the intended project, reports its effective scope and runtime resolution, and runs the agreed bounded command manifest with captured exits. Explain missing runtimes, ownership rejection and project mismatch before the user is led to believe the session is ready for development. Test Full Access propagation separately only through its supported, authorized consent flow; never use a broader permission mode to conceal a wrong project.

## TIN-HARD-04 — interrupted audits need reliable identity and outcome recovery

**P2; observed recovery/visibility gap; owner: Tinto Agents/history.**
Related existing tracker: `TR-BLK-2026-08-31-05` (historical/context evidence).

Native audit session `bd65c1a3-4444-4b04-8522-00d8f4345b22` appears in history under `ICook continuation runtime preflight, read-only,…` with 3401 events. Opening the saved transcript shows an archived state, zero active sessions, and a close control named `Cerrar Nueva conversación`. The second inventory has partial command output and no final response. The title discrepancy is observed; its persistence and root cause are unproven.

The partial output includes a 43-recipe inventory; it is not a completed review, worker terminal or successful test run. Opening the archive did not resume the Agent. Neither uncertain message was resent.

Evidence: `archived-audit-identity`, `archived-tab-title-and-zero-active`.

Close with a bounded command interrupted at a known point: preserve the project, full session identity, title, command exit/interruption disposition and last completed turn after restart. An archived transcript must not silently become a new dispatch. Ensure the UI distinguishes partial output, completed response and unknown send outcome.

## TIN-HARD-05 — large transcript observation needs bounded, recoverable output

**P2; observed automation limit; owner: Tinto semantic surface + Pumarejo observation.**
Related existing tracker: `TR-BLK-2026-08-31-05`.

Broad semantic observations timed out or returned `SEMANTIC_EXTRACTION_FAILED` in the first audit. In the recovered3401-event archive, a snapshot visited 6359 DOM nodes and hit `fieldBudget`: early output consumed the budget and later nodes lost text. A fresh scoped `rootRef` snapshot recovered the required output with no provider truncation. Do not confuse field-budget truncation with a model-context limit, a process crash or failed tests.

Evidence: `large-transcript-field-budget`, `scoped-transcript-read-recovers`; original timeout envelopes remain in the external evidence packet.

Close with a representative long transcript: bounded semantic reads preserve attribution, explicitly signal truncation and support scoped recovery of the final output. A timeout must remain distinguishable from terminal session loss. Measure high-context compaction/performance separately; it is still unproven.

## TIN-HARD-06 — Gitleaks failure falls back to a weaker detector

**P2; observed degraded verification; owner: Tinto secret-scan integration/environment triage.**

The Tinto repository overview reports `Gitleaks falló Detector básico activo` and the accessible status says Gitleaks could not complete, suggests checking `.gitleaks.toml`, and reports use of the basic detector. This proves degraded scanning, not that the configuration is invalid or that a secret exists. No scanner exit code or causal diagnostic was recovered. It is a Tinto observation, not an ICook secret-scan result.

Evidence: `gitleaks-basic-detector-fallback`.

Close by capturing the resolved scanner/config identity, causal error and process exit; repair only the demonstrated cause. A configured full scan must complete successfully, or the UI and downstream verification must retain an explicit degraded/blocked state. The basic detector must never silently satisfy a full secret-scan gate. Do not expose secrets in diagnostics or relax scan rules to obtain a pass.

## Preserve repaired behavior and existing open items

- **Unknown-send safety worked:** the UI retained the draft and reported `No se pudo confirmar el envío…`. No duplicate send was attempted. Keep this regression while improving outcome recovery; see `unknown-send-draft-retained`.
- Do not reopen prior completion/idle repairs merely because the native audit was interrupted. This continuation did not freshly certify every completion/refresh path.
- `TR-BLK-2026-08-31-04`: one-child interruption and sibling continuity remain unproven. Prior two-child routing success is not interruption proof.
- `TR-BLK-2026-08-31-02`: nonempty quarantine still needs an identity-bound native deletion adapter. No recursive deletion workaround.
- `TR-BLK-2026-08-31-03`: provider staging/integrity and minimized-window restore remain open. Do not ignore doctor drift or report restore from an unverified postcondition.
- `TR-BLK-2026-08-31-06`: prior WSL text-policy and effect-lint baseline failures remain explicitly separate from newly observed defects.
- `TR-BLK-2026-08-31-01`: the external Seneschal transition compiler cannot represent the explicit user-authorized documentation bypass. That is an orchestration dependency, not permission to fabricate an approval receipt or a Tinto bug.

## Keep non-Tinto findings separate

**Pumarejo:** native picker coverage, semantic observation limits, actual keyboard activation/reload evidence, integrity, restore and quarantine capabilities require their own attribution. Native ICook campaign results remain 13 observed passes and 4 partial/blocked cases.

**ICook:** the recovered inventory contains five concrete ingredient-list/step mismatches. They are recorded in the external [static review](C:/Users/User/.codex/visualizations/2026/08/31/01a056e3-6d9a-7911-bf89-2221dbabb68e/icook-continuation/recovered-static-review.md), not as Tinto defects. No ICook edits, current automated gates, commits or push were completed here.

**Environment:** the native managed check found Android SDK variables unset and no emulator in PATH/the checked conventional Windows SDK location. Other locations and signing readiness were not exhaustively inspected. No SDK or credential changes were made.

## Next session order and boundaries

1. Re-read this handoff, the linked observations and existing blocker dispositions. Revalidate the current revisions and project/session identity.
2. Restore native project onboarding or the diagnosed WSL route first. Prove one bounded command in the intended ICook workspace before dispatching implementation.
3. Exercise runtime readiness diagnostics, interruption/history recovery and large-transcript observation independently with finite budgets.
4. Retain the repaired unknown-send behavior. Prove child interruption, native dialogs and high-context behavior only where the provider advertises the necessary capability.
5. Run focused regressions for actual repairs, then the required aggregate checks; record baseline failures honestly. Update these IDs with exact closure evidence.

The user asked to **track** these gaps for the next hardening session, not to start that repair campaign now. No implementation, permission/config change, new worker lane, commit, push, PR or Jira mutation is authorized by this handoff. ICook's expired autonomy ledger remains expired; this document does not renew or substitute for it.

## Current local hardening campaign — 2026-08-31

The subsequent explicit user request authorizes reversible local hardening with
a documentation-gate bypass, not a fabricated approval. Historical dispositions
above remain historical. See [current authorization](runs/icook-hardening-2026-08-31/authorization.md)
and [root progress and fresh evidence](runs/icook-hardening-2026-08-31/progress.md).
Native ICook execution is not yet verified; file refresh and public launch-ready
are not accepted as substitutes. No expired publication authority is reused.

Current disposition: three local defects repaired and independently reviewed
(WSL diagnostics, archived title fallback, Gitleaks error categories); no finding
is end-to-end native-verified closed. See the [final six-finding report](runs/icook-hardening-2026-08-31/final-handoff.md)
and [machine-readable reconciliation](runs/icook-hardening-2026-08-31/reconciliation.json).
The supported-integration and complete-scanner prerequisites are retained as
`IC-HARD-BLK-2026-08-31-04` and `IC-HARD-BLK-2026-08-31-05` in the blocker ledger.



