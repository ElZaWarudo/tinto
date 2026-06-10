---
title: "feat: capa de git read-only — trait GitEngine + impl git2-rs (RDM-002)"
type: feat
date: 2026-06-11
origin: docs/brainstorms/2026-06-11-rdm-002-git-engine-requirements.md
---

# feat: capa de git read-only — trait GitEngine + impl git2-rs (RDM-002)

## Summary

Añadir al backend Rust el módulo `git`: tipos de dominio serializables, trait `GitEngine` (sync, read-only) y `Git2Engine` con git2-rs, con tests contra repos git temporales reales. Sin tocar el frontend ni el bus de eventos.

## Requirements Trace

- R1–R3, AE1–AE2 (status/branch/último commit) → U1, U2
- R4–R6, AE3 (log, blobs, diffs) → U1, U3
- R7–R8, AE4 (errores tipados, binarios) → U1–U3 transversal
- R9–R10 (impl completa + tests con fixtures reales) → U2, U3
- R11 (escape hatch documentado) → U1
- Origin: `docs/brainstorms/2026-06-11-rdm-002-git-engine-requirements.md`

## Key Technical Decisions

- **Módulo `git` con tipos primero.** `src-tauri/src/git/mod.rs` define tipos de dominio (`RepoStatus`, `BranchInfo`, `CommitInfo`, `FileDiff`, `DiffHunk`, `DiffLine`, `GitError`) todos `Serialize` (serde) para que RDM-006 los emita al frontend sin re-mapeo (assumption del origin).
- **Trait sync con `&self` y `Send + Sync`.** git2 es sync; los métodos toman `&self` y el trait exige `Send + Sync` para poder compartirlo desde tareas async (RDM-006 lo envolverá con `spawn_blocking`). Sin async-trait aquí.
- **git2 con `vendored-libgit2`.** Compila libgit2 estáticamente vía cc/cmake del toolchain MSVC ya presente; evita depender de una libgit2 del sistema en Windows y Linux. Es la opción robusta para app de escritorio.
- **Paginación del log por offset+limit** (resuelve Outstanding Question): `log(offset: usize, limit: usize)` sobre revwalk desde HEAD; suficiente para el timeline inicial en memoria (RDM-010); un cursor por oid puede añadirse después sin romper el trait.
- **Errores con `thiserror`.** `GitError` enum (`NotARepository`, `RepositoryNotFound`, `Internal(git2::Error)`, etc.); ningún panic en paths esperables (R7).
- **Escape hatch solo documentado** (R11): rustdoc del trait anota el criterio pendiente (medir status/diff en monorepos grandes; si p95 excede presupuesto del live-diff, introducir `GitCliEngine`).

## Output Structure

```text
src-tauri/src/
├── lib.rs           # añade `pub mod git;`
└── git/
    ├── mod.rs       # tipos de dominio + GitError + trait GitEngine (rustdoc con escape hatch)
    ├── git2_engine.rs  # Git2Engine: impl del trait con git2-rs
    └── test_fixtures.rs # helpers de tests: crear repos temporales poblados (cfg(test))
```

## Implementation Units

### U1. Tipos de dominio, GitError y trait GitEngine

- **Goal:** Contrato completo de la capa de git: tipos serializables + trait documentado.
- **Requirements:** R1–R8 (formas), R11.
- **Dependencies:** None.
- **Files:** `src-tauri/src/git/mod.rs`, `src-tauri/src/lib.rs` (declaración del módulo), `src-tauri/Cargo.toml` (deps `git2` vendored, `thiserror`).
- **Approach:** Definir structs/enums serde-serializables y el trait. Nota rustdoc en los tipos: los paths son relativos al repo y se serializan con separador del SO (backslash en Windows); la normalización a forward-slash para el frontend, si hace falta, la decide RDM-006 al congelar el contrato de eventos. Métodos:
  - `status(&self) -> Result<RepoStatus, GitError>` (modified/staged/untracked como `Vec<PathBuf>` relativos al repo)
  - `branch_info(&self) -> Result<BranchInfo, GitError>` (nombre o detached, ahead/behind opcionales)
  - `head_commit(&self) -> Result<CommitInfo, GitError>`
  - `log(&self, offset: usize, limit: usize) -> Result<Vec<CommitInfo>, GitError>`
  - `blob_at(&self, commit_id: &str, path: &Path) -> Result<Vec<u8>, GitError>`
  - `worktree_diff(&self) -> Result<Vec<FileDiff>, GitError>` y `commit_diff(&self, commit_id: &str) -> Result<Vec<FileDiff>, GitError>`
  - `FileDiff { path, old_path (renames), is_binary, hunks }`; `DiffHunk { old_start, new_start, lines }`; `DiffLine { kind: Added|Removed|Context, content, old_lineno, new_lineno }`.
- **Test scenarios:** Test expectation: none — unit de contrato puro (tipos+trait); el comportamiento se prueba en U2/U3 vía Git2Engine.
- **Verification:** `cargo check` y `cargo clippy -- -D warnings` en verde; rustdoc del trait menciona el escape hatch.

### U2. Git2Engine: status, branch, head y log

