---
audit_schema_version: 1
audit_date: "2026-07-17"
status: "complete"
source_commit: "8b7c4c85a3e561eb60f68b10678b59bb45919ccd"
atlas: "docs/product/application-atlas.md"
atlas_fingerprint: "sha256:9022ba30a85237722b2976b4bc9757a2e4e2bb9c48afb056af188cee996120a0"
evidence: "docs/audits/2026-07-17-product-polish-evidence.md"
implementation_authorized: false
remediation_status: "criteria-satisfied"
remediation_implementation_authorized: true
remediation_evidence: "docs/audits/2026-07-17-product-polish-regression-evidence.md"
remediation_worktree_base: "8b7c4c85a3e561eb60f68b10678b59bb45919ccd"
---

# Product Polish Audit

## Verdict

Tinto muestra una base funcional sólida en los caminos normales observados —Dashboard responsive, filtros reversibles, estados de Agent legibles y navegación de teclado básica—, pero todavía no alcanza una madurez de producto confiable en sus bordes de mayor consecuencia. El eslabón más débil es la protección del trabajo: una sobrescritura elimina el destino antes de terminar el reemplazo, lo que se clasifica como `P0` por riesgo de pérdida de datos aunque la evidencia sea de código y falte inyección de fallos en runtime. La confianza factual también se rompe cuando el ruler lleva a una línea distinta y cuando fallos de sincronización pueden quedar silenciosos. El perfil mínimo es `1 — frágil` en cinco dimensiones; no se calcula una media global.

## Scope and freshness

- Atlas: [`docs/product/application-atlas.md`](../product/application-atlas.md), estado `validated`, huella `sha256:9022ba30a85237722b2976b4bc9757a2e4e2bb9c48afb056af188cee996120a0`.
- Frescura: el preflight cubrió 281 archivos y devolvió `fresh`; la huella coincide con `develop@8b7c4c85a3e561eb60f68b10678b59bb45919ccd` y no hay cambios relevantes fuera de los artefactos excluidos.
- Evidencia común: [`docs/audits/2026-07-17-product-polish-evidence.md`](./2026-07-17-product-polish-evidence.md), congelada antes de los doce pases independientes.
- Intención validada por producto: observar cambios mediante Dashboard/Live Diff/Timeline y dirigir sesiones mediante Agents/Agent Lens son acciones co-principales.
- Plataformas soportadas: Windows y Linux; WSL es un límite de ejecución en Windows. macOS no está soportado oficialmente y no se evalúa como plataforma objetivo.
- Cobertura: el atlas cartografía 31 flujos, 4 actores/roles, 33 superficies productivas y la superficie de fixtures `SURF-900`. Se observaron la raíz Vite, Dashboard y estados responsive, Agent Lens/completed/working/journal/compact/navigator/runtime, y el ruler; el resto se contrastó con código, pruebas y documentación.
- Comprobaciones: TypeScript y ESLint sin errores; Vitest aprobó 49 archivos y 638 pruebas.
- Huecos materiales: no se ejecutó el binario HEAD en Windows/Linux, no había distribución WSL, y no se ejercieron Agent reales, red, diálogos del SO, mutaciones/destrucción, restauraciones ni datos de perfil reales.

## Quality profile

