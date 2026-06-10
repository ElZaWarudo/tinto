---
title: "feat: Esqueleto Tauri 2 + React + tooling base (RDM-001)"
type: feat
date: 2026-06-10
origin: docs/brainstorms/2026-06-10-rdm-001-esqueleto-tauri-requirements.md
---

# feat: Esqueleto Tauri 2 + React + tooling base (RDM-001)

## Summary

Crear el esqueleto de Tinto en la raíz del repo: app Tauri 2 con frontend React + TypeScript (Vite) y backend Rust con tokio, puente webview↔Rust probado en ambas direcciones (comando `ping` y evento tick), tooling de calidad local (ESLint/Prettier, rustfmt/clippy, Vitest, cargo test) y endurecimiento npm (`ignore-scripts=true`). La publicación a GitHub (R12) se prepara aquí y se ejecuta en la fase de entrega.

## Requirements Trace

- R1–R4 (esqueleto y build) → U1
- R7–R11 (tooling de calidad, npmrc, gitignore) → U2
- R5–R6, AE1–AE2 (puente de humo) → U3
- R12, AE3 (entrega) → U4 + fase de release
- Origin: ver `docs/brainstorms/2026-06-10-rdm-001-esqueleto-tauri-requirements.md`

## Key Technical Decisions

- **Scaffold oficial `create-tauri-app` con template `react-ts`** (see origin). El template actual del ecosistema scaffoldea React 19 + Vite; el plan adopta las versiones que genere el template estable del día de ejecución, sin pinnear manualmente versiones distintas. Resuelve la Outstanding Question "React 18 vs 19" → la que traiga el template (esperado: 19).
- **Scaffold vía directorio temporal.** `create-tauri-app` rehúsa directorios no vacíos y la raíz ya contiene `.git/` y `docs/`. Se scaffoldea en un subdirectorio temporal y se mueve el contenido a la raíz (preservando `.git/`, `docs/`). Directional, no choreography: el implementador puede usar otra vía si el tooling del día lo permite con resultado idéntico.
- **Async sobre el runtime de Tauri.** Tauri 2 trae su propio runtime tokio expuesto como `tauri::async_runtime`; las tareas se spawnean con `tauri::async_runtime::spawn` (no `tokio::spawn` directo) para integrarse con el event loop de la app. Se añade `tokio` como dependencia explícita solo por sus utilidades (`time` para intervalos; cargo unifica la versión con la de Tauri). R4 queda satisfecho: el runtime async disponible y probado es el de Tauri, que ES tokio. El evento tick de U3 valida el patrón.
- **`ignore-scripts=true` con verificación temprana.** El chain moderno Vite/esbuild/Rollup/@tauri-apps/cli distribuye binarios nativos vía `optionalDependencies` (sin postinstall), por lo que se espera compatibilidad; U1 lo verifica como primer paso tras el scaffold y documenta cualquier excepción en el propio `.npmrc` (assumption del origin, verificada en ejecución).
- **Sin CI en este item** (decisión D2 del roadmap, resuelta por usuario): la verificación es local; Linux queda best-effort hasta que exista CI.

## Output Structure

```text
tinto/
├── .npmrc                  # ignore-scripts=true
├── .gitignore              # node_modules/, dist/, target/, bundle de tauri
├── package.json            # scripts: dev, build, tauri, lint, format, test
├── vite.config.ts
├── tsconfig.json
├── eslint.config.js        # ESLint flat config
├── .prettierrc             # config Prettier
├── index.html
├── src/                    # frontend React+TS
│   ├── main.tsx
│   ├── App.tsx             # instrumentación de humo: ping + tick
│   ├── App.test.tsx        # Vitest
│   └── setupTests.ts       # setup jsdom/testing-library
├── src-tauri/
│   ├── Cargo.toml          # tauri 2, tokio, serde
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs
│       └── lib.rs          # comando ping + emisor de tick
└── docs/                   # ya existente (roadmap, brainstorms, plans, orchestration)
```

