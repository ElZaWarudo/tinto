# Windows native regression evidence — 2026-08-27

## Scope and baseline

- Repository: Tinto
- Baseline commit: `199a9aeba05f36cef3fd959fe2f885f569d2cca0`
- Checkout state during evidence capture: baseline commit plus the uncommitted,
  root-owned Windows E2E cleanup correction in `scripts/run-tauri-e2e.mjs` and
  documentation-only gap-closure artifacts. The RDM-024 product implementation
  had not started when the Pumarejo observations below were captured.
- Host: Microsoft Windows 11 Pro `10.0.26200` (build `26200`)
- WebView2/Edge runtime: `151.0.4129.93`
- Node/npm: `v24.13.0` / `11.6.2`
- Rust/Cargo: `1.97.1` / `1.97.1`

This record distinguishes observed behavior from unverified behavior. It does
not treat a DOM snapshot or a build result as evidence for an interaction that
was not exercised.

## Automated native-shell gate

Command:

```text
npm run test:e2e:tauri
```

The cold run built the frontend and Tauri executable and the real WebDriver IPC
spec passed (`1 passing`). The command still exited non-zero because Windows
briefly retained a WebView state-directory handle and the runner's single
recursive removal returned `EPERM`.

The bounded correction retries only `EPERM`, `EBUSY`, and `ENOTEMPTY` while
removing the runner-created direct temp child. It makes no application change,
kills no unrelated process, and retains the existing path/prefix safety check.
After that correction the same command completed with exit code 0, including
the real IPC spec and cleanup.

## Pumarejo native observation

Pumarejo launched Tinto with the project-approved debug feature and owned the
application, WebDriver, proxy, and cleanup lifecycle. The first run exposed a
blank WebView because this checkout's installed packages were stale
(`vite@7.3.5` and `@vitejs/plugin-react@4.7.0`) while the committed lockfile
requires Vite `8.2.2` and plugin `6.x`. A lockfile-only `npm ci` restored
`vite@8.2.2` and `@vitejs/plugin-react@6.1.0`; no product code or dependency
manifest changed. Subsequent Pumarejo runs rendered the application normally,
used an owned Tinto PID, and returned to `idle` after `tauri_close`.

Observed at the configured 800x600 native window:

- Dashboard: the `Resumen` tab rendered live workbench data, repo states,
  filters, passive status messages, `Añadir repo`, and `Abrir Agents` controls.
- Timeline: `Ver` > `Abrir cronología` opened a selected `Cronología` tab with
  a named `Entradas de la cronología` region, dated groups, commit buttons, and
  an explicit `Sin commit seleccionado` detail state.
- Agent entry points: the Dashboard exposed `Abrir Agents`, per-repository
  provider selectors (`Codex`, `Claude Code`, `Kimi Code`, `OpenCode`), named
  launch buttons, and a visible availability-checking state. No provider
  process was launched during this read-focused regression.
- Keyboard: after opening and dismissing the `Ver` menu, a WebDriver `TAB`
  action placed focus on the named `Ver` menu item. The Timeline capture did
  not retain a focused semantic node after a separate `TAB`, so broader
  Timeline traversal remains unverified rather than inferred.
- Native lifecycle: the title bar exposed named minimize, maximize, and close
  controls. Pumarejo's WebDriver maximize operation returned
  `UNSUPPORTED_ACTION`; restore reported the original 800x600 window. This is
  a harness disposition, not evidence that the visible maximize button fails.

## Flow disposition

| Flow | Disposition | Evidence boundary |
| --- | --- | --- |
| Native launch and real IPC | Passed | Cold Tauri build, WebDriver session, real IPC spec, and cleanup all exited 0 after the bounded cleanup correction. |
| Dashboard action surface | Passed | Live 800x600 semantic snapshot contained named filters, repo cards, status messages, `Añadir repo`, and `Abrir Agents`. |
| Timeline open/read | Passed | `Ver` > `Abrir cronología` produced a selected Timeline tab, named live region, dated groups, and commit controls. |
| Keyboard | Partial | `TAB` focus on the named `Ver` menu item was observed; full traversal across Timeline and Agents was not exercised. |
| Responsive/native resize | Unverified | The 800x600 configured surface rendered; Pumarejo reported maximize as unsupported and could not supply a second effective size. |
| Agent launch/session | Partial | Provider selectors, named launch controls, and availability state rendered; starting an external agent process was intentionally not part of this read-focused run. |
| Detached terminal/consoles | Unverified | No safe existing terminal/session fixture was present, so detach and reattach were not claimed. |

## Result

The repaired native gate is current and repeatable on the baseline commit. The
Dashboard and Timeline regressions are directly observed. Keyboard, responsive
resize, live Agent session, and detached-window coverage remain explicitly
partial or unverified; they are not release blockers for the bounded RDM-024
implementation, but they must not be represented as current passing evidence.

## Post-implementation addendum

After the RDM-024 implementation and review fixes were present in the working
tree, `npm run test:e2e:tauri` passed again with the real Tauri IPC spec (`1
passing`) and bounded cleanup. The run used WebView2 `151.0.4129.107`.

A current Pumarejo retry launched the combined build, reached `ready` with an
owned Tinto process and WebDriver, rendered the configured 800x600 surface,
observed the Dashboard/Timeline navigation and semantic Timeline content, and
closed back to `idle`. WebDriver maximize again returned
`UNSUPPORTED_ACTION`; restore retained 800x600. A targeted Agents retry could
not reach the new MCP Details content because the saved main tab reopened on
Timeline, and no external Agent session was started merely to manufacture an
audit fixture. Therefore the MCP management surface is supported by code and
focused tests, not claimed as post-change Pumarejo-observed behavior.

This addendum does not change the baseline commit: the implementation remains
uncommitted. It does establish current combined-tree native IPC and bounded
lifecycle evidence without expanding the original partial/unverified claims.