- **Goal:** Implementación git2-rs de la mitad "estado e historial" del trait.
- **Requirements:** R1–R4, R7; AE1, AE2, AE4.
- **Dependencies:** U1.
- **Files:** `src-tauri/src/git/git2_engine.rs`, `src-tauri/src/git/test_fixtures.rs`, `src-tauri/src/git/mod.rs` (re-export).
- **Approach:** `Git2Engine::open(path) -> Result<Self, GitError>` mapeando errores de apertura a `NotARepository`/`RepositoryNotFound`. `status()` con `git2::StatusOptions` (include_untracked, exclude ignored). `branch_info()` con `head()` + `graph_ahead_behind` contra upstream si existe. `log()` con revwalk + skip/take. Fixtures: helper `cfg(test)` que crea repo temporal (`tempfile`), configura user, y permite commits/modificaciones/stage programáticos con git2.
- **Patterns to follow:** estilo de errores y módulos establecido en U1; tests en `#[cfg(test)] mod tests` como en `lib.rs` de RDM-001.
- **Test scenarios:**
  - Covers AE1. status() en repo con un modificado + un staged + un untracked → tres listas de un elemento.
  - Covers AE2. branch_info() con upstream local 2 commits adelante → behind=2, ahead=0. Fixture: segunda branch local como upstream vía `Branch::set_upstream` (sin remote real); si set_upstream rechaza branches locales, configurar `branch.<name>.remote/merge` directo en config.
  - Covers AE4. `Git2Engine::open` sobre dir no-git → `Err(NotARepository)`, sin panic.
  - Happy: log(0, 2) en repo con 3 commits devuelve los 2 más recientes en orden; log(2, 2) devuelve el restante.
  - Edge: repo recién `git init` sin commits → status ok, branch_info reporta unborn/HEAD sin commits sin panic, head_commit → error tipado.
- **Verification:** `cargo test git::` en verde; clippy limpio.

### U3. Git2Engine: diffs y blobs

- **Goal:** Material crudo de diffs (working tree y por commit) y lectura de blobs.
- **Requirements:** R5, R6, R8; AE3.
- **Dependencies:** U1, U2 (fixtures).
- **Files:** `src-tauri/src/git/git2_engine.rs`, `src-tauri/src/git/test_fixtures.rs`.
- **Approach:** `worktree_diff()` con `diff_index_to_workdir` + `diff_tree_to_index` combinados (o `diff_tree_to_workdir_with_index` contra HEAD tree) produciendo `FileDiff` por archivo vía callbacks de hunk/línea; `commit_diff()` con `diff_tree_to_tree(parent, commit)` (primer padre; commit raíz → tree vacío). `blob_at()` resolviendo commit → tree → entry → blob. Binarios: flag `is_binary` del delta, sin hunks (R8).
- **Test scenarios:**
  - Covers AE3. Modificar 2 líneas de archivo trackeado → un FileDiff con hunk; líneas removed/added con old/new lineno correctos.
  - Happy: commit_diff de un commit que añade un archivo → FileDiff con todas las líneas Added.
  - Happy: blob_at del archivo en el commit anterior devuelve el contenido viejo.
  - Edge: archivo binario (bytes no-UTF8 con NUL) modificado → is_binary=true, hunks vacíos.
  - Edge: archivo untracked presente en el repo NO aparece en worktree_diff (el diff es de archivos trackeados; los untracked viven en status).
  - Error: blob_at con commit id inexistente → error tipado, sin panic.
- **Verification:** `cargo test git::` completo en verde; `cargo clippy -- -D warnings`; suite frontend intacta (`npm test`) — no debería tocarse nada del frontend.

## Scope Boundaries

Igual que el origin: sin escrituras git, sin watcher/clasificador, sin wrapping async/eventos (RDM-006), sin render de diffs (RDM-008), sin impl CLI del escape hatch, sin baseline de perf.

### Deferred to Follow-Up Work

- `GitCliEngine` (escape hatch) si el baseline futuro lo justifica.
- Cursor por oid en `log` si el timeline lo necesita.

## Risks & Dependencies

- **Compilación de libgit2 vendored en Windows:** libgit2-sys vendored compila con el crate `cc` usando el cl.exe/SDK ya presentes (no requiere cmake). Mitigación igualmente: `cargo check` inmediato tras añadir la dep en U1 para fallar rápido si el toolchain sorprende.
- **Discriminación de errores de apertura:** verificar en U2 (test) que `Repository::open` sobre (a) path inexistente y (b) dir no-git produce kinds distinguibles en git2; si no lo son, colapsar `RepositoryNotFound`/`NotARepository` en un solo variante y ajustar AE4 — el trait no debe prometer una distinción que el backend no da.
- **Unborn HEAD:** git2 `head()` devuelve `Err` (Code::UnbornBranch), no panic; mapear a variante propia y cubrirlo en el edge test de U2.
- **Semántica staged vs modified en git2 status:** un archivo puede estar en ambas listas (staged con cambios posteriores); los tests fijan la semántica esperada (status bits INDEX_* → staged; WT_* → modified).
- **Repos unborn/detached:** cubiertos como edge cases en U2 para no romper el dashboard con repos recién creados.

## Verification Strategy

`cargo test` (módulo git completo) → `cargo clippy -- -D warnings` + `cargo fmt --check` → `npm test` + `npm run lint` (sin regresión frontend) → `npm run tauri build` solo si algo del shell cambió (no esperado; gap registrable si se omite).