| # | Dimension | Rating | Confidence | Strongest evidence | Main gap |
|---:|---|:---:|:---:|---|---|
| 1 | Alcance y foco | 2 | medium | Las dos intenciones están explícitas y Dashboard/Agent tienen entradas orientadas a tarea | Onboarding y descubribilidad nativos no recorridos |
| 2 | Coherencia de comportamiento | 2 | medium | Filtro/Restablecer, Escape y estados Agent son predecibles; `OBS-016` demuestra la excepción del ruler | Comparación Windows/Linux y mutaciones reales |
| 3 | Estado y feedback | 2 | medium | Carga de Dashboard y estados de Agent son claros; el código confirma silencios de sincronización y mutación | IPC, notificaciones y operaciones lentas no ejecutadas |
| 4 | Estados no ideales | 1 | medium | Fallo browser-only, marcadores densos y contenedores compactos fueron reproducidos | Errores nativos, límites de archivo y fallos de Agent |
| 5 | Protección del usuario | 1 | medium | El orden destructivo de overwrite y los límites de consentimiento se verificaron en código | Falta fault injection local/WSL y recuperación real |
| 6 | Jerarquía de interfaz | 1 | medium | Dashboard conserva jerarquía; transcript y composer de fixture rompen la geometría | Dock, tipografía y extremos nativos no observados |
| 7 | Contenido y lenguaje | 2 | medium | Los estados españoles son específicos; error interno, mezcla de idiomas y promesa de terminal son verificables | Inventario completo y copy destructivo en runtime |
| 8 | Rendimiento percibido | 2 | medium | Dashboard conserva el frame; Timeline descarta historial antes de reemplazarlo | Sin tiempos ni volumen real en Windows/Linux/WSL |
| 9 | Convenciones de plataforma | 1 | medium | Chrome y menús tienen patrones desktop; drag, modalidad, F10 y reattach dejan costuras | Sin ejecución nativa, DPI, ventanas ni DnD del SO |
| 10 | Accesibilidad incorporada | 2 | medium | Foco, teclado, slider y diálogos tienen una base semántica; hit targets y reflow fallan | Sin NVDA/Orca, contraste, zoom ni reduced motion real |
| 11 | Continuidad de contexto | 2 | medium | Búsqueda, Details, journal y carga preservan contexto; el ruler rompe la correspondencia | Sin recorrido real repo→diff→timeline ni Agent→checkpoint |
| 12 | Finalización y costuras | 1 | medium | Varias regresiones geométricas y de QA conviven con 638 tests verdes | Sin smoke nativo integral ni cobertura de hit-testing/layout |

## Weakest links

1. **Mutaciones que no son transaccionales ni expresan completamente la intención.** `POL-05-01` puede destruir el destino durante una sobrescritura fallida; `POL-09-01` mueve cuando la convención prometida indica copiar. Es el riesgo más grave por irreversibilidad y alcance Windows/Linux/WSL.
2. **La interfaz factual puede mostrar o seleccionar algo distinto de la realidad.** `POL-03-01` permite canales Agent/delta fallidos sin estado degradado y `POL-02-01` navega a otra línea cuando los marcadores se solapan. Ambos dañan la promesa central de supervisión confiable.
3. **Los contratos de entorno y contenedor están incompletos.** La raíz browser-only falla (`POL-04-02`), Agent desborda compact/navigator (`POL-06-02`) y varias convenciones desktop dependen de puntero o actúan detrás de modales (`POL-09-03` a `POL-09-05`).

## Prioritized findings

### Now

- **[P0] `POL-05-01` — La sobrescritura puede perder el destino original.** Dimensión primaria: Protección del usuario. Confianza `medium`; frecuencia `occasional`; esfuerzo `M`; afecta `FLOW-019`, `SURF-014/015/022`, `PLAT-001/002/003`.
  - Evidencia: `src-tauri/src/file_ops/commands.rs:670-696` elimina un destino existente antes de `copy_recursive`; `:699-732` repite el patrón al mover. Un fallo posterior deja el original eliminado y puede dejar un parcial.
  - Efecto: pérdida o corrupción de trabajo después de una confirmación que el usuario interpreta como reemplazo controlado.
  - Corrección: copiar primero a staging vecino, validar, conservar backup temporal y hacer intercambio atómico; ante fallo restaurar el original y retirar el parcial.
  - Verificación: inyectar fallos a mitad de archivo/directorio, local y WSL; origen y destino original deben quedar byte a byte intactos. Probar también reemplazo exitoso.

