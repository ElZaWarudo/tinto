---
evidence_schema_version: 1
evidence_revision: 2
evidence_date: "2026-07-17"
status: "frozen"
purpose: "product-polish-regression"
source_commit: "8b7c4c85a3e561eb60f68b10678b59bb45919ccd"
atlas: "docs/product/application-atlas.md"
atlas_status: "stale"
atlas_fingerprint: "sha256:9022ba30a85237722b2976b4bc9757a2e4e2bb9c48afb056af188cee996120a0"
original_audit: "docs/audits/2026-07-17-product-polish-audit.md"
implementation_authorized: true
acceptance_criteria_count: 38
---

# Product polish regression evidence

Paquete común congelado para los doce pases independientes. Registra los 18 hallazgos de la auditoría original y los 20 criterios concretos añadidos por las primeras verificaciones, sin convertir evidencia automatizada o de navegador en una afirmación de ejecución nativa.

## 1. Provenance and freshness

- La implementación está en el working tree de `develop`, sin commit, sobre `8b7c4c85a3e561eb60f68b10678b59bb45919ccd`.
- El atlas conserva la última huella validada, `sha256:9022ba30a85237722b2976b4bc9757a2e4e2bb9c48afb056af188cee996120a0`, y se marcó `stale` porque las correcciones todavía no están representadas por `HEAD`.
- El preflight oficial cubrió 281 archivos y devolvió `status: stale`, `reason: relevant working-tree or index changes are not represented by HEAD`. La ejecución `--compute` devolvió la misma huella de baseline; no se usó para declarar validado el árbol modificado.
- La intención confirmada sigue siendo doble y co-principal: observar cambios del repositorio y dirigir sesiones Agent. Windows y Linux son las plataformas soportadas; WSL es un límite Linux dentro de Windows. macOS permanece fuera de alcance oficial.

## 2. Implementation and acceptance evidence