## Implementation Units

### U1. Scaffold Tauri 2 + React-TS en la raíz del repo

- **Goal:** App Tauri 2 con React+TS que compila y abre ventana en dev.
- **Requirements:** R1, R2, R3, R4.
- **Dependencies:** None.
- **Files:** `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `src/main.tsx`, `src/App.tsx`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `.npmrc`.
- **Approach:** Scaffold con `create-tauri-app` (template react-ts, manager npm) vía directorio temporal → mover a raíz preservando `.git/` y `docs/`. Crear `.npmrc` con `ignore-scripts=true` ANTES de `npm install` y verificar que la instalación queda funcional (esbuild/rollup/tauri-cli via optionalDependencies). Añadir `tokio` a `src-tauri/Cargo.toml` con features `rt`, `macros`, `time`. Identidad de app en `tauri.conf.json`: productName `Tinto`, identifier estilo `dev.tinto.app`.
- **Patterns to follow:** layout estándar del template oficial; no inventar estructura propia.
- **Test scenarios:** Test expectation: none — scaffolding puro; la verificación es de build/arranque (abajo). La cobertura de comportamiento llega en U3.
- **Verification:** `npm install` termina sin scripts ejecutados y sin errores; `npm run tauri dev` abre ventana en Windows; `cargo check` en `src-tauri/` pasa.

### U2. Tooling de calidad local

- **Goal:** Lint/format/test ejecutables y en verde en ambos lados.
- **Requirements:** R7, R8, R9, R10, R11.
- **Dependencies:** U1.
- **Files:** `eslint.config.js`, `.prettierrc`, `package.json` (scripts `lint`, `format`, `test`), `src/App.test.tsx`, `src/setupTests.ts`, `.gitignore`; limpieza clippy de lo que el scaffold haya dejado en `src-tauri/src/`.
- **Approach:** ESLint flat config + Prettier (archivo `.prettierrc`) para TS/React. Vitest configurado en `vite.config.ts` con bloque `test: { environment: 'jsdom', setupFiles: './src/setupTests.ts' }` y `@testing-library/react` + `@testing-library/jest-dom` como devDependencies — el template no trae tests; este bloque no interfiere con el build de Tauri porque solo aplica al runner. En Rust: `cargo fmt` aplicado y `cargo clippy -- -D warnings` limpio tras remover lo que el scaffold traiga de más; `#[allow]` solo con comentario justificando. `.gitignore`: `node_modules/`, `dist/`, `src-tauri/target/`, artefactos de bundle.
- **Test scenarios:**
  - Happy path: un test Vitest de humo que monta `App` y asserta que renderiza (smoke render).
  - Happy path: `cargo test` corre al menos un test unitario trivial del backend (p. ej. test del payload del ping definido en U3; si U2 se completa antes que U3, un test placeholder del módulo lib).
- **Verification:** `npm run lint`, `npm test`, `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` — todos en verde localmente.

### U3. Puente de humo invoke + emit

