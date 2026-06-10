---
title: "feat: clasificador de paths en tres buckets (RDM-003)"
type: feat
date: 2026-06-11
origin: docs/brainstorms/2026-06-11-rdm-003-clasificador-paths-requirements.md
---

# feat: clasificador de paths en tres buckets (RDM-003)

## Summary

Módulo `paths` en el backend: `PathClassifier` construido con (repo_root, watchlist) que clasifica paths en `GitInternal | GitMeta | Plane1 | Plane2 | Ignored | OutsideRepo` usando `ignore::gitignore::Gitignore` y `globset`, sin I/O por evento. Lógica pura con tests de tabla.

## Requirements Trace

- R1–R2, AE1 (buckets + .git) → U1
- R3–R9, AE2–AE4 (reglas ignore + watchlist + edges) → U1, U2
- Origin: `docs/brainstorms/2026-06-11-rdm-003-clasificador-paths-requirements.md`

## Key Technical Decisions

- **`ignore::gitignore::GitignoreBuilder` por repo** con los `.gitignore` del árbol añadidos al construir (raíz y anidados, recorridos una vez con `ignore::WalkBuilder` o añadidos lazy). Resolución elegida: construir con el builder añadiendo cada `.gitignore` encontrado en un walk inicial — el rebuild ante cambios de .gitignore lo dispara el consumidor (RDM-004/006).
- **`globset::GlobSet` para watchlist**, matcheando sobre el path relativo con separadores normalizados a `/` (los patrones del usuario usan `/`).
- **Builder `PathClassifier::new(repo_root, &[String]) -> Result<Self, ClassifierError>`**: falla solo si root no existe o un patrón de watchlist es inválido (error tipado con el patrón culpable); .gitignore ilegibles se omiten con tolerancia (mismo comportamiento que git).
- **`classify(&self, path: &Path, is_dir: bool) -> Classification`** infalible y **sin I/O alguno**: el flag `is_dir` lo pasa el caller (el watcher lo conoce del evento de notify; `false` para borrados). Paths fuera del root → `Classification::OutsideRepo` (bucket explícito, resuelve R9 sin Result en el hot path).
- **GitignoreBuilder con scoping por archivo:** `GitignoreBuilder::new(root)` + `add(<path al .gitignore>)` por cada `.gitignore` del walk — el crate escopa las reglas de cada archivo a su directorio contenedor, así un solo `Gitignore` compilado respeta el anidamiento; `matched_path_or_any_parents(rel, is_dir)` evalúa el path y sus ancestros (patrones dir-only tipo `target/`). AE4 actúa como spike que verifica este comportamiento del crate antes de dar U2 por bueno.
- **Detección de `.git` por componentes:** `Path::components()` comparando `Component::Normal(".git")` (cross-platform, sin string-matching del path crudo con backslashes).
- **Sin estado git** (limitación tracked-pero-ignored documentada en rustdoc, decisión del origin).

## Implementation Units

### U1. Tipos, builder y reglas .git

- **Goal:** `Classification`, `ClassifierError`, `PathClassifier::new` y clasificación de `.git/*`.
- **Requirements:** R1, R2, R8, R9; AE1.
- **Files:** `src-tauri/src/paths/mod.rs`, `src-tauri/src/lib.rs` (mod decl), `src-tauri/Cargo.toml` (deps `ignore`, `globset`).
- **Approach:** normalizar el path entrante (absoluto → relativo al root; rechazar fuera del root con `OutsideRepo`); detectar componente inicial `.git` (`GitMeta` para exactamente `.git/HEAD` y `.git/index`, `GitInternal` para el resto).
- **Test scenarios:** tabla AE1 completa + path absoluto vs relativo equivalentes (R8) + path fuera del repo → OutsideRepo (R9) + el propio directorio `.git` → GitInternal.
- **Verification:** `cargo test paths::` (subset U1) + clippy limpio.

### U2. Reglas gitignore + watchlist

- **Goal:** Plano 1 / Plano 2 / Ignored según ignore y globset.
- **Requirements:** R3–R7; AE2–AE4.
- **Dependencies:** U1.
- **Files:** `src-tauri/src/paths/mod.rs` (mismo módulo; lógica + tests).
- **Approach:** sobre el matcher `Gitignore` compilado en `new()`, `matched_path_or_any_parents(rel_path, is_dir)` decide ignorado; no-ignorado → `Plane1`; ignorado → watchlist globset sobre rel-path normalizado a `/`: match → `Plane2`, sin match → `Ignored`. `is_dir` viene del caller (R8): cero I/O en classify.
- **Test scenarios:** tabla AE2/AE3/AE4; watchlist con patrón inválido → error del builder con patrón nombrado; archivo borrado (no existe en FS) ignorado por patrón de dir (`dist/**`) → Plane2 si watchlist lo cubre; `target/debug/x` con `target/` en gitignore → Ignored (patrón de directorio, R7).
- **Verification:** `cargo test paths::` completo; `cargo clippy -- -D warnings`; `npm test`/`lint` sin regresión.

## Scope Boundaries

Las del origin. Módulo puro; sin watcher, sin git state, sin persistencia.

## Risks & Dependencies

- **Fidelidad de `ignore` vs git real:** el crate de ripgrep es el estándar de facto pero no replica el 100% de quirks de git (p. ej. excludes globales del usuario). Aceptado para v1; el status real (RDM-002) es la fuente de verdad del Plano 1.
- **`is_dir` para paths borrados:** un dir borrado clasifica como archivo; los patrones de watchlist tipo `dist/**` siguen matcheando por glob. Cubierto en tests.

## Verification Strategy

`cargo test` → `cargo fmt --check` + `cargo clippy -- -D warnings` → `npm test` + `npm run lint`.
