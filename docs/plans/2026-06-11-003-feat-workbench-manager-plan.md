---
title: "feat: workbench manager con persistencia TOML y autodetección (RDM-005)"
type: feat
date: 2026-06-11
origin: docs/brainstorms/2026-06-11-rdm-005-workbench-manager-requirements.md
---

# feat: workbench manager con persistencia TOML y autodetección (RDM-005)

## Summary

Módulo `workbench` en el backend: modelo serde (workbenches/repos/fs_watch), store TOML con escritura atómica en el config dir (`dirs`), operaciones CRUD + conmutación + autodetección de repos git, y comandos Tauri delgados que lo exponen al frontend. Tests con config dir temporal inyectado.

## Requirements Trace

- R1–R5, AE1–AE2 (modelo + persistencia atómica) → U1
- R6, R8–R9, AE3–AE4 (CRUD + autodetección + validación git) → U2
- R7 (conmutación devolviendo el workbench completo) → U2 (lógica) + U3 (comando)
- R10–R11 (comandos invoke + errores serializables) → U3
- Origin: `docs/brainstorms/2026-06-11-rdm-005-workbench-manager-requirements.md`

## Key Technical Decisions

- **Config dir inyectable:** `WorkbenchStore::new(config_dir)` recibe el directorio (en producción `dirs::config_dir().join("tinto")`; en tests un tempdir). Evita mocks y deja AE1/AE2 como tests reales de disco.
- **Estructura TOML versionada:** `version = 1`, `active = "Trabajo"`, `[[workbench]]` con `name` y `[[workbench.repos]]` (`path`, `alias?`, `fs_watch = []`) — calcada del ejemplo del diseño §8.
- **Escritura atómica:** serializar a `workbenches.toml.tmp` + `fs::rename` (mismo volumen) — R5. En Windows `std::fs::rename` reemplaza el destino existente (usa `MoveFileExW` con `MOVEFILE_REPLACE_EXISTING`); el test AE1 escribe dos veces (archivo ya existente) y lo verifica de facto, con el tempdir Windows aportando backslashes reales al round-trip TOML.
- **Estado en memoria + persistencia explícita:** el manager mantiene la config deserializada; cada mutación persiste antes de devolver Ok (sin auto-save diferido — simplicidad v1).
- **Autodetección con walk acotado:** BFS con profundidad máx. 4, sin descender dentro de repos encontrados, saltando nombres pesados conocidos (`node_modules`, `target`, `.git`, `dist`, `build`, `vendor`) y dirs ocultos salvo el propio `.git` como marcador. Candidato = dir que contiene entrada `.git` (dir o archivo, para worktrees).
- **Validación al agregar (R9):** `Git2Engine::open(path)` del módulo git existente; la conversión `From<GitError>` produce un `WorkbenchError` con kind+mensaje — GitError no necesita implementar Serialize, solo el error del workbench.
- **Errores serializables (R11):** `WorkbenchError` con `thiserror` + `Serialize` manual (mensaje + kind) para que los comandos devuelvan `Result<T, WorkbenchError>` y Tauri lo serialice al frontend.
- **Comandos con estado gestionado:** `tauri::State<Mutex<WorkbenchStore>>` registrado en `lib.rs`; los comandos son wrappers de una línea.

## Output Structure

```text
src-tauri/src/
├── lib.rs                  # añade mod workbench + .manage(store) + handlers
└── workbench/
    ├── mod.rs              # modelo, WorkbenchError, WorkbenchStore (CRUD+persistencia)
    ├── autodetect.rs       # scan de repos git bajo una raíz
    └── commands.rs         # #[tauri::command] delgados
```

## Implementation Units

### U1. Modelo + store TOML con escritura atómica

- **Goal:** Config tipada que persiste y se recarga fiel.
- **Requirements:** R1–R5; AE1, AE2.
- **Files:** `src-tauri/src/workbench/mod.rs`, `src-tauri/src/lib.rs` (mod decl), `src-tauri/Cargo.toml` (`dirs`, `toml`).
- **Approach:** structs serde (`WorkbenchConfig { version, active, workbenches }`, `Workbench { name, repos }`, `RepoEntry { path, alias, fs_watch }`); `WorkbenchStore { config_dir, config }` con `load()` (ausente→default; corrupto→`WorkbenchError::CorruptConfig` sin tocar el archivo) y `persist()` atómico (tmp+rename).
- **Test scenarios:** AE1 (round-trip con store re-creado), AE2 (TOML inválido → error tipado + archivo intacto byte a byte), archivo ausente → default vacío, active inexistente → degrada a None (R4).
- **Verification:** `cargo test workbench::` subset.

### U2. Operaciones CRUD + autodetección

- **Goal:** Mutaciones completas y scan de repos.
- **Requirements:** R6–R9; AE3, AE4.
- **Dependencies:** U1.
- **Files:** `src-tauri/src/workbench/mod.rs` (CRUD), `src-tauri/src/workbench/autodetect.rs`.
- **Approach:** métodos del store (crear/renombrar/eliminar workbench con unicidad de nombre; add/remove/edit/reorder repos con rechazo de duplicados por path; set_active devolviendo el workbench). `autodetect(root, max_depth=4)`: BFS acotado per KTD; devuelve `Vec<PathBuf>`. `add_repo` valida con `Git2Engine::open` (gated por flag `validate: bool` para no exigir repos reales en todos los tests de CRUD).
- **Test scenarios:** AE3 (árbol con repo anidado y no-repo), AE4 (no-repo rechazado con kind de la capa git; duplicado rechazado), renombrar a nombre existente → error, eliminar workbench activo → active queda None, reorden persiste.
- **Verification:** `cargo test workbench::` completo.

### U3. Comandos Tauri

- **Goal:** Superficie invoke para el frontend.
- **Requirements:** R10, R11.
- **Dependencies:** U1, U2.
- **Files:** `src-tauri/src/workbench/commands.rs`, `src-tauri/src/lib.rs` (manage + generate_handler).
- **Approach:** comandos `list_workbenches`, `create_workbench`, `rename_workbench`, `delete_workbench`, `add_repo`, `remove_repo`, `update_repo`, `set_active_workbench`, `autodetect_repos` — wrappers sobre `State<Mutex<WorkbenchStore>>`; `WorkbenchError: Serialize` (kind + message). El smoke `ping`/`tick` de RDM-001 se conserva.
- **Test scenarios:** Test expectation: none — wrappers de una línea sin lógica; la lógica está testeada en U1/U2 y el wiring se verifica con `cargo check` + arranque dev (los comandos se prueban end-to-end cuando exista UI, RDM-007).
- **Verification:** `cargo check`/`clippy`; `npm run tauri dev` arranca (humo manual breve si se desea); suite completa verde.

## Scope Boundaries

Las del origin (sin UI, sin watcher/eventos, sin SQLite, sin migraciones).

## Risks & Dependencies

- **Mutex en comandos:** contención trivial a esta escala; `std::sync::Mutex` (no async) porque las operaciones son cortas y sin awaits internos.
- **Paths Windows en TOML:** se serializan como strings con backslashes; round-trip estable. La normalización para mostrar es de la UI.
- **`dirs::config_dir()` None (entornos raros):** error tipado al construir el store de producción; tests no lo sufren (dir inyectado).

## Verification Strategy

`cargo test` → `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings` → `npm test` + `npm run lint` → arranque dev de humo.