- **[P1] `POL-09-01` — Ctrl+arrastrar mueve aunque el contrato indica copiar.** Dimensión primaria: Convenciones de plataforma. Confianza `medium`; frecuencia `occasional`; esfuerzo `S`; afecta `FLOW-019`, `SURF-014/015`, `PLAT-001/002`.
  - Evidencia: `ProjectExplorer.tsx:566-592,696-724` documenta Ctrl=copia, pero no transmite el modificador y usa siempre `strategy: "move"`.
  - Efecto: el origen cambia de ubicación contra una convención asentada y contra el propio comentario del producto.
  - Corrección: resolver `copy|move` desde el modificador de la entrega y mostrar la estrategia en cursor/indicador.
  - Verificación: Ctrl+drag conserva el origen; drag sin Ctrl mueve; ambos funcionan con y sin conflicto de overwrite.

- **[P1] `POL-03-01` — La pérdida de sincronización puede parecer un estado vacío o vigente.** Dimensión primaria: Estado y feedback. Confianza `medium`; frecuencia `unknown`; esfuerzo `M`; afecta `FLOW-001/009/021/022`, `SURF-005/025/026`, `PLAT-001/002`.
  - Evidencia: `src/bus/connection.ts:73-79` descarta errores de `listAgentSessions`; `:102-117` no maneja rechazo de registros de listeners; `OBS-001` reprodujo rechazos `transformCallback`.
  - Efecto: repositorios o Agents pueden parecer sincronizados cuando un canal dejó de actualizarse.
  - Corrección: modelar salud separada de repos y Agents, capturar rechazos, conservar último estado conocido y ofrecer degradación/reintento visibles.
  - Verificación: forzar rechazo de listado y listeners; no sustituir datos por vacío, anunciar el canal afectado y recuperar tras reconexión.

- **[P1] `POL-02-01` — El ruler activa una línea distinta de la elegida.** Dimensión primaria: Coherencia de comportamiento; cruza feedback, jerarquía, accesibilidad, continuidad y costuras. Confianza `medium`; frecuencia `occasional`; esfuerzo `M`; afecta `FLOW-013`, `SURF-018`, `PLAT-005` y debe regresarse en `PLAT-001/002`.
  - Evidencia: `OBS-016` observó 13→14 y 57→59; los objetivos 57/58/59 miden 18 px con apenas ~4 px entre posiciones. `OBS-015` confirma que End por teclado sí conserva la línea exacta.
  - Efecto: en Live Diff el usuario puede inspeccionar otra señal o posible secreto y pierde confianza en la navegación factual.
  - Corrección: eliminar hitboxes solapados; agrupar señales densas o distribuir objetivos inequívocos con una identidad única para etiqueta, foco, scroll y `activeLine`.
  - Verificación: puntero y teclado sobre 7, 13, 14, 57, 58 y 59 deben enfocar, anunciar y activar exactamente la misma línea sin solapamiento.

### Next

- **[P2] `POL-04-02` — La raíz browser-only ofrece una recuperación imposible.** Primaria: Estados no ideales; `frequent`, esfuerzo `M`, `FLOW-001`, `SURF-002/900`, `PLAT-005`.
  - Evidencia/efecto: `OBS-001` muestra `invoke` interno, listeners repetidos y “Reintentar” sin bridge; contradice `CODE-002` y `DECL-001` y bloquea la entrada oficial de QA.
  - Corrección/verificación: detectar ausencia de Tauri antes de bootstrap y abrir un modo degradado o launchpad de fixtures; `/` debe quedar útil, localizado y sin excepciones de consola.

