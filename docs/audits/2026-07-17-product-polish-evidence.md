---
evidence_schema_version: 1
audit_date: "2026-07-17"
source_commit: "8b7c4c85a3e561eb60f68b10678b59bb45919ccd"
atlas: "docs/product/application-atlas.md"
atlas_fingerprint: "sha256:9022ba30a85237722b2976b4bc9757a2e4e2bb9c48afb056af188cee996120a0"
environment: "Windows host; Vite browser-only preview; Codex in-app browser"
---

# Product polish evidence packet

This is the frozen common evidence packet for the twelve independent product-polish evaluators. It records facts and audit constraints, not findings or recommendations.

## 1. Provenance and declared intent

- Repository: `C:\Users\User\Documents\personal\tinto`
- Branch/commit: `develop` at `8b7c4c85a3e561eb60f68b10678b59bb45919ccd`.
- The working tree was clean before creating the atlas and audit artifacts.
- Primary user [declared]: a local developer supervising repositories and coding agents.
- Product promise [declared]: keep the developer aware of repository changes and agent sessions in real time, without requiring a heavy editor or losing the thread (`README.md:12-28`).
- Primary intentions [declared by the product owner on 2026-07-17]: both are co-primary and complementary:
  1. Observe changes through Dashboard, Live Diff and Timeline.
  2. Direct sessions through Agents and inspect them through Agent Lens.
- Success signal [declared by the product owner on 2026-07-17]: visible repository activity/diff/timeline updates plus a completed agent turn with a verifiable checkpoint.
- Supported product platforms [declared]: Windows and Linux. WSL is a Windows boundary. macOS is not officially supported; it is only a technically compilable target.
- Deliberate limits [declared]: the monitoring loop is passive, local, factual and read-only; mutable workspace tools require explicit user initiation (`README.md:30-65`).

## 2. Audit methods and inputs

| Input | Disposition |
|---|---|
| Atlas | `docs/product/application-atlas.md`; validated for the reduced scope below |
| Browser runtime | Vite 7.3.5 at `http://127.0.0.1:1420` |
| Browser | Codex in-app browser |
| Desktop viewports | 1280×720 and 768×900 |
| Narrow viewport | 390×844 |
| Input methods | Pointer actions and keyboard actions through the browser automation surface |
| Code/config | React/TypeScript sources, Rust/Tauri sources, tests and product documentation at the source commit |
| Screenshots | Visually inspected during the run; not persisted as repository artifacts |
| Test data | Synthetic fixture data only; no real repositories, credentials or user profile data were read |

## 3. Scope

### Material flows examined directly or through fixtures

- `FLOW-001` — bootstrap and connection failure.
- `FLOW-009` — passive monitoring presentation through the dashboard fixture.
- `FLOW-010` — filter/reset and dashboard navigation.
- `FLOW-013` — file inspection and overview-ruler navigation.
- `FLOW-021` / `FLOW-022` — agent launch/send surfaces, without starting an agent or submitting a turn.
- `FLOW-023` — working-turn queue/steer controls, without submission.
- `FLOW-027` — completed and journal conversation presentation.
- `FLOW-030` — responsive and keyboard behaviors represented by fixtures.

### High-consequence or native flows examined only in code

- `FLOW-016` fetch, `FLOW-017` add-on installation, `FLOW-019`/`FLOW-020` file mutation, `FLOW-025`/`FLOW-026` stop/revert/restore, `FLOW-028` journal deletion, `FLOW-029` detach/reattach and `FLOW-031` notifications.
- Windows/Linux native shell, WSL, OS dialogs, native notifications, real Git changes and real agent processes were not executed.

## 4. Browser observations

All rows below are `observed` on 2026-07-17 unless marked otherwise.

| Evidence ID | Surface/flow | Exact observation |
|---|---|---|
| `OBS-001` | `/`, `FLOW-001`, `SURF-002` | The browser-only root rendered `Tinto no pudo conectarse`, alert text `Cannot read properties of undefined (reading 'invoke')`, and a `Reintentar conexión` button. Console errors included repeated missing `transformCallback` calls from bus listeners and detached-console reattach setup. |
| `OBS-002` | `dashboard-review.html`, `SURF-005` | At 1280×720 the dashboard rendered three repositories, status/metric columns, repository actions and Agent selectors. Document `scrollWidth` equaled `clientWidth` (1280). |
| `OBS-003` | `FLOW-010`, `SURF-006` | Filling search with `tinto` reduced the three articles to `Repo tinto`; `Restablecer` restored all three. |
| `OBS-004` | `SURF-004` | The Workbench menu opened with `Producto` checked and `Gestionar workbenches…`; Escape closed the menu and returned focus to Workbench. |
| `OBS-005` | Dashboard loading | `dashboard-review.html?state=loading` kept the dashboard frame visible, blurred the content and exposed status `Cargando repos`. |
| `OBS-006` | Dashboard responsive | At 768×900 all three articles were 768 px wide and the document had no horizontal overflow. At 390×844 articles were 380 px wide, the document had no horizontal overflow and no button/input/select extended beyond the viewport. |
| `OBS-007` | Agent Lens completed | `agent-lens-restorable.html` rendered two turns, completed/waiting/checkpoint/change statuses, disabled composer controls, search, result navigation, Details, and a disabled Stop action with enabled Revert. |
| `OBS-008` | Agent Lens search | Search `restore` reported `2 de 2 turnos`; Next selected `2 / 2`; Escape cleared the search and disabled Previous/Next. |
| `OBS-009` | Agent Lens details | Details opened a complementary inspector with session metrics, turn map, restore-point metric `2/2`, Files/Commands/Timeline tabs and `Restaurar aquí`; selecting Timeline rendered two events; Close removed the inspector. |
| `OBS-010` | Agent Lens working | `?state=working` rendered `En ejecución`, `Trabajando`, enabled Stop, disabled Revert, a pending command status, enabled composer/preset/Fast controls and disabled send/queue/steer while the message was empty. |
| `OBS-011` | Journal mode | `?mode=journal` removed Stop/Revert from the header and rendered an enabled composer with helper text `Escribe un mensaje para retomar esta conversación archivada.` |
| `OBS-012` | Compact Agent Lens | `?viewport=compact` produced a 499 px client-wide panel. Turn articles reached x=527.39; `Copiar turno` reached x=525.39; message copy controls reached x=520.39–527.39. |
| `OBS-013` | Agent navigator | `?surface=navigator` rendered two conversation entries and the active transcript. At a 1280 px viewport, turn articles reached x=1318.39 and turn/message copy/edit controls reached x=1284.39–1318.39. |
| `OBS-014` | Agent runtime fixture | `agent-runtime.html` rendered the runtime preset dialog correctly when opened. Before opening it, the message textarea measured 38×66.59 px and the adjacent `Enviar` button measured 1010×66.59 px at a 1280 px viewport. |
| `OBS-015` | File overview ruler | `demo.html` exposed a slider, 16 secret markers and keyboard guidance. Pressing End on the slider set `aria-valuenow=80`, `aria-valuetext=\"Línea 80 de 80\"`, `topLine=61` and `activeLine=80`. |
| `OBS-016` | Consecutive markers | Pointer-clicking marker line 7 activated line 7. Pointer-clicking marker line 13 activated/focused line 14. Pointer-clicking marker line 57 activated line 59. Lines 57, 58 and 59 each had 18 px-high marker boxes whose top positions were 455.83, 459.88 and 463.92 px, respectively. |

