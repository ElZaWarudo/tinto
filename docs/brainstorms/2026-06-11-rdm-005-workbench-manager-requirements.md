---
date: 2026-06-11
topic: rdm-005-workbench-manager
---

# RDM-005 — Workbench manager + persistencia de config: requisitos

## Summary

Implementar el modelo de workbenches (conjuntos nombrados de repos con alias, orden y watchlist `fs_watch` por repo), su persistencia en TOML en el config dir del SO, y los comandos `invoke` para listar, crear, modificar, conmutar workbenches y autodetectar repos git bajo una carpeta raíz.

## Key Decisions

- **TOML como formato** (el ejemplo del diseño §8 es TOML); archivo único `workbenches.toml` en el config dir de la app vía el crate `dirs` (`%APPDATA%/tinto` en Windows, `~/.config/tinto` en Linux).
- **Sin base de datos** (diseño §8): el estado de monitoreo en vivo no se persiste; solo la configuración.
- **El workbench activo se persiste** (campo `active`) para reabrir la app donde quedó — consistente con "config persiste entre sesiones" (§2).
- **Autodetección en dos niveles:** el scan busca directorios que contengan `.git` (check liviano, sin abrir el repo); la validación real (¿abre como repo?) usa el `Git2Engine` existente solo sobre los candidatos al confirmarlos. Resuelve la pregunta diferida del roadmap.
- **Un repo puede estar en varios workbenches** (§2): el modelo no impone unicidad cross-workbench.
- **Comandos delgados:** los `#[tauri::command]` validan y delegan en el manager; la UI de onboarding/edición llega en RDM-007/009.

## Requirements

**Modelo y persistencia**

- R1. Modelo: lista ordenada de workbenches, cada uno con `name` único y lista ordenada de repos; cada repo con `path` absoluto, `alias` opcional y `fs_watch: Vec<String>` opt-in (vacía por defecto).
- R2. La config se persiste en `workbenches.toml` en el config dir del SO (crate `dirs`) y sobrevive reinicios; el directorio se crea si falta.
- R3. Si el archivo no existe, el manager arranca con config vacía sin error; si existe pero está corrupto, error tipado que preserva el archivo (no se sobrescribe silenciosamente).
- R4. Se persiste cuál workbench está activo; si el activo referenciado no existe, se degrada a ninguno-activo sin error fatal.
- R5. Escrituras atómicas: escribir a archivo temporal + rename, para no corromper la config ante un crash.

**Operaciones**

- R6. CRUD de workbenches: crear (nombre único), renombrar, eliminar, listar; y de repos dentro de un workbench: agregar (path puntual), quitar, editar alias y fs_watch, reordenar.
- R7. Conmutación de workbench activo, devolviendo el workbench completo para que el frontend lo cargue.
- R8. Autodetección: dado un path raíz, devuelve los repos git encontrados (dirs con `.git`), con profundidad limitada y sin descender dentro de repos encontrados (un repo no contiene a otro en el resultado) ni a directorios pesados conocidos.
- R9. Agregar un repo valida que el path sea un repo git abrible (vía la capa git existente) y rechaza duplicados dentro del mismo workbench.

**Exposición a frontend**

- R10. Comandos `invoke` para: listar workbenches, crear/renombrar/eliminar workbench, agregar/quitar/editar repo, conmutar activo y autodetectar bajo una raíz. Tipos serializables compartidos con el modelo.
- R11. Errores de comando llegan al frontend como mensajes tipados/serializables, no como strings opacos de panic.

## Acceptance Examples

- AE1. **Covers R2, R5.** Crear workbench "Trabajo" con un repo, reiniciar el manager (releer del disco) → la config reaparece idéntica.
- AE2. **Covers R3.** Con un `workbenches.toml` con TOML inválido, el manager reporta error tipado y el archivo queda intacto.
- AE3. **Covers R8.** En un árbol con `a/` (repo git), `a/vendored/b` (repo git anidado) y `c/` (no repo), la autodetección con raíz en el padre devuelve solo `a` y no desciende dentro de `a`.
- AE4. **Covers R9.** Agregar un path que no es repo git devuelve el error tipado de la capa git; agregar dos veces el mismo path al mismo workbench rechaza el duplicado.

## Scope Boundaries

- Sin UI (RDM-007 hace onboarding/selector; RDM-009 el editor de fs_watch).
- Sin watcher ni eventos (RDM-004/006 consumen esta config).
- Sin SQLite ni timeline persistente (§8 futuro).
- Sin migraciones de esquema de config (greenfield; versionado simple `version = 1` en el archivo para el futuro).

## Dependencies / Assumptions

- RDM-002 mergeado (validación de repos con Git2Engine) — ✔.
- Crates nuevos: `dirs`, `toml` (serde). Config dir escribible.

## Outstanding Questions

- **Deferred to Planning:** profundidad máxima de la autodetección (propuesta: 4 niveles) y lista de directorios excluidos del scan (node_modules, target, .git internos).