- **[P2] `POL-06-02` — Agent desborda compact y navigator.** Primaria: Jerarquía de interfaz; `frequent`, esfuerzo `S–M`, `FLOW-021/022/027/030`, `SURF-025/026/028/029`.
  - Evidencia/efecto: `OBS-012/013` mide artículos y controles 21–38 px fuera del contenedor/viewport; conversación y acciones quedan recortadas en una intención co-principal.
  - Corrección/verificación: contrato container-responsive (`min-width:0`, `border-box`, wrapping y reflow); a 499/390/320 px y navigator 1280 ningún elemento debe rebasar su contenedor ni exigir scroll horizontal.

- **[P2] `POL-03-04` — Las mutaciones de archivo no muestran pendiente ni bloquean repeticiones.** Primaria: Estado y feedback; `occasional`, esfuerzo `M`, `FLOW-019/020`.
  - Evidencia/efecto: `ProjectExplorer.tsx:542-664` espera backend sin estado pending; una operación lenta deja el árbol accionable y permite gestos incompatibles.
  - Corrección/verificación: estado pending por objeto, anuncio y bloqueo selectivo; con promesa diferida solo debe emitirse una llamada y el éxito/error debe conservar contexto.

- **[P2] `POL-05-02` — El fallo de undo global solo llega a consola.** Primaria: Protección del usuario; `rare`, esfuerzo `S`, `FLOW-020`.
  - Evidencia/efecto: `src/qol/shortcuts.ts:170-175` hace `console.warn`; el usuario puede creer que Ctrl+Z restauró el archivo aunque la operación siga recuperable.
  - Corrección/verificación: feedback persistente con reintento; forzar un fallo y confirmar que el backup/operación permanece recuperable y el segundo intento actualiza el árbol.

- **[P2] `POL-05-03` — Los comandos backend de borrado no exigen consentimiento.** Primaria: Protección del usuario; `rare`, esfuerzo `S`, `FLOW-020/028`.
  - Evidencia/efecto: las UI confirman, pero `bus/client.ts:143-144,301-302` y los comandos file/journal no codifican ese límite; un caller interno puede saltarlo.
  - Corrección/verificación: parámetro backend obligatorio y rechazo antes de filesystem/SQLite; `false` u omitido no debe producir cambios y `true` solo sale del flujo confirmado.

- **[P2] `POL-08-01` — Timeline retira historial visible durante cada actualización.** Primaria: Rendimiento percibido; `frequent`, esfuerzo `S`, `FLOW-015`, `SURF-023`.
  - Evidencia/efecto: `TimelinePanel.tsx:115-146,294-395` filtra primero y pide después; el contenido desaparece y el layout se contrae.
  - Corrección/verificación: conservar snapshot anterior con “Actualizando…” y reemplazar atómicamente; una respuesta demorada o fallida no debe vaciar la lista.

- **[P2] `POL-03-02` — Los deltas principales de Dashboard no se anuncian.** Primaria: Estado y feedback; `frequent`, esfuerzo `M`, `FLOW-009/011`, `SURF-005/011`.
  - Evidencia/efecto: `DashboardPanel.tsx:208-235` y `RepoCard.tsx:220-357` actualizan señales fuera de región viva; quien no mira puede perder la señal principal.
  - Corrección/verificación: una región `polite` deduplicada por lote/repositorio; varios deltas rápidos deben producir una síntesis, no ruido por contador.

- **[P2] `POL-03-05` — El rechazo asíncrono de notificaciones queda como éxito.** Primaria: Estado y feedback; `occasional`, esfuerzo `S`, `FLOW-031`, `SURF-032`.
  - Evidencia/efecto: `notifications.ts:149-164` registra la clave antes de resolver `send` y un `try/catch` no captura `Promise.reject`; no se reintenta ni se muestra indisponibilidad.
  - Corrección/verificación: esperar la promesa, retirar deduplicación al fallar y pasar a unavailable; un adapter rechazado debe poder reintentarse al recuperarse.

