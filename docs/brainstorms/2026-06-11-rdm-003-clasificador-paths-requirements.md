---
date: 2026-06-11
topic: rdm-003-clasificador-paths
---

# RDM-003 — Clasificador de paths (tres buckets): requisitos

## Summary

Implementar la función pura que clasifica cada path/evento de FS de un repo en los buckets del diseño §3: descartar (interno de `.git/`), señal de metadata git (`HEAD`/`index`), Plano 1 (trackeable), Plano 2 (gitignoreado pero vigilado por `fs_watch`) o descartar (gitignoreado sin vigilar). Es el filtro que enruta los eventos del watcher (RDM-004) y evita barrer `node_modules`/`target`.

## Key Decisions

- **Crate `ignore` para las reglas de .gitignore** (design §5): matchers construidos por repo, aptos para el hot path del watcher (muchos eventos/segundo) sin abrir el repo git por evento.
- **`globset` para la watchlist `fs_watch`**: los patrones opt-in del Plano 2 (`.env`, `dist/**`, `*.log`) son globs compilados una vez por repo.
- **Clasificador puro y sin I/O por evento.** Se construye con (repo_root, watchlist) leyendo los .gitignore una vez; `classify(path)` no toca disco. Re-construible cuando cambien los .gitignore (lo decide el watcher/bus).
- **`.git/HEAD` y `.git/index` son señal, no descarte** (design §7): se clasifican como evento de metadata git para que el bus detecte commits/cambios de branch.
- **Limitación v1 documentada:** archivos tracked-pero-gitignoreados (raros; requieren `git add -f`) se clasifican por las reglas de ignore (Plano 2/descartar) aunque git los trackee. El recálculo de status (RDM-006) sigue siendo la fuente de verdad del Plano 1.

## Requirements

- R1. Existe un tipo `Classification` con los buckets: `GitInternal` (descartar), `GitMeta` (HEAD/index), `Plane1`, `Plane2`, `Ignored` (descartar).
- R2. Un path bajo `.git/` clasifica `GitInternal`, salvo `.git/HEAD` y `.git/index` que clasifican `GitMeta`.
- R3. Un path no ignorado por las reglas de .gitignore del repo clasifica `Plane1` (tracked o trackeable).
- R4. Un path ignorado que matchea algún patrón de la watchlist clasifica `Plane2`.
- R5. Un path ignorado sin match de watchlist clasifica `Ignored`.
- R6. Con watchlist vacía, ningún path ignorado clasifica `Plane2` (opt-in estricto, design §3).
- R7. El clasificador respeta .gitignore anidados (subdirectorios) y patrones de directorio (`target/`).
- R8. `classify(path, is_dir)` acepta paths absolutos dentro del repo o relativos a su raíz, y no hace ningún I/O: el flag `is_dir` lo aporta el caller (el watcher lo conoce del evento; `false` para paths borrados).
- R9. Paths fuera del repo_root devuelven un error tipado o bucket explícito `OutsideRepo` (no panic).

## Acceptance Examples

- AE1. **Covers R2.** En un repo con `.gitignore` = `target/`: `.git/objects/ab/cdef` → `GitInternal`; `.git/HEAD` → `GitMeta`; `.git/index` → `GitMeta`.
- AE2. **Covers R3.** `src/main.rs` (no ignorado) → `Plane1`, exista o no en el índice.
- AE3. **Covers R4, R6.** Con `.gitignore` = `*.log` y watchlist `["*.log"]`: `app.log` → `Plane2`. Con watchlist vacía: `app.log` → `Ignored`.
- AE4. **Covers R7.** Con `.gitignore` raíz = `target/` y `sub/.gitignore` = `local.txt`: `target/debug/x` → `Ignored`; `sub/local.txt` → `Ignored`; `sub/otro.txt` → `Plane1`.

## Scope Boundaries

- Sin watcher (RDM-004): el clasificador es consumido por él, no lo contiene.
- Sin estado git (índice/status): la pertenencia real al working set la decide RDM-002/006.
- Sin persistencia de watchlist (RDM-005 la provee); aquí llega como `Vec<String>`.

## Dependencies / Assumptions

- RDM-001/002 mergeados (módulos backend establecidos).
- Crates `ignore` y `globset` compilan en Windows/Linux sin features especiales.

## Outstanding Questions

- **Deferred to Planning:** forma del builder (¿un `PathClassifier::new(root, watchlist)` que falle solo si root no existe?) y semántica exacta de rebuild.
