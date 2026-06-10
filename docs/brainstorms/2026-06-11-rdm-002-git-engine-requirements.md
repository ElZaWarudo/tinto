---
date: 2026-06-11
topic: rdm-002-git-engine
---

# RDM-002 — Capa de Git: trait GitEngine + impl git2-rs: requisitos

## Summary

Crear la capa de lectura de git de Tinto: un trait `GitEngine` que abstrae todas las operaciones read-only del Plano 1 (status, branch, ahead/behind, log, blobs, material crudo de diffs) con una implementación basada en git2-rs, dejando documentado el escape hatch a shellear `git` si aparece lentitud en repos grandes.

## Key Decisions

- **Trait como frontera.** Todos los consumidores (bus RDM-006, timeline RDM-010, diff viewer RDM-008) dependen del trait, nunca de git2 directo. Permite cambiar de backend sin reescribirlos (design §5, matiz git2-rs).
- **API síncrona.** git2 es sync; el trait es sync. Los consumidores async lo envuelven (p. ej. `spawn_blocking`); ese wrapping pertenece a RDM-006, no a este item.
- **Solo lectura.** Ninguna operación de escritura git (commit, stage, branch, merge, revert) existe en el trait — no-goal explícito del producto (design §9).
- **Diffs como datos, no como render.** El trait expone hunks/líneas estructuradas del diff del working tree; el highlighting y los modos de vista son de RDM-008.
- **Fixtures reales en tests.** Tests unitarios contra repos git temporales creados durante el test (git2 para crearlos), sin mocks del filesystem.

## Requirements

**Estado del repo**

- R1. El trait expone el status del working tree: listas de archivos modificados, staged y untracked, suficiente para los conteos de las cards del dashboard (design §4).
- R2. El trait expone la branch actual (incluido estado detached HEAD) y ahead/behind respecto a su upstream remoto cuando existe.
- R3. El trait expone el último commit (id, mensaje, autor, timestamp) para la card del repo.

**Historial y contenido**

- R4. El trait expone el log de commits navegable con paginación (offset/límite o cursor), con id, mensaje, autor y timestamp por commit.
- R5. El trait expone la lectura de blobs: contenido de un archivo en un commit dado (para la vista de archivo completo y diffs de commits, RDM-008/010).
- R6. El trait expone el diff del working tree por archivo como datos estructurados (hunks con líneas added/removed/context y números de línea), y el diff de un commit contra su padre con la misma forma.

**Errores y robustez**

- R7. Todas las operaciones retornan `Result` con un tipo de error propio del trait (repo inexistente, no es repo git, repo corrupto, archivo binario en diff); ningún `panic!` en paths de error esperables.
- R8. Los diffs marcan archivos binarios como tales en lugar de producir hunks de texto.

**Implementación y verificación**

- R9. Existe `Git2Engine` que implementa el trait completo con git2-rs.
- R10. Cada operación del trait tiene al menos un test contra un repo git temporal real (creado y poblado por el propio test) cubriendo el happy path, y tests de error para repo inexistente/no-git.
- R11. El módulo documenta (rustdoc del trait) el escape hatch: el criterio de performance que justificaría una impl CLI queda anotado como pregunta abierta para medir, no se implementa ahora.

## Acceptance Examples

- AE1. **Covers R1.** En un repo temporal con un archivo modificado, uno staged y uno nuevo sin trackear, `status()` devuelve exactamente esas tres listas con un elemento cada una.
- AE2. **Covers R2.** En un repo temporal con branch `main` y un upstream simulado 2 commits adelante, el trait reporta branch `main`, behind=2, ahead=0.
- AE3. **Covers R6.** Tras modificar dos líneas de un archivo trackeado, el diff del working tree contiene un hunk con esas líneas marcadas removed/added y números de línea correctos.
- AE4. **Covers R7.** Pedir status de un path que no es repo git devuelve el error tipado correspondiente, sin panic.

## Scope Boundaries

- Sin escrituras git de ningún tipo (design §9).
- Sin watcher ni clasificador de paths (RDM-003/004); sin wrapping async ni emisión de eventos (RDM-006); sin render/highlighting de diffs (RDM-008); sin UI.
- Sin implementación CLI del escape hatch (solo documentado) y sin baseline de performance (pregunta de plan/futuro).

## Dependencies / Assumptions

- RDM-001 mergeado (base `develop` disponible) — cumplido.
- git2-rs compila en Windows con el toolchain MSVC ya instalado (vendored o libgit2 precompilado; el plan fija la feature).
- El contrato de eventos de RDM-006 consumirá estos tipos; los tipos del trait deben ser serializables (serde) para viajar al frontend sin re-mapeo innecesario.

## Outstanding Questions

- **Deferred to Planning:** forma exacta de la paginación del log (offset vs cursor por oid).
- **Deferred to Planning:** feature flags de git2 (vendored-libgit2 vs sistema) para build limpio en Windows/Linux.
- **Deferred to Future:** criterio numérico del escape hatch a CLI (requiere baseline con repos reales grandes).
