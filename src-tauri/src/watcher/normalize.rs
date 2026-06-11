//! Normalización de eventos de notify a la taxonomía del canal
//! (`Created | Modified | Removed`), según la regla del plan:
//!
//! - Renames se descomponen en `Removed(origen)` + `Created(destino)`, cada
//!   mitad clasificable por su propio path (cubre saves atómicos de
//!   editores/agentes).
//! - Kinds desconocidos/`Any` se mapean conservadoramente a `Modified`.
//! - Eventos `Access` se descartan antes de clasificar.
//! - `is_dir` se deriva del `EventKind` cuando el backend lo tipa
//!   (Create/Remove File|Folder); en su ausencia se usa `false`, consistente
//!   con el contrato de `PathClassifier::classify` para borrados.

use std::path::PathBuf;

use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventKind};

/// Tipo de evento normalizado que viaja en los lotes del canal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Created,
    Modified,
    Removed,
}

/// Un path normalizado: a qué path le pasó qué, y si el backend supo
/// decirnos que era un directorio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedEvent {
    pub path: PathBuf,
    pub kind: EventType,
    pub is_dir: bool,
}

/// Normaliza un evento crudo de notify a cero o más eventos de la
/// taxonomía del canal. Función pura, sin I/O.
pub fn normalize(event: &Event) -> Vec<NormalizedEvent> {
    match &event.kind {
        // Lecturas: ruido para un monitor de cambios.
        EventKind::Access(_) => Vec::new(),

        EventKind::Create(kind) => {
            let is_dir = matches!(kind, CreateKind::Folder);
            map_paths(event, EventType::Created, is_dir)
        }

        EventKind::Remove(kind) => {
            let is_dir = matches!(kind, RemoveKind::Folder);
            map_paths(event, EventType::Removed, is_dir)
        }

        EventKind::Modify(ModifyKind::Name(mode)) => match mode {
            // Par completo en un solo evento: paths = [origen, destino].
            RenameMode::Both => {
                let mut out = Vec::with_capacity(2);
                if let Some(from) = event.paths.first() {
                    out.push(NormalizedEvent {
                        path: from.clone(),
                        kind: EventType::Removed,
                        is_dir: false,
                    });
                }
                if let Some(to) = event.paths.get(1) {
                    out.push(NormalizedEvent {
                        path: to.clone(),
                        kind: EventType::Created,
                        is_dir: false,
                    });
                }
                out
            }
            RenameMode::From => map_paths(event, EventType::Removed, false),
            RenameMode::To => map_paths(event, EventType::Created, false),
            // Rename sin dirección conocida: conservador.
            RenameMode::Any | RenameMode::Other => map_paths(event, EventType::Modified, false),
        },

        // Resto de modificaciones (Data/Metadata/Any/Other) y kinds
        // globales sin tipar: conservadoramente "modificado".
        EventKind::Modify(_) | EventKind::Any | EventKind::Other => {
            map_paths(event, EventType::Modified, false)
        }
    }
}

fn map_paths(event: &Event, kind: EventType, is_dir: bool) -> Vec<NormalizedEvent> {
    event
        .paths
        .iter()
        .map(|path| NormalizedEvent {
            path: path.clone(),
            kind,
            is_dir,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, DataChange, MetadataKind};

    fn event(kind: EventKind, paths: &[&str]) -> Event {
        let mut e = Event::new(kind);
        for p in paths {
            e = e.add_path(PathBuf::from(p));
        }
        e
    }

    #[test]
    fn create_y_remove_tipados_llevan_su_is_dir() {
        let archivo = normalize(&event(EventKind::Create(CreateKind::File), &["/r/a.txt"]));
        assert_eq!(
            archivo,
            vec![NormalizedEvent {
                path: "/r/a.txt".into(),
                kind: EventType::Created,
                is_dir: false,
            }]
        );

        let carpeta = normalize(&event(EventKind::Create(CreateKind::Folder), &["/r/dir"]));
        assert!(carpeta[0].is_dir);

        let borrado = normalize(&event(EventKind::Remove(RemoveKind::Folder), &["/r/dir"]));
        assert_eq!(borrado[0].kind, EventType::Removed);
        assert!(borrado[0].is_dir);
    }

    #[test]
    fn modify_data_mapea_a_modified_sin_dir() {
        let out = normalize(&event(
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            &["/r/a.txt"],
        ));
        assert_eq!(out[0].kind, EventType::Modified);
        assert!(!out[0].is_dir);
    }

    #[test]
    fn rename_both_se_descompone_en_removed_mas_created() {
        let out = normalize(&event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            &["/r/viejo.txt", "/r/nuevo.txt"],
        ));
        assert_eq!(
            out,
            vec![
                NormalizedEvent {
                    path: "/r/viejo.txt".into(),
                    kind: EventType::Removed,
                    is_dir: false,
                },
                NormalizedEvent {
                    path: "/r/nuevo.txt".into(),
                    kind: EventType::Created,
                    is_dir: false,
                },
            ]
        );
    }

    #[test]
    fn rename_from_y_to_por_separado() {
        let from = normalize(&event(
            EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            &["/r/viejo.txt"],
        ));
        assert_eq!(from[0].kind, EventType::Removed);

        let to = normalize(&event(
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            &["/r/nuevo.txt"],
        ));
        assert_eq!(to[0].kind, EventType::Created);
    }

    #[test]
    fn access_se_descarta() {
        let out = normalize(&event(EventKind::Access(AccessKind::Read), &["/r/a.txt"]));
        assert!(out.is_empty());
    }

    #[test]
    fn kinds_desconocidos_caen_a_modified() {
        for kind in [
            EventKind::Any,
            EventKind::Other,
            EventKind::Modify(ModifyKind::Any),
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::Permissions)),
            EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
        ] {
            let out = normalize(&event(kind, &["/r/x"]));
            assert_eq!(out.len(), 1, "kind {kind:?}");
            assert_eq!(out[0].kind, EventType::Modified, "kind {kind:?}");
            assert!(!out[0].is_dir);
        }
    }

    #[test]
    fn evento_multi_path_produce_una_entrada_por_path() {
        let out = normalize(&event(
            EventKind::Create(CreateKind::File),
            &["/r/a.txt", "/r/b.txt"],
        ));
        assert_eq!(out.len(), 2);
    }
}