| Finding | Implemented correction | New evidence | Evidence type |
|---|---|---|---|
| `POL-05-01` | Copy, move y export usan staging vecino, backup temporal, intercambio y rollback; la ruta WSL/Linux delega en el mismo contrato transaccional. | Los tests interrumpen un archivo de 8192 B tras 4096 B y un directorio después de un archivo completo más 4096/8192 B del segundo. Origen y destino quedan byte a byte intactos y no quedan nombres `.tinto-*`; el reemplazo exitoso también pasa por el protocolo WSL. | `code`, `observed` en test |
| `POL-09-01` | La estrategia de drop se resuelve con `event.ctrlKey` en la entrega; la UI muestra copy/move y bloquea una mutación repetida. | Tests cubren Ctrl pulsado solo al soltar → copy, Ctrl liberado solo al soltar → move, preservación del origen y llamada única durante promesa pendiente. | `code`, `observed` en test |
| `POL-03-01` | Salud por canal, conservación del último estado, banner global, reintento de listeners y reintento automático de `agent-session-list`. | Tests fuerzan rechazo de listado/listener, comprueban que no se sustituye por vacío, muestran el canal afectado, reintentan a 1 s y cancelan el retry al recuperarse o desmontarse. | `code`, `observed` en test |
| `POL-02-01` | El ruler agrupa por línea exacta y distribuye objetivos de 24 px sin solapamiento. | En navegador, 16 objetivos midieron 24 px y cero solapamientos. Click directo verificó `requested === active` para 7, 13, 14, 57, 58 y 59; End dejó `aria-valuenow=80`. | `observed` |
| `POL-04-02` | La raíz sin Tauri abre un launchpad browser-only intencional y sin IPC nativo. | `/` fue útil a 1280 y 390 px, mostró cuatro enlaces de fixtures, no desbordó el documento y produjo cero logs `error`/`warn`. | `observed` |
| `POL-06-02` | Agent adopta `min-width: 0`, `border-box`, wrapping y reflow de contenedor. | Compact a contenedor 499 px y viewports 390/320, y navigator a 1280, tuvieron cero descendientes fuera del contenedor y cero overflow horizontal de documento. | `observed` |
| `POL-03-04` | Pending por objeto, bloqueo selectivo y feedback de estrategia durante mutaciones del árbol. | Con promesa diferida, los tests prueban una sola llamada, controles bloqueados y contexto preservado en éxito/error. | `code`, `observed` en test |
| `POL-05-02` | Undo/redo global fallido aparece en UI con acción de reintento. | Tests fuerzan el rechazo, comprueban mensaje persistente y verifican que el reintento vuelve a invocar la operación sin fingir éxito. | `code`, `observed` en test |
| `POL-05-03` | Borrado de archivo y journal exige `user_consent`/`userConsent` en frontend y backend; `false` se rechaza antes de estado, filesystem o SQLite. | Tests de contrato, cliente, file ops y Agent console cubren consentimiento negativo y positivo. | `code`, `observed` en test |
| `POL-08-01` | Timeline conserva el snapshot visible durante carga y error, y reemplaza resultados de forma atómica. | Tests con respuesta diferida y fallida conservan filas y estado “Actualizando” hasta el reemplazo exitoso. | `code`, `observed` en test |
| `POL-03-02` | Dashboard usa región viva `polite`, agrupada por lote/repositorio y con secuencia para volver a anunciar resúmenes idénticos. | Tests cubren deltas rápidos, ruido no material, limpieza de repos obsoletos y repetición textual de un resumen en eventos distintos. | `code`, `observed` en test |
| `POL-03-05` | El adaptador espera `send`; al rechazar elimina la deduplicación, pasa a unavailable y permite reintento. | Tests cubren `Promise.reject`, estado indisponible y reenvío después de recuperación. | `code`, `observed` en test |
| `POL-09-03` | Los atajos globales se suspenden mientras existe un modal. | Tests prueban que comandos de workspace no actúan detrás del diálogo y vuelven a funcionar al cerrarlo. | `code`, `observed` en test |
| `POL-09-04` | F10 y Alt llevan foco a Workbench; Escape restaura el foco anterior. | En navegador, F10 y Alt enfocaron Workbench y Escape devolvió el foco al campo de búsqueda; los recorridos internos continúan cubiertos por tests. | `observed` |
| `POL-09-05` | La ventana separada ofrece `Reanexar`, accesible por click/teclado, con pending, error y reintento; solo marca/cierra tras confirmación del evento. | Tests cubren respuesta `false`, rechazo, repetición bloqueada, reintento y orden evento → marca → cierre. | `code`, `observed` en test |
| `POL-07-02` | Acciones Agent se localizaron y explicitan objeto: “Detener turno”, “Revertir sesión”, “Restaurar desde este turno”, “Revertir archivo” y “Rápido”. | Tests de Terminal/Agent verifican las etiquetas y corrigen la contracción “del turno”. | `code`, `observed` en test |
| `POL-01-02` | README deja de prometer un terminal genérico y describe las consolas Agent realmente alcanzables. | Revisión del diff README ↔ registro de superficies del producto. | `code`, `declared` |
| `POL-12-05` | La fixture runtime usa geometría de composer compatible con textarea y botón reales. | A 1280 px el textarea midió 922 px y Enviar 88 px; a 390 px, 177 px y 50.7 px, sin overflow horizontal. | `observed` |

## 3. Acceptance criteria added during focused verification

Estos criterios forman parte del mismo alcance cerrado. La corrección se limita al cambio mínimo y al `verify` indicado; la ronda final no autoriza nuevas mejoras.

