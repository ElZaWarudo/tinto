---
date: 2026-06-10
topic: rdm-001-esqueleto-tauri
---

# RDM-001 — Esqueleto Tauri 2 + React: requisitos

## Summary

Crear el esqueleto del proyecto Tinto: app Tauri 2 con frontend React + TypeScript (Vite) y backend Rust con tokio, que compila y abre una ventana vacía en Windows y Linux, con el puente webview↔Rust probado en ambas direcciones y el tooling de calidad baseline configurado.

## Key Decisions

- **TypeScript en el frontend.** El contrato de eventos backend↔frontend (RDM-006) define todo el frontend; tiparlo desde el inicio evita drift. Decisión de usuario 2026-06-10.
- **npm con `ignore-scripts=true`.** Package manager estándar sin ejecución de lifecycle scripts de dependencias (`.npmrc` con `ignore-scripts=true`), como endurecimiento de supply chain. Decisión de usuario 2026-06-10.
- **Repo GitHub ahora, CI después.** Se crea el repo remoto en GitHub (privado) y se pushea `main` para habilitar el flujo de PRs. El workflow de CI se difiere a un item posterior del roadmap. Decisión de usuario 2026-06-10 — actualiza la decisión D2 del roadmap.
- **Scaffold estándar de Tauri 2.** `create-tauri-app` con template React-TS: layout `src/` (frontend) + `src-tauri/` (backend Rust). Es el camino soportado por el ecosistema; no hay fork real que justifique otra estructura.

## Requirements

**Esqueleto y build**

- R1. El repo contiene una app Tauri 2 con frontend React + TypeScript (Vite) — la versión de React es la que scaffoldee el template estable de `create-tauri-app` (esperada: 19) — y backend Rust, con el layout estándar `src/` + `src-tauri/`.
- R2. `npm run tauri dev` abre una ventana de escritorio funcional en Windows y Linux.
- R3. `npm run tauri build` produce un binario sin errores en ambas plataformas.
- R4. El backend Rust tiene tokio configurado como async runtime, listo para que items posteriores registren tareas async.

**Puente webview↔Rust (humo)**

- R5. Existe un comando `invoke` de humo (p. ej. `ping` que retorna un payload con timestamp): el frontend lo llama y recibe la respuesta estructurada del backend.
- R6. Existe un evento `emit` de humo (p. ej. un tick periódico con timestamp): el backend lo emite y el frontend lo recibe con un listener registrado.

**Tooling de calidad**

- R7. El frontend tiene ESLint + Prettier configurados y un script `npm run lint` que pasa.
- R8. El backend pasa `cargo fmt --check` y `cargo clippy` sin warnings, tras limpiar lo que el scaffold genere; supresiones puntuales (`#[allow]`) solo con justificación en el código.
- R9. Hay tests de humo ejecutables: `cargo test` (backend) y `npm test` con Vitest (frontend), ambos en verde.
- R10. `.npmrc` fija `ignore-scripts=true`; las dependencias se instalan sin ejecutar lifecycle scripts.
- R11. `.gitignore` cubre `target/`, `node_modules/`, `dist/` y artefactos de build de Tauri.

**Entrega**

- R12. Como paso de entrega (no de implementación del esqueleto): existe un repo GitHub privado como remote `origin` con `main` pusheado. Si `gh` no está disponible/autenticado, R12 queda como blocker de entrega sin invalidar R1–R11.

## Acceptance Examples

- AE1. **Covers R5.** Al arrancar la app en dev, el frontend invoca el comando de humo (`ping` o equivalente) y muestra la respuesta del backend en la ventana.
- AE2. **Covers R6.** Al arrancar la app en dev, el backend emite un evento de humo (por ejemplo, un tick con timestamp) y el frontend lo muestra al recibirlo.
- AE3. **Covers R2, R3.** En Windows (máquina de desarrollo actual), `npm run tauri dev` abre la ventana y `npm run tauri build` termina con binario generado — criterio duro de aceptación. La verificación Linux es best-effort en este item: si no hay máquina Linux disponible, queda documentada como pendiente y se cubre cuando exista CI (item posterior).

## Scope Boundaries

- Sin lógica de git, watcher, clasificador de paths, workbenches ni UI de producto (dashboard, diff viewer) — eso pertenece a RDM-002..012. El display mínimo que verifica R5/R6 en la ventana es instrumentación de humo, no "UI real".
- Sin workflow de CI en el repo (GitHub Actions): diferido a un item posterior por decisión de usuario. El tooling local de calidad (R7–R9) sí es parte de este item; CI solo orquestará después lo que aquí ya corre localmente.
- Sin íconos, branding ni configuración de updater/instalador más allá del default de `tauri build`.

## Dependencies / Assumptions

- Toolchain requerido en la máquina de desarrollo: Rust estable + Node LTS + prerequisitos de Tauri 2 por plataforma (WebView2 en Windows, webkit2gtk en Linux).
- La verificación cross-platform completa (AE3) puede requerir una máquina Linux; si no está disponible al ejecutar, se verifica Windows localmente y Linux queda pendiente documentado.
- `gh` CLI autenticado para crear el repo remoto (R12); si no está disponible, R12 se reporta como blocker de entrega, no de implementación.

## Outstanding Questions

- **Deferred to Planning:** estructura interna mínima del backend (módulos placeholder o un solo `lib.rs`/`main.rs`) — el plan decide según lo que el template genere.
- **Deferred to Planning:** verificar que `npm install` con `ignore-scripts=true` instala limpio el chain Vite/esbuild/Rollup del template (los esbuild/rollup modernos usan `optionalDependencies`, pero hay que confirmarlo en el template concreto y documentar excepciones si aparecen).
