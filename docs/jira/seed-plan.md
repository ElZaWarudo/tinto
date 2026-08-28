# Proposed Jira seed plan — Codex subagent parity

Created: 2026-08-28
Status: proposal only; Jira is optional and no provider is selected

## Recommendation

Do not seed Jira during the autonomous implementation run. The conversation,
initiative contract, worker contracts, queue state, and verification evidence
are authoritative. Jira mutations remain outside the autonomy ledger and are
deferred to the single guarded Release Marshal plan.

If the user approves Jira work during release, first resolve the provider and
reuse an open matching issue when one exists. Otherwise propose one standalone
Spanish task rather than an artificial parent/child hierarchy:

- **Resumen:** Incorporar soporte completo de subagentes Codex en Tinto
- **Descripción:** Replicar el comportamiento de subagentes de Codex dentro de
  Agent Lens, con jerarquía persistente, controles directos y restauración tras
  reinicio, preservando la arquitectura y experiencia visual de Tinto.
- **Labels:** `tinto`, `agents`, `codex`, `subagents`
- **Initial status/sprint:** resolve from the selected provider; do not infer.

## Release-time mutation classes

- Search/reuse or create the standalone task.
- Add the approved PR remote link.
- Transition the task to `En Revisión` after a ready PR exists.

No Jira read or mutation is authorized before the guarded release plan is
explicitly approved.