- **[P2] `POL-09-03` — Atajos globales actúan detrás de un modal.** Primaria: Convenciones de plataforma; `occasional`, esfuerzo `S`, `FLOW-004/005/019/030`.
  - Evidencia/efecto: `shortcuts.ts:151-265` solo excluye inputs y el diálogo intercepta Escape/Tab; Ctrl+W/B/Tab/Z puede cambiar el workspace oculto.
  - Corrección/verificación: suspender atajos globales mientras haya modal; cada diálogo debe aislarlos y restaurarlos al cerrar.

- **[P2] `POL-09-04` — La barra de menús no ofrece entrada Alt/F10.** Primaria: Convenciones de plataforma; `frequent`, esfuerzo `S`, `FLOW-003/030`, `SURF-004`.
  - Evidencia/efecto: `MenuBar.tsx:162-244,295-320` cubre navegación una vez enfocada, pero no Alt/F10; el usuario de teclado debe tabular todo el chrome.
  - Corrección/verificación: F10/Alt enfocan Workbench, flechas recorren menús y Escape vuelve al foco previo.

- **[P2] `POL-09-05` — Reanexar Agents depende de un drag con puntero.** Primaria: Convenciones de plataforma; `occasional`, esfuerzo `S`, `FLOW-029`, `SURF-031`.
  - Evidencia/efecto: `DetachedConsolesApp.tsx:48-68` solo usa `onPointerDown`; Enter/Espacio/clic normal no ofrecen una recuperación explícita.
  - Corrección/verificación: acción “Reanexar” por clic, teclado y menú, preservando drag; todos los caminos deben reintegrar Agents y cerrar la ventana separada.

- **[P2] `POL-07-02` — Agent mezcla idiomas y no nombra el alcance de acciones sensibles.** Primaria: Contenido y lenguaje; `frequent`, esfuerzo `M`, `FLOW-021/022/023/025/026/027`.
  - Evidencia/efecto: `OBS-007–011` mezcla Details/Stop/Revert/Next con estados en español; “Revert” no aclara si afecta turno, sesión o archivos.
  - Corrección/verificación: vocabulario localizado y objeto explícito (“Detener turno”, “Revertir sesión”, “Restaurar desde este turno”) en completed/working/details/journal.

- **[P2] `POL-01-02` — README promete un terminal genérico sin entrada productiva.** Primaria: Alcance y foco; `occasional`, esfuerzo `S`, `SURF-033`.
  - Evidencia/efecto: `README.md:49-50,110-114,162-164` lo presenta como enviado; `App.tsx:52-58` registra superficies de repo/Timeline/Agent, no un terminal genérico alcanzable.
  - Corrección/verificación: retirar/calificar la promesa o documentar la ruta real; cada capacidad pública debe tener una entrada reproducible en Windows/Linux.

### Later

- **[P3] `POL-12-05` — La fixture runtime usa una cuadrícula incompatible con su composer.** Primaria: Finalización y costuras; `frequent` dentro de la fixture, esfuerzo `S`, `SURF-900`.
  - Evidencia/efecto: `OBS-014` midió textarea de 38 px y botón de 1010 px; `agentRuntime.tsx:91-95` monta dos controles sobre CSS productivo de cinco columnas. Contamina la revisión visual, pero no prueba que el composer productivo esté roto.
  - Corrección/verificación: estructura completa o grid de dos columnas; a 1280/768/390 el textarea conserva el espacio principal y el diálogo de presets sigue operativo.

## Flow lifecycle matrix

