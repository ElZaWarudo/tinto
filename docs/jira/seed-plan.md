# Proposed Jira seed plan — Tinto gap closure

Created: 2026-08-27  
Status: proposal only; no Jira provider or project is configured

## Recommendation

Do not seed Jira for this initiative by default. The local repository already
contains the authoritative roadmap, Compound state, and work-package history;
creating an artificial hierarchy would add administration without improving
delivery. Keep `jira.provider` and `project_key` unset.

If the user later requests Jira traceability and explicitly selects a provider,
reuse existing issues when available and propose only:

- One parent: **Tinto gap closure**.
- One child for **RDM-024 provider-neutral MCP**.
- One child for **Windows native regression and atlas refresh**.
- Subtasks only for independently executable review units produced by the
  RDM-024 work package; do not mirror every document or internal gate.

## Proposed labels and states

- Labels: `tinto`, `gap-closure`, plus `mcp` or `qa` as applicable.
- Initial status: the provider's ordinary backlog state.
- No sprint placement is inferred.
- Blocked/deferred state is copied from the local blocker ledger only after
  provider selection and explicit mutation approval.

## Mutation classes requiring later approval

- Search/read the selected Jira project for reuse candidates.
- Create the parent and missing child issues.
- Link dependencies or PRs.
- Comment or transition issues during release handoff.

No Jira read or mutation is part of the current documentation-planning run.