| Finding | Acceptance criterion | Verification evidence | Status |
|---|---|---|---|
| `POL-02-91` | La estrategia de drag debe seguir Ctrl aunque el puntero permanezca quieto y usar el modificador final al soltar. | Keydown/keyup actualizan indicador; pointerup decide copy/move; ProjectExplorer focal. | `resolved-code/test` |
| `POL-03-91` | El primer snapshot Dashboard establece baseline y no se anuncia como delta. | Prueba de montaje/lifecycle y deltas posteriores. | `resolved-code/test` |
| `POL-04-90` | Reanexar requiere request/ack con timeout; un fallo de cierre solo reintenta el cierre y nunca repite la transferencia. | Pruebas de ack, timeout, cierre fallido y close-only retry. | `resolved-code/test` |
| `POL-05-90` | Copy/move/export/delete/restore/redo son atómicos a nivel de lote; cleanup posterior es warning, rollback incompleto conserva token, y el manifest solo sirve en su repo canónico. | Fault injection local/WSL: rollback, warning, replay parcial idempotente, token retriable y rechazo cross-repo. | `resolved-code/test` |
| `POL-05-91` | Restore crea un safety checkpoint y, si falla, restaura worktree e índice Git exactos; symlinks no preservables fallan antes de mutar. | Tests staged, staged+unstaged, fallo intermedio/refresh, cleanup y ruta agent-side. | `resolved-code/test` |
| `POL-06-90` | A ≤520 px el composer coloca Adjuntar/textarea/Enviar en la primera fila y acciones activas en la segunda, sin overflow. | CSS/component tests y observación previa a 499/390/320. | `resolved-code/test` |
| `POL-06-91` | La barra del ruler representa coordenada proporcional real y mantiene targets de 24 px sin solapamiento. | Tests de geometría y selección exacta; observación previa de 16 targets. | `resolved-code/test` |
| `POL-06-92` | La ventana separada distingue drag handle de Reanexar y el error puede envolver sin recorte. | Tests focales de estructura, acción y estado. | `resolved-code/test` |
| `POL-07-91` | Reanudar journal es idempotente, reutiliza `session_id` conocido y limpia borrador solo tras write confirmado. | Tests de resume/write/open, retry y fallo de envío. | `resolved-code/test` |
| `POL-07-92` | El cierre de turno distingue finalización con y sin checkpoint en el estado principal y la franja Actividad. | `agentActivitySummary` reutiliza la etiqueta canónica; paridad con/sin checkpoint, TerminalPanel 93/93 y suite final 681/681. | `resolved-code/test` |
| `POL-07-93` | Errores visibles explican recuperación en español; configuración y habilidades usan nombres de tarea, dejando detalle técnico en logs. | Tests focales de copy y logging técnico. | `resolved-code/test` |
| `POL-08-90` | Tras resume+send se abre la sesión sin esperar `listAgentSessions`; el refresh queda en segundo plano. | Respuesta de lista diferida/fallida no bloquea apertura ni compositor. | `resolved-code/test` |
| `POL-08-91` | Un refresh de journals conserva entradas anteriores, marca stale/error y ofrece retry. | Tests con respuesta diferida, rechazo y recuperación. | `resolved-code/test` |
| `POL-08-92` | Timeline publica cada repo al resolverse sin esperar al más lento y mantiene pending agregado. | Test fast-before-slow con dos repos diferidos. | `resolved-code/test` |
| `POL-09-90` | F10 es inmediato; Alt solo entra al menubar en keyup si fue aislado. Alt+Tab/F4/Space no cambia foco ni se previene. | Tests de bare Alt y tres acordes de sistema. | `resolved-code/test` |
| `POL-09-91` | Selección múltiple separa activo/foco/selección; Ctrl-toggle, Shift-rango de hermanos y Ctrl+Space alimentan una sola operación batch. | Tests de `aria-selected`, `aria-multiselectable`, tres hermanos, copy/cut/delete/drag. | `resolved-code/test` |
| `POL-10-90` | En compact el orden Tab sigue la geometría: Adjuntar → Mensaje → Enviar → Encolar → Intervenir. | Test de orden DOM/teclado; CSS conserva las dos filas. | `resolved-code/test` |
| `POL-11-91` | Texto y adjuntos sobreviven al remount del mismo `sessionId` durante el proceso; éxito limpia, fallo conserva y otra sesión no hereda. | Registro en memoria acotado a 20 y pruebas de remount/éxito/fallo/aislamiento. | `resolved-code/test` |
| `POL-12-90` | Enviar en la fixture runtime nunca queda silencioso: vacío deshabilita y submit muestra que fue simulado sin Agent real. | Tests por click/teclado, feedback accesible y limpieza. | `resolved-code/test` |
| `POL-12-91` | La sesión reanudada abre una vez como provisional `starting/waiting`; demora no bloquea, fallo conserva y ofrece retry, confirmación reemplaza y limpia estado. | TerminalPanel cubre apertura única, demora usable, fallo/ausencia, retry y reemplazo confirmado; 93/93 focales. | `resolved-code/test` |

## 4. Browser regression pass

La verificación se ejecutó en el navegador integrado sobre el servidor Vite local y se cerró al terminar.