| Flow | Before | During | After | Failure | Real conditions |
|---|---|---|---|---|---|
| `FLOW-001` Bootstrap | Config y perfil disponibles | Carga snapshot y listeners | Workspace o first-run | Hoy browser expone excepción/reintento inútil | Browser observado; nativo no verificado |
| `FLOW-009/010/013` Observar → abrir → Diff | Dashboard con repos | Filtro, deltas, archivo y ruler | Señal/línea entendida | Snapshot degradado y selección densa deben conservar verdad | Dashboard/ruler observados; Git real no |
| `FLOW-015` Timeline | Historial visible | Actualiza al cambiar HEAD | Lista reemplazada | Debe conservar último resultado y explicar error | Código; no causalidad Git real |
| `FLOW-019/020` Mutar/undo archivo | Selección y confirmación | Copia/move/delete/backup | Árbol refrescado o undo | Requiere rollback, pending y reintento visible | Código; ninguna mutación ejecutada |
| `FLOW-021/022/023` Lanzar/dirigir Agent | Runtime y sesión listos | Composer, working, queue/steer | Turno completo y checkpoint | Lanzamiento/IPC/settling deben ser recuperables | Estados fixture; Agent real no |
| `FLOW-025/026` Stop/revert/restore | Sesión/turno elegidos | Confirmación y consent backend | Estado terminal/restaurado coherente | Nunca éxito parcial; contexto preservado | Controles/código; acción no ejecutada |
| `FLOW-027/028` Journal | Conversación guardada | Reabrir o confirmar borrado | Composer reanudado o entrada eliminada | Sesión activa protegida y consentimiento explícito | Reapertura observada; borrado no |
| `FLOW-029` Detach/reattach | Agent acoplado | Nueva ventana/drag | Sesión reintegrada y foco recuperado | Alternativa de teclado y recuperación de ventana | Solo código |
| `FLOW-030` Menús/atajos/zoom | Foco/contexto conocidos | Comando visible o teclado | Vista cambia y foco vuelve | Modales aíslan atajos; Alt/F10 alcanzan menú | Escape observado; resto código |

## Keep

- Dashboard no desborda a 1280, 768 ni 390 px; conserva el frame y anuncia `Cargando repos` (`OBS-002/005/006`).
- Filtro y `Restablecer` forman un ciclo claro y reversible (`OBS-003`).
- Escape cierra Workbench y restaura el foco (`OBS-004`); preservar este contrato al añadir Alt/F10.
- Agent distingue completed/working/journal, ajusta Stop/Revert/composer y mantiene búsqueda, posición, Details, tabs y checkpoint 2/2 (`OBS-007–011`).
- El ruler ya tiene semántica de slider, instrucciones de teclado y navegación End exacta (`OBS-015`); corregir colisiones sin degradar esta base.
- Mantener confirmación y consentimiento backend en fetch y restore/revert Agent (`CODE-008`), así como backup/undo de delete (`CODE-009`).
- Diff ya contempla binarios, tamaño, líneas largas, cap de render y ausencia de hunks (`CODE-004`).
- Conservar TypeScript, ESLint y las 638 pruebas como baseline, ampliándolas con geometría, hit-testing y fallos intermedios.

## Verification gaps

- Ejecutar una build HEAD limpia con perfiles y repositorios desechables en Windows y Linux: first-run → Dashboard → repo → Diff → Timeline → Agents → cierre.
- Instalar/proveer una distro WSL desechable y repetir registro, archivos, Agent, overwrite/rollback y normalización de rutas.
- Inyectar fallos de disco, permisos y copia parcial en overwrite, move, delete/undo, restore y revert; comprobar atomicidad y ausencia de éxito parcial.
- Ejecutar un Agent sintético real desde launch hasta checkpoint, incluyendo queue/steer, stop, journal, reinicio, detach/reattach y runtime ausente.
- Provocar pérdida y recuperación independiente de snapshot, listeners repo y listeners Agent; medir conservación de último estado y anuncios.
- Probar red/fetch/Gitleaks con hosts controlados, cancelación, checksum y permisos; no usar datos reales del usuario.
- Recorrer con NVDA en Windows y Orca en Linux; medir contraste/high-contrast, zoom 200/400 %, foco visible, touch y reduced motion.
- Perfilar arranque y deltas con repositorios, Timeline, diffs y transcripts grandes; registrar latencia, frame time, CPU, memoria y layout shift.
- Verificar diálogos del SO, drag-and-drop real, DPI y ventanas Tauri separadas. macOS permanece fuera de alcance.