- **Goal:** Validar el puente webview↔Rust en ambas direcciones con instrumentación visible.
- **Requirements:** R5, R6; AE1, AE2.
- **Dependencies:** U1 (U2 deseable antes, para que el código nuevo nazca lint-clean).
- **Files:** `src-tauri/src/lib.rs` (comando `ping` + task tokio que emite `tick`), `src/App.tsx` (botón/efecto que invoca `ping` y listener de `tick`), `src/App.test.tsx` (tests con mock de `@tauri-apps/api`), `src-tauri/src/lib.rs` tests del payload.
- **Approach:** Comando `#[tauri::command] ping` que retorna payload estructurado `{ message, timestamp }` (serde). En `setup`, `tauri::async_runtime::spawn` de una task que emite evento `tick` con timestamp cada ~1s vía `AppHandle::emit` (intervalo con `tokio::time::interval`). Frontend: invoca `ping` al montar y muestra respuesta; registra `listen("tick")` y muestra el último tick. Tests frontend con el módulo oficial `@tauri-apps/api/mocks` (`mockIPC` para `invoke`; `clearMocks` en `afterEach`). Esto es instrumentación de humo (origin: no es "UI real"), pero define el patrón invoke/emit que RDM-006 generalizará.
- **Technical design (directional):** payload `PingResponse { message: String, timestamp_ms: u64 }`; evento `tick` con el mismo shape sin `message`.
- **Test scenarios:**
  - Covers AE1. Happy path (frontend): con `invoke` mockeado, `App` muestra el `message` del ping tras montar.
  - Covers AE2. Happy path (frontend): al disparar el callback registrado por `listen("tick")` con un payload de prueba, la UI refleja el timestamp.
  - Happy path (backend): test unitario de que el payload del ping serializa con los campos esperados.
  - Error path (frontend): si `invoke` rechaza, la UI muestra estado de error sin crashear.
- **Verification:** En `npm run tauri dev` (Windows) se ve la respuesta del ping y el tick actualizándose; `npm test` y `cargo test` en verde.

### U4. Preparación de entrega (GitHub remoto)

- **Goal:** Dejar el repo listo para el flujo de PRs del programa de entrega.
- **Requirements:** R12, AE3.
- **Dependencies:** U1–U3 implementados y verificados.
- **Files:** ninguno nuevo (acción de infraestructura, no de código).
- **Approach:** Este unit NO se ejecuta en fase de trabajo: la creación del repo GitHub privado (`gh repo create`, remote `origin`, push de `main`) y el commit del esqueleto son mutaciones externas que ejecuta la fase de release (krt-release-marshal: gitflow-knight para commits, marshal para remote/push) con aprobación. El plan lo registra para trazabilidad de R12. Si `gh` no está autenticado, R12 queda como blocker de entrega documentado sin invalidar U1–U3.
- **Test scenarios:** Test expectation: none — paso de infraestructura/entrega.
- **Verification:** `git remote -v` muestra `origin` GitHub privado y `main` pusheado (verificado en fase de release, no de trabajo). Covers AE3 en su tramo Windows; tramo Linux queda documentado como pendiente hasta CI.

## Scope Boundaries

- Igual que el origin: sin git engine, watcher, clasificador, workbenches ni UI de producto. El display de ping/tick es instrumentación de humo.
- Sin workflow de CI (decisión D2): la verificación de este plan es local.

### Deferred to Follow-Up Work

- Workflow de CI GitHub Actions Windows+Linux (item posterior del roadmap, según D2 resuelta).
- Íconos/branding/instalador más allá del default de `tauri build`.

## Open Questions

- Ninguna bloqueante. Las deferred del origin quedan resueltas así: versión de React → la del template (U1); estructura backend → `main.rs` + `lib.rs` del template (U1); compatibilidad `ignore-scripts` → verificación temprana en U1 con documentación de excepciones si aparecen.

## Risks & Dependencies

- **Toolchain local:** requiere Rust estable + Node LTS + prerequisitos Tauri (WebView2 ya presente en Windows 11; webkit2gtk solo aplica si se verifica Linux). Mitigación: U1 falla rápido y reporta el prerequisito exacto.
- **`ignore-scripts` rompe algún paquete del template:** improbable (optionalDependencies); mitigación: detectar en U1, documentar excepción mínima en `.npmrc`/README del repo y registrar el hallazgo.
- **Binario `tauri build` lento la primera vez:** esperado (compilación Rust completa); no es señal de error.

## Verification Strategy

Escalera local (sin CI): `npm run lint` → `npm test` → `cargo fmt --check` + `cargo clippy -- -D warnings` → `cargo test` → `npm run tauri dev` (humo visual AE1/AE2) → `npm run tauri build` (binario AE3-Windows).
