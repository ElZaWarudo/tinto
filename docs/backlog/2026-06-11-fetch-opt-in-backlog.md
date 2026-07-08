---
title: "Backlog: fetch opt-in para el git engine"
status: completed
date: 2026-06-11
relates_to: src-tauri/src/git/ (RDM-002, entregado read-only sin red en PR #2)
---

# Backlog — Fetch opt-in para el git engine

> Diseño preservado de la corrida 2026-06-11 (decisión de usuario: backlog, no ejecutar ahora). El git engine entregado (PR #2) es deliberadamente **sin red** (`git2 0.20`, sin features ssh/https, "Tinto nunca hace fetch/push"). Este item, si se activa, **enmienda esa decisión** y el principio "Local, sin red" del diseño (§1/§9) con una excepción acotada.

## Qué es

Una operación `fetch` **opt-in** (explícita, disparada por el usuario, nunca automática) que refresca las remote-tracking refs de un repo monitoreado, para que el ahead/behind del dashboard no quede silenciosamente stale. Único alcance de red admitido: refrescar refs. Sin gestión de remotes, push, clone, ni reporte entre máquinas (§9 sigue vigente).

## Decisiones ya tomadas (usuario 2026-06-11, 3 confirmaciones)

1. **Incluir el fetch opt-in** (califica deliberadamente el principio §1/§9; decisión de producto, no auto-otorgada por un doc).
2. **Reusar credenciales del sistema** vía callbacks de libgit2 (SSH agent / git credential helper). Tinto no pide ni almacena secretos propios. Sin paridad garantizada con el binario `git` (cobertura parcial de helpers/tokens; SSH requiere compilar `git2` con feature libssh2).
3. **Empaquetado aislado**: review unit/PR propio con gate de Security Sentinel (superficie de red/auth separada del núcleo read-only).

## Requisitos de seguridad (del review multi-persona 2026-06-11 — no negociables si se activa)

- **Remote no confiable:** Tinto monitorea repos que no creó; la URL de remote en `.git/config` es entrada potencialmente hostil. Surfacear el host exacto al usuario antes de fetchear; **scoping de credenciales al host confirmado** (el callback `credentials` compara el url que libgit2 le pasa contra el host confirmado y rehúsa si difieren; denegar redirects cross-host).
- **Verificación de host fail-closed:** contrato del callback `certificate_check` (git2 ≥0.21: `Result<CertificateCheckStatus, Error>`): solo `Ok(CertificateOk)` tras verificación afirmativa; `Err` ante fallo; **nunca `CertificatePassthrough`** (delega al default de libgit2, que procede en SSH → MITM). git2-rs **no** verifica `known_hosts` por ti: comparación manual del host-key (o crate helper), con fallback acotado = cert TLS para HTTPS + prompt de confianza explícito en primer uso SSH.
- **Sanitización de errores:** errores categorizados (`credencial-ausente` / `auth-rechazada` / `host-no-verificado` / `red-inalcanzable`) sin credenciales, sin URLs con userinfo inline, sin stdout del helper — en toda la cadena (Display + Debug + source). `remote_url` expuesto a UI con userinfo eliminado.
- **No-write:** el fetch no toca working tree ni índice; escribe solo en `.git` (refs y objetos), datos del remote tratados como no confiables.
- **Testing:** el transporte local/`file://` NO invoca los callbacks de credentials/certificate_check → los caminos de seguridad se testean ejercitando los closures directamente con entradas sintéticas; test fail-closed primero.

## Notas de integración con lo ya entregado

- El engine en develop (`src-tauri/src/git/git2_engine.rs`, trait con `branch_info`/`worktree_diff`/etc., error `GitError`) difiere en nombres del diseño original de este backlog; al activar, derivar el plan **del código real en develop**, no de los artefactos de la corrida descartada.
- Requiere cambiar `Cargo.toml`: `git2` con features ssh/https (hoy `0.20, default-features = false, vendored-libgit2`); evaluar bump a ≥0.21 por el contrato de `certificate_check`.
- Señal de staleness honesta: el tip commit time de la remote-tracking ref mide edad del commit, no recencia de sync; si se quiere "last synced", que el fetch registre su propio timestamp.

## Cuándo activarlo

Cuando haya evidencia de que el ahead/behind stale confunde al supervisor (o el usuario lo pida). Entrar por brainstorm corto → plan → package con RU única + Security Sentinel.

## Activacion 2026-07-08

Estado: completado como slice de producto con un limite de implementacion distinto al diseno original.

- Se agregaron los comandos aditivos `get_repo_fetch_preview` y `fetch_repo` para repos locales del workbench activo.
- El dashboard muestra `Fetch` solo cuando el repo local tiene upstream real; WSL y ramas sin upstream no exponen la accion.
- La UI obtiene primero una previsualizacion con `remote`, `host` y `sanitized_url`, muestra el host exacto al usuario y solo llama `fetch_repo` tras confirmacion explicita.
- El backend revalida que el host actual del remote coincida con `confirmed_host`, ejecuta `git fetch --prune <remote>` con `GIT_TERMINAL_PROMPT=0`, clasifica errores comunes y elimina userinfo de URLs antes de devolver mensajes.
- El fetch sigue sin tocar working tree ni indice; la escritura queda acotada al comportamiento normal de `git fetch` dentro de `.git` (refs/objetos).
- El `Git2Engine` principal permanece read-only/sin red. No se implemento el diseno anterior de callbacks `credentials`/`certificate_check` de libgit2; la excepcion de red queda aislada en el comando explicito de sistema `git`.

Verificacion local:

- `cargo test --manifest-path src-tauri\Cargo.toml fetch -- --test-threads=1` (4/4)
- `npm test -- src\bus\contract.test.ts src\workbench\operations.test.ts src\panels\RepoCard.test.tsx src\panels\DashboardPanel.test.tsx --run` (80/80)
- `npx tsc --noEmit`