## 5. Code and declared evidence

| Evidence ID | Fact | Source |
|---|---|---|
| `CODE-001` | The root App calls Tauri-backed bus configuration/snapshot functions before selecting loading, failure, first-run or workspace surfaces. | `src/App.tsx:217-305`; `src/bus/connection.ts:36-118` |
| `CODE-002` | Browser-only missing event bridges are intended to degrade to no-op subscriptions, while unexpected setup failures reject. | `docs/contracts/bus-contract.md:30`; `src/bus/client.ts:110-150` |
| `CODE-003` | Dashboard includes loading, zero-repo, filtered-empty, degraded and populated states. | `src/panels/DashboardPanel.tsx:41-310` |
| `CODE-004` | Text diff handles binary, oversized, long-line, render-cap and no-hunk states. | `src/panels/diff/DiffView.tsx:80-165`; `src/panels/diff/limits.ts:1-2` |
| `CODE-005` | Dialog infrastructure includes focus entry, Escape, Tab trap, inert background and focus restoration. | `src/workbench/useAccessibleDialog.ts:3-174` |
| `CODE-006` | File overview ruler exposes slider semantics, Arrow/Page/Home/End/Escape keys and marker buttons. | `src/panels/file/FileOverviewRuler.tsx:73-121,153-251` |
| `CODE-007` | Agent sessions expose working/waiting/settling states, transcript search, composer, journal, restore and Agent Lens views. | `src/panels/terminal/TerminalPanel.tsx:840-990,1356-2028,3020-3135,4274-4994` |
| `CODE-008` | Destructive agent restore/revert paths require explicit consent in the backend. | `src-tauri/src/agent_console/mod.rs:563-655` |
| `CODE-009` | File delete uses a temporary backup and undo token; overwrite uses an explicit confirmation path. | `src-tauri/src/file_ops/commands.rs:328-506`; `src/panels/file/OverwriteConfirmModal.tsx:15-83` |
| `CODE-010` | Responsive/container-query rules, focus-visible rules and reduced-motion handling exist in the main stylesheet. | `src/App.css:790,1154,1883-1891,3341,7019,9921-9928,10529` |
| `DECL-001` | The browser-only preview is meant for responsive visual QA and fixtures, not Rust commands. | `docs/build-guide.md:363-401` |
| `DECL-002` | The current product ships dashboard, diff, file tree, timeline, workbenches, WSL, Agent surfaces, glance and notifications. | `README.md:89-166` |

## 6. Automated verification

| Check | Result |
|---|---|
| TypeScript `tsc --noEmit` | exit 0 |
| ESLint `eslint .` | exit 0 |
| Vitest | 49 files passed; 638 tests passed; exit 0; duration 125.25 s |

## 7. Restrictions applied

- No real user profile, Tinto config, SQLite journal, authentication data or repository content was read.
- No Agent/Codex/Claude/OpenCode process was started.
- No turn was submitted and no external/model/network call was made.
- No fetch, add-on install, notification permission, WSL operation, file mutation, workbench/repository mutation, delete, overwrite, restore or revert action was executed.
- A browser-only Vite server was the only live product process used.

## 8. Verification gaps

- Native startup and end-to-end flows on supported Windows and Linux builds at the source commit.
- WSL behavior; WSL2 was enabled on the host but no distribution was installed.
- Real repository monitoring, Live Diff update timing and Timeline causality.
- Native workbench/repository dialogs, file operations, fetch, notifications and detach/reattach.
- Real Agent launch, turn completion, checkpoint creation, stop, restore and revert.
- Screen reader, accessibility tree inspection beyond DOM semantics, high contrast, touch and reduced-motion runtime checks.
- Maximum tree/file/diff/session volumes and extreme content envelopes.
- macOS behavior is out of scope because macOS is not an officially supported platform.