## Remediation slices

| Slice | Findings | Expected outcome | Effort | Acceptance checks |
|---|---|---|:---:|---|
| A. Mutaciones transaccionales | `POL-05-01`, `POL-09-01`, `POL-03-04`, `POL-05-02`, `POL-05-03` | Ninguna operación pierde trabajo, confunde copy/move o oculta recuperación | L | Fault injection local/WSL; Ctrl+drag; pending; undo visible; consent negativo |
| B. Verdad de sincronización | `POL-03-01`, `POL-03-02`, `POL-03-05` | Cada canal revela salud y cambios sin ruido ni falsos éxitos | M | Rechazos/listeners; snapshot preservado; live-region deduplicada; Promise.reject |
| C. Navegación exacta del Diff | `POL-02-01` | Marcador, foco, scroll y línea activa comparten identidad | M | Matriz 7/13/14/57/58/59 con puntero, teclado y árbol a11y |
| D. Contenedores Agent | `POL-06-02`, `POL-12-05` | Transcript y QA fixtures se adaptan sin recorte | M | 1280/800/640/499/390/320 y zoom 200/400 % |
| E. Entrada browser-only | `POL-04-02` | Vite abre una superficie de QA intencional sin errores IPC | M | `/` útil, fixtures descubribles, consola limpia, reintento solo si es viable |
| F. Continuidad Timeline | `POL-08-01` | Actualizar no desmonta el historial ni contrae el layout | S | Respuesta diferida, error y reemplazo atómico en uno/varios repos |
| G. Contratos desktop | `POL-09-03`, `POL-09-04`, `POL-09-05` | Modalidad, menús y ventanas funcionan con teclado y puntero | M | Modal aísla atajos; Alt/F10; reattach Enter/Espacio/clic/drag |
| H. Promesa y vocabulario | `POL-07-02`, `POL-01-02` | El idioma y la documentación describen exactamente objeto y alcance | S–M | Inventario de copy; acciones sensibles; paridad README↔navegación |

## Handoff

Auditoría completa. No se autorizó implementación y no se modificó código de producto. La secuencia recomendada es A → B/C → D/E/F/G → H, comenzando por `POL-05-01` y su fault injection antes de cualquier trabajo cosmético. Tras cada slice: actualizar el atlas si cambia una superficie/flujo, repetir el preflight de frescura, ejecutar los criterios `verify`, reabrir las dimensiones primarias y cruzadas, y clasificar cada hallazgo como `resolved`, `partially-resolved`, `not-reproduced` u `open` con evidencia nueva.

## Remediation addendum — 2026-07-17

El responsable autorizó después la implementación. El working tree sobre `8b7c4c85a3e561eb60f68b10678b59bb45919ccd` satisface los 18 hallazgos originales y los 20 criterios concretos añadidos durante la verificación focal: **38/38 criterios satisfechos**. El consejo final terminó con `findings: []` en las doce dimensiones; `POL-07-92` se reabrió una vez, se corrigió dentro de su criterio original y volvió a verificarse sin hallazgos.

Evidencia de cierre: [product-polish-regression-evidence.md](2026-07-17-product-polish-regression-evidence.md). Validación global: 52 archivos/681 pruebas frontend, 348/348 pruebas Rust, TypeScript, ESLint, Prettier, contrato generado, build Vite y `git diff --check`, todos correctos.

El atlas factual está actualizado en el working tree pero conserva `status: stale`: el preflight no puede declararlo `fresh` hasta que estas correcciones estén representadas por un commit limpio y se calcule una nueva huella. También siguen sin ejecutarse las sondas nativas Windows/Linux, WSL real, Agent real y tecnologías de asistencia enumeradas como huecos de verificación; no se clasifican como criterios fallidos de esta remediación.
