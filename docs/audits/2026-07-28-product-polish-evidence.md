---
evidence_schema_version: 1
audit_date: "2026-07-28"
source_commit: "9897dc023759dce1e3a29c5dc016f644dd60fa2f"
source_state: "dirty working tree"
atlas: "docs/product/application-atlas.md"
atlas_status: "stale"
baseline_atlas_fingerprint: "sha256:9022ba30a85237722b2976b4bc9757a2e4e2bb9c48afb056af188cee996120a0"
observed_working_tree_fingerprint: "sha256:1ce1410a2103505c289041be8ea08de911e589f17848507ba9a77dfd8a309eb9"
environment: "Windows host; visible native Tauri development build; Pumarejo MCP over stdio"
---

# Product polish evidence packet

This is the frozen common evidence packet for the twelve independent product-polish evaluators. It records facts, source references, observed behavior and audit limits. It does not contain findings, priorities or recommendations.

## 1. Provenance and freshness

- Repository branch: `develop`.
- Source commit: `9897dc023759dce1e3a29c5dc016f644dd60fa2f`.
- The product working tree already contained ten changes before this audit:
  - modified: `.gitignore`, `docs/orchestration/compound-master-state.md`, `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `src/App.css`;
  - untracked: `.pumarejo.json`, `docs/review-findings/2026-07-21-rdm-023-roadmap-review.md`, `docs/roadmaps/2026-07-21-010-provider-neutral-mcp-layer-roadmap.md`, `src/windowLayout.test.ts`.
- The atlas freshness check returned `stale`: its verified source remains `8b7c4c85a3e561eb60f68b10678b59bb45919ccd`, while repository `HEAD` and the working-tree fingerprint differ.
- The atlas was updated by factual diff for the observations in this packet, but its baseline commit and fingerprint were intentionally not replaced.
- This evidence therefore supports a reduced working-tree review, not an exhaustive product certification.

## 2. Declared product intent

- Primary user: a local developer supervising repositories and coding-agent sessions.
- Product promise: keep that developer aware of what changed, where, when and during which agent turn, without requiring a heavy editor or losing context (`README.md:12-28`).
- Co-primary actions:
  1. observe changes through Dashboard, Live Diff and Timeline;
  2. direct sessions through Agents and inspect them through Agent Lens.
- Declared success: visible repository activity/diff/timeline updates plus a completed agent turn with a verifiable checkpoint.
- Supported product platforms: Windows and Linux. WSL is a Windows execution/repository boundary. macOS is technically compilable but not officially supported.
- Deliberate limits: monitoring is passive, local, factual and read-only; workspace mutations and agent/runtime actions require explicit user initiation (`README.md:30-65`).
- No quantitative product north-star metric is declared in the current product documents.

## 3. Pumarejo connection and contract

Pumarejo was an essential evidence source in this run.

| Fact | Evidence |
|---|---|
| The local Pumarejo project was located and its real MCP server was started as a child process over `stdio`. | Pumarejo `README.md`; `src/mcp/server.ts` |
| The MCP client used the Model Context Protocol SDK with `StdioClientTransport`; no HTTP endpoint or simulated browser adapter was used. | Runtime observation |
| The server exposed exactly seven tools: `tauri_launch`, `tauri_snapshot`, `tauri_screenshot`, `tauri_click`, `tauri_type`, `tauri_press_key`, `tauri_close`. | MCP `tools/list` response |
| Tinto's `.pumarejo.json` launches `npm run tauri -- dev --features pumarejo --config {tauriConfig}`, targets the `main` window and disables retained artifacts. | `.pumarejo.json:1-19` |
| The WebDriver plugin is optional and is registered only when both `debug_assertions` and feature `pumarejo` are active. | `src-tauri/Cargo.toml:26-31`; `src-tauri/src/lib.rs:1-15,104` |
| Every interaction invalidates earlier element references; a new semantic snapshot was taken after each action. | Pumarejo MCP contract and run procedure |
| The visible owned session closed through `tauri_close` with `{ alreadyClosed: false, state: "idle" }`, after which the MCP client was closed. | Runtime observation |

The Codex shell did not initially expose `node`/`npm` on its direct `PATH`. The packaged Node runtime directory was explicitly prepended for the Pumarejo child and for frontend verification. This was a harness-environment condition, not an observed Tinto product error.

## 4. Methods and common scope

| Input | Disposition |
|---|---|
| Native runtime | Visible Tauri development window launched and owned by Pumarejo |
| Native sizes | 800×600 default; 1920×1032 maximized |
| Inputs | Pumarejo semantic-reference clicks, text entry, bounded key actions and screenshots |
| Product data | Current local workbench with two repositories and existing recent sessions; user content was not copied into this artifact |
| Code/config | React/TypeScript, Rust/Tauri, tests, product documentation and the current working-tree diff |
| Screenshots | Inspected during the run; not persisted in the repository |
| Pumarejo artifacts | Retention disabled; no audit screenshot bundle retained |
| Verification | Vitest, ESLint, production frontend build and Rust `cargo check` with the Pumarejo feature |

### Material flows sampled natively

- `FLOW-001` — native debug bootstrap.
- `FLOW-009` — passive monitoring presentation and live status.
- `FLOW-010` — matched filter, unmatched filter, reset and project open.
- `FLOW-011` — repository status overview.
- `FLOW-012` / `FLOW-013` — open a small changed file and inspect its diff.
- `FLOW-015` — cross-repository Timeline list and one commit detail.
- `FLOW-021` — Agent launch/history surface only; no process was started.
- `FLOW-030` — View/Help menus and keyboard-shortcuts dialog.
- `FLOW-032` — Pumarejo launch, observation, interaction and clean close.

### Flows not executed

- Workbench/repository creation, rename, removal or deletion.
- Fetch, add-on installation, external network actions and native notifications.
- File copy, move, overwrite, delete, undo, restore or other repository mutation.
- Agent launch, turn submission, queue/steer, stop, restore, revert or journal deletion.
- Detached windows, WSL, Linux native and release/installer flows.

## 5. Native observations

All observations below were made on 2026-07-28 through the real Pumarejo MCP unless marked as code evidence.

| Evidence ID | Surface/flow | Exact observation |
|---|---|---|
| `OBS-2026-07-28-001` | Bootstrap / Pumarejo | `tauri_launch` created a visible 800×600 Tinto session and returned a semantic generation for the `main` window. The shell exposed Workbench, Repos, Proyectos, Ver, Complementos, Ayuda and native window controls. |
| `OBS-2026-07-28-002` | Dashboard 800×600 | The summary showed two repositories. At the default width, repository rows used the narrow reflow and remained within the visible content area. |
| `OBS-2026-07-28-003` | Dashboard 1920×1032 | Maximized, the same repositories used the wide table layout with status, metrics, signals and actions visible without observed horizontal overflow. |
| `OBS-2026-07-28-004` | Dashboard filter | Entering `tinto` in `Buscar` left one repository button named `tinto`; `Restablecer` restored both repositories. |
| `OBS-2026-07-28-005` | Dashboard unmatched filter | Entering a non-matching audit string rendered `Ningún repo coincide con los filtros actuales.` while retaining an enabled `Restablecer` action. |
| `OBS-2026-07-28-006` | Dashboard state | A live status region reported that `tinto` had updated with ten changed files and two signals. A contextual Gitleaks status announced that the basic detector was active after Gitleaks failed. |
| `OBS-2026-07-28-007` | Dashboard accessible names | With two repositories visible, the semantic snapshot contained two controls named `Actualizar`, two comboboxes named `Tipo de Agent` and two buttons named `Iniciar`. Repository open/remove controls included repository context in their names. |
| `OBS-2026-07-28-008` | View menu | Opening Ver set `aria-expanded=true`, focused `Abrir resumen`, exposed a named `menu`, ordinary menu items and checked-state menu-item checkboxes. |
| `OBS-2026-07-28-009` | Timeline list | The cross-repository Timeline combined events and commits from multiple calendar dates. Each visible row displayed a localized time of day but no date or day grouping. |
| `OBS-2026-07-28-010` | Timeline detail | Activating a commit selected its row, moved visible context to a commit detail and rendered file/diff content. A subsequent semantic snapshot of the large detail returned Pumarejo `INTERNAL_ERROR`; the screenshot itself succeeded. |
| `OBS-2026-07-28-011` | Agents overview | Agents rendered `No hay Agents activos`, contextual quick-launch controls for each repository/provider and a scrollable recent-session list. No agent was launched and no transcript was opened. |
| `OBS-2026-07-28-012` | Keyboard shortcuts | Ayuda → Atajos de teclado opened a dialog named `Atajos de teclado`, focused its named `Cerrar` button and exposed an `h2` plus grouped `h3` headings. Escape closed the dialog and returned focus to Ayuda. |
| `OBS-2026-07-28-013` | Repository/explorer | Opening `tinto` exposed a semantic project tree with folder/file `treeitem` nodes, changed/untracked markers, configuration statuses, signals, watched-pattern controls and dated commit history. |
| `OBS-2026-07-28-014` | Small file diff | Opening the modified `.gitignore` rendered an inline diff in a region named `Contenido del archivo .gitignore`, with controls for `Cambios`, `Archivo completo`, `En línea` and `Lado a lado`. |
| `OBS-2026-07-28-015` | Dock tabs | Top-level Resumen, Cronología and Agents headers, and later project/file headers, were exposed in semantic snapshots as focusable/clickable `div` controls without `tab` role or an accessible name. Clicking the Resumen header focused it but did not activate the summary; Ver → Abrir resumen did activate it. |
| `OBS-2026-07-28-016` | Key-driver coverage | Repeated `TAB` actions while Agents was open left the reported focus on `Minimizar`. The run did not establish whether this came from the application, WebView driver or Pumarejo key dispatch. |
| `OBS-2026-07-28-017` | Instrumentation isolation | Tinto launched successfully with feature `pumarejo`; source guards keep the WebDriver plugin out of normal/non-debug builds. |
| `OBS-2026-07-28-018` | Window/layout working tree | The current config sets minimum window size 640×480; CSS moves the dashboard to narrow reflow at 820 px and changes wide-column proportions. The native 800 px and maximized layouts both rendered. |
| `OBS-2026-07-28-019` | Semantic-result limit | A large Timeline commit detail exceeded the usable semantic-result path, while screenshots and smaller semantic snapshots continued to work after a fresh session. This limits evidence coverage for that large detail. |
| `OBS-2026-07-28-020` | Cleanup | The final summary state again showed both repositories and no alert; `tauri_close` then returned the owned session to `idle`. |

## 6. Code and declared evidence

| Evidence ID | Fact | Source |
|---|---|---|
| `CODE-2026-07-28-001` | Timeline activity and commit rows format their timestamps with `toLocaleTimeString()` only. | `src/panels/timeline/TimelinePanel.tsx:335,357` |
| `CODE-2026-07-28-002` | Repository Agent selectors use static `aria-label="Tipo de Agent"` and launch buttons render `Iniciar`; fetch buttons visibly render `Actualizar` while contextual text is only in `title`. | `src/panels/RepoCard.tsx:188,216,483-491` |
| `CODE-2026-07-28-003` | File-view mode buttons do carry `aria-pressed` states and focused unit tests assert them, although those states were absent from the Pumarejo snapshot payload. | `src/panels/file/FileView.tsx:242-273`; `src/panels/file/FileView.test.tsx:173-180` |
| `CODE-2026-07-28-004` | Dashboard layout reflows at container width 820 px; the main window has 640×480 minimum dimensions. | `src/App.css:3779`; `src-tauri/tauri.conf.json:14-22` |
| `CODE-2026-07-28-005` | Pumarejo instrumentation is an optional dependency and debug-feature wrapper around the normal Tauri builder. | `src-tauri/Cargo.toml:26-31`; `src-tauri/src/lib.rs:1-15,104` |
| `CODE-2026-07-28-006` | The new layout test checks window minimums, the 820 px reflow threshold, wide column proportions and metric-label sizing. | `src/windowLayout.test.ts:1-26` |
| `CODE-2026-07-28-007` | View-menu commands provide an alternate semantic path to open Summary and Timeline. | `src/workspace/openDashboard.ts`; `src/workspace/openTimeline.ts`; `src/workbench/MenuBar.tsx` |
| `DECL-2026-07-28-001` | Monitoring must remain passive, local, factual and read-only; mutations belong to explicit tools/actions. | `README.md:30-65`; `tinto-design.md` |

## 7. Automated verification

| Check | Result |
|---|---|
| Pumarejo native launch and close | visible launch succeeded; final close returned `state: idle` |
| Vitest `npm test` | 54 files passed; 714 tests passed; exit 0; duration 299.97 s |
| ESLint `npm run lint` | exit 0 |
| Production frontend build `npm run build` | 450 modules transformed; exit 0 |
| Rust `cargo check --manifest-path src-tauri/Cargo.toml --features pumarejo` | exit 0 |

## 8. Restrictions and privacy handling

- The existing local workbench and recent-session overview were visible because the audit used the current native profile.
- Repository paths, conversation excerpts and other user-specific content observed in semantic payloads were not copied into this evidence packet.
- No credentials, secret values, authentication stores or external endpoints were inspected.
- No product mutation, agent/runtime start, turn submission, network action or OS-level destructive action was authorized or executed.
- Only a small existing changed text file was opened; its content was not edited.
- Pumarejo artifact retention remained disabled, and no screenshots were added to the repository.

## 9. Verification gaps

- The atlas is stale against a dirty working tree; a clean committed source fingerprint is unavailable.
- Linux native, WSL, macOS, installer and release-bundle behavior.
- Real first-run/zero-repository setup, higher repository volume and long-running live updates.
- Native degradation/reconnect, Timeline failure recovery and very large Timeline details.
- Real Agent launch, authenticated providers, send/queue/steer, checkpoints, stop, restore and revert.
- File/workbench/repository mutations, native pickers, add-on installation, fetch and notifications.
- Detached windows and cross-window continuity.
- NVDA/Orca or another real screen reader, high contrast, touch and reduced-motion runtime checks.
- Pumarejo did not yield a usable semantic snapshot for one large commit detail, and its key-driver focus result was inconclusive.

## 10. Coverage-gate disposition

The gate is accepted only for the explicitly sampled Windows-native surfaces, code paths and automated checks in this packet. It is not accepted for exhaustive product polish because the atlas remains stale and material platforms, high-consequence flows, real-agent paths and assistive-technology checks remain uncovered.
