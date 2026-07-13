---
artifact_kind: roadmap
artifact_path: docs/roadmaps/2026-06-22-003-post-closeout-ux.md
title: Tinto — Post-Closeout UX Iteration
status: delivered
date: 2026-06-22
source_docs:
  - docs/roadmaps/2026-06-10-001-tinto-roadmap.md
  - docs/roadmaps/2026-06-19-002-agent-console-integration.md
  - docs/orchestration/compound-master-state.md
initiative: Post-Closeout UX Iteration
production_posture: prototype
---

# Tinto — Post-Closeout UX Iteration

## Scope Fit and Constraints

- Post-roadmap-closeout UX iteration. Both the Tinto roadmap (`2026-06-10-001-tinto-roadmap.md`) and the Agent Console Integration roadmap (`2026-06-19-002-agent-console-integration.md`) are closed.
- Production posture remains `prototype`. No compatibility guarantees apply beyond current desktop behavior.
- Pulls from the "Residual Backlog" in `docs/orchestration/compound-master-state.md` and converts the highest-impact UX gap into a single planned package (RUL-001).
- Branch strategy: local fast-forward merge into `develop` and push, no PR (standing user preference).
- Iterative on `develop`; this roadmap packages the ruler work so it can be reviewed through the artifact pipeline while preserving the iterative cadence for later UX gaps.
- **Review path for this iteration:** dev-only browser fixture at `http://127.0.0.1:1420/demo.html` (see RUL-001 work package and state file). The Tauri app is not on the review path for RU1 because the user could not inspect the rail in the Tauri webview.

## Roadmap Items

### RUL-001 — File Overview Ruler parity with Visual Studio Code

- **Outcome:** Turn the right-side file overview ruler (currently alert-only) into a true whole-file navigation/follow surface synced to the full document, matching the Visual Studio Code overview-ruler experience: always-visible track, scroll-synced caret, click-to-jump on the full track, configurable width and density, and the same marker style for alerts, diff hunks, and (future) search matches.
- **Scope surfaces:** `src/panels/file/FileOverviewRuler.tsx`, `src/panels/file/useOverviewScrollSync.ts` (new), `src/panels/file/FileOverviewRuler.test.tsx` (new), `src/panels/file/useOverviewScrollSync.test.tsx` (new), `src/panels/diff/DiffView.tsx`, `src/panels/diff/FullFileView.tsx`, `src/panels/file/FileView.tsx`, `src/App.css`, `demo.html` (new), `src/demo/main.tsx` (new), `src/demo/demo.css` (new), and corresponding test updates in `src/panels/diff/*View.test.tsx` and `src/panels/file/FileView.test.tsx`.
- **Out of scope (deferred):** Adding a new file-content search feature is deferred. Search-result markers in the rail are blocked on a separate search initiative. WSL2 watcher fallback, file/tree UX polish, delete undo/redo, file operations, and Gitleaks addon/configuration flows are already shipped (see state file "Post-Closeout Iteration" sections) and not in scope here.
- **Dependencies:** none in code. The RUL-001 package depends only on the existing `FileOverviewRuler` and the three view components shipped in `233bd41` and earlier commits.
- **Verification criteria:** see the work package "What to test (user review checklist)" and the state file RUL-001 section.

## Deferred Items

The following residual backlog items are intentionally **not** in this iteration:

- Opt-in Git fetch.
- Phantom-repo generation token after workbench switch.
- TypeScript/Rust contract code generation.
- Keyboard arrow navigation / Escape polish (in the file body itself; the rail's own keyboard nav is in scope for RUL-001).
- Diff viewer hardening / polish.
- File-content search feature and search-result markers in the rail (RUL-001 includes the rail foundation and hunk-marker style placeholder; the search markers themselves are deferred).
- Rail in `FileView` itself (markdown/media surfaces). The rail stays inside `DiffView` and `FullFileView` for now.

## Delivery Strategy

- One work package (RUL-001) with two review units (RU1 foundation, RU2 diff-hunk markers) plus one deferred placeholder (RU3 search markers, blocked).
- Each review unit is independently mergeable; the open-stack cap is `target <=2, max 3`. With one ready review unit at a time on `develop`, the cap is comfortably respected.
- Jira policy: `optional` with the existing `jira-env-not-configured` fallback (no Jira lookup, record the omission in the release closeout).
- Local fast-forward merge into `develop` and push, no PR.
- **Review path:** dev-only browser fixture at `/demo.html` (see work package). Tauri app review is not used for this iteration.