- Root launchpad: 1280 y 390 px, cuatro fixtures alcanzables, sin overflow ni excepciones.
- Dashboard: 1280 y 390 px, `scrollWidth === clientWidth`.
- Menú: F10 y Alt enfocaron Workbench; Escape restauró el foco de búsqueda.
- Agent: compact 499/390/320 y navigator 1280 sin overflow de descendientes; runtime composer correcto a 1280/390.
- Ruler: 16 objetivos de 24 px, sin intersecciones; selección exacta 7/13/14/57/58/59 y End=80.
- Consola del navegador: `[]` para logs de error/advertencia durante el recorrido.

La automatización por locator no pudo accionar los marcadores densos y el capturador de screenshot no estuvo disponible. La comprobación de hit-testing se completó mediante interacción directa sobre el DOM y lectura posterior del estado activo; no se adjunta imagen.

## 5. Automated verification

- Vitest completo, árbol final: 52 archivos, 681 pruebas aprobadas; el pase anterior al formateo mecánico arrojó el mismo 52/681 y tres archivos afectados por ese formato se repitieron 33/33.
- TypeScript: `tsc --noEmit`, correcto.
- ESLint: repositorio completo, correcto.
- Prettier 3.8.4 del lockfile: repositorio completo, correcto.
- Contrato Rust ↔ TypeScript: generado y `contract:check`, correcto.
- Build web con Vite 7.3.5 del lockfile: 449 módulos transformados, correcto.
- Rust completo: 348/348 pruebas aprobadas; file operations transaccionales 17/17, runtime WSL 25/25 y protocolo WSL 6/6 en pases focales.
- Rust: `cargo check --lib` y `cargo fmt --all -- --check`, correctos.
- Árbol: `git diff --check`, correcto; no quedan manifests pnpm ni artefactos `.tinto-*`/temporales versionables.

## 6. Restrictions and remaining probes

- No se ejecutó una build Tauri de este working tree en Windows ni Linux; el comportamiento nativo sigue inferido de código y tests donde no existe fixture web equivalente.
- No había una distribución WSL desechable. La ruta Linux interna y el protocolo se probaron en Rust, pero no se provocó un fallo real de disco/permisos dentro de WSL.
- No se ejecutó un Agent real ni se ejercieron filesystem, SQLite, diálogos del SO, drag-and-drop nativo o ventanas Tauri separadas con datos de usuario.
- No se recorrió con NVDA/Orca, high contrast, zoom 200/400 %, touch o reduced motion en el SO.
- No se perfilaron repositorios, diffs, Timeline o transcripts grandes con carga real.

Estas restricciones impiden usar el pase como certificación nativa completa, pero no invalidan los resultados observados en navegador ni las pruebas automatizadas acotadas.

## 7. Final council result

Los doce pases usaron este mismo paquete congelado, trabajaron en solo lectura y se limitaron a los criterios definidos. `POL-07-92` se reabrió una vez porque la franja Actividad conservaba una etiqueta antigua; se corrigió con la etiqueta canónica, la suite completa volvió a 681/681 y el Evaluador 07 cerró la reverificación con `findings: []`.

| # | Dimension | Rating | Confidence | Findings |
|---:|---|---:|---|---:|
| 01 | Alcance y foco | 2 | medium | 0 |
| 02 | Coherencia de comportamiento | 2 | medium | 0 |
| 03 | Estado y feedback | 2 | medium | 0 |
| 04 | Estados no ideales | 2 | medium | 0 |
| 05 | Protección del usuario | 2 | medium | 0 |
| 06 | Jerarquía de interfaz | 2 | medium | 0 |
| 07 | Contenido y lenguaje | 3 | medium | 0 |
| 08 | Rendimiento percibido | 3 | medium | 0 |
| 09 | Convenciones de plataforma | 2 | medium | 0 |
| 10 | Accesibilidad incorporada | 2 | medium | 0 |
| 11 | Continuidad de contexto | 2 | medium | 0 |
| 12 | Finalización y costuras | 2 | medium | 0 |

Resultado del alcance cerrado: **38/38 criterios satisfechos; 0 hallazgos abiertos en el consejo final**. Las calificaciones 2 no representan criterios fallidos: reflejan el atlas formalmente `stale` y las sondas nativas, WSL, Agent real y tecnologías de asistencia todavía no ejecutadas.
