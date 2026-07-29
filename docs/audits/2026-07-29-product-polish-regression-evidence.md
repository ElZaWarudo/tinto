---
evidence_date: "2026-07-29"
source_state: "dirty working tree"
parent_audit: "docs/audits/2026-07-28-product-polish-audit.md"
findings_implemented:
  - "POL-10-001"
  - "POL-07-001"
  - "POL-10-002"
native_runtime_status: "blocked before application start by Pumarejo APP_START_FAILED"
---

# Evidencia de regresión del pulido de Tinto

## Resultado

Los tres hallazgos aceptados se implementaron y pasaron regresión automatizada. La confirmación nativa mediante Pumarejo no se pudo completar: la versión actual expuso correctamente sus 12 herramientas y aceptó una copia QA con manifiesto v2, capability mínima y ejecutable configurado, pero `tauri_launch` volvió a `idle` con `APP_START_FAILED` antes de crear el proceso de Tinto. No se atribuye ese resultado a la interfaz ni se usa como evidencia visual de producto.

## `POL-10-001` — pestañas Dockview

- Se añadió un adaptador compartido para las pestañas por defecto y personalizadas.
- El contenedor expone `tablist`; cada cabecera expone `tab`, nombre, `aria-selected`, foco itinerante y `aria-controls`.
- El contenido activo expone `tabpanel` y actualiza `aria-labelledby` al cambiar de pestaña.
- Clic, Enter, Espacio, flechas, Inicio y Fin activan mediante la misma API de panel.
- Las acciones de cierre se excluyen de la activación.
- La cobertura comprende pestañas principales, repositorios, archivos y conversaciones de Agent.
- Prueba focal: `src/workspace/AccessibleDockTab.test.tsx`.

## `POL-07-001` — verdad temporal

- Las entradas se agrupan por día local con encabezado visible y semántico.
- Cada hora usa `<time datetime="<ISO completo>">` y un nombre accesible con fecha y hora completas.
- Se conserva el orden global, la selección del commit, la apertura del detalle y el retorno de foco.
- La prueba focal usa dos días locales y valida encabezados y `datetime`.

## `POL-10-002` — acciones contextuales

- Fetch anuncia `Actualizar referencias remotas de <repo>`.
- El selector anuncia `Tipo de Agent para <repo>`.
- El lanzamiento anuncia `Iniciar <provider> en <repo>` y refleja el estado de inicio.
- El nombre de presentación se propaga también al lanzador del panel de proyecto.

## Verificación automatizada

| Comprobación | Resultado |
|---|---|
| TypeScript `tsc --noEmit` | exit 0 |
| ESLint sobre superficies modificadas y pruebas | exit 0 |
| Vitest completo | 55 archivos, 718 pruebas, todas pasan |
| Build frontend de producción | 452 módulos transformados, exit 0 |
| Regresión focal tras asociar `tab`/`tabpanel` | 12 archivos, 174 pruebas, todas pasan |

Vitest y Vite se ejecutaron con `--configLoader runner` porque el cargador por defecto de esbuild intentó leer un directorio padre denegado por el sandbox antes de cargar `vite.config.ts`.

## Intento nativo con Pumarejo actualizado

1. El MCP por `stdio` expuso `tauri_launch`, `tauri_status`, `tauri_snapshot`, `tauri_screenshot`, las herramientas de interacción/ventana y `tauri_close`.
2. Para preservar los cambios locales de Tinto se usó una copia QA de los mismos fuentes, con integración Pumarejo v2 independiente.
3. `doctor` confirmó config, manifiesto, registro debug/feature, capability, alineación de versiones, Node, Cargo y ejecutable `pnpm`.
4. La heurística de WebView continuó informando `not_detected`, aunque el host había ejecutado Tinto con WebView2 en la auditoría anterior.
5. `tauri_launch` devolvió `APP_START_FAILED`; `tauri_status` quedó en `idle`, `lastAction: launch`, sin stderr ni proceso `tinto`.
6. `tauri_close` confirmó cierre idempotente en `idle`.

Por tanto, la regresión nativa de clic/teclado, 800×600 y 1920×1032 sigue pendiente. El bloqueo pertenece al arnés o al entorno de lanzamiento hasta que Pumarejo aporte la causa de `APP_START_FAILED`; no invalida las pruebas de Tinto, pero impide elevarlas a observación nativa.
