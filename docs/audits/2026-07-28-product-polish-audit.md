---
audit_schema_version: 1
audit_date: "2026-07-28"
source_commit: "9897dc023759dce1e3a29c5dc016f644dd60fa2f"
source_state: "dirty working tree"
atlas: "docs/product/application-atlas.md"
atlas_status: "stale"
evidence: "docs/audits/2026-07-28-product-polish-evidence.md"
accepted_findings: 3
open_p1: 2
open_p2: 1
---

# Product Polish Audit

## Verdict

Tinto presenta una superficie nativa Windows funcional, coherente en muchas transiciones y con buenas defensas de estado, pero **no puede declararse pulida ni auditada de forma exhaustiva**. El eslabón más débil es el renderer compartido de pestañas Dockview: las cabeceras principales no exponen semántica de pestaña y la activación directa de Resumen no cambió el panel, una costura sistémica que afecta navegación, accesibilidad y continuidad. La segunda ruptura está en la verdad temporal de Cronología: datos reales de varios días se muestran sólo con hora, lo que puede hacer parecer reciente actividad antigua y contradice el trabajo central de entender “cuándo”. El atlas sigue obsoleto frente a cambios sin commit, por lo que el veredicto es de madurez parcial y condicionada.

## Scope and freshness

- Atlas: `docs/product/application-atlas.md`, estado `stale`.
- Proveniencia validada del atlas: commit `8b7c4c85a3e561eb60f68b10678b59bb45919ccd`, fingerprint `sha256:9022ba30a85237722b2976b4bc9757a2e4e2bb9c48afb056af188cee996120a0`.
- Fuente observada: rama `develop`, commit `9897dc023759dce1e3a29c5dc016f644dd60fa2f`, con diez cambios de producto preexistentes sin commit.
- Fingerprint observado del working tree: `sha256:1ce1410a2103505c289041be8ea08de911e589f17848507ba9a77dfd8a309eb9`.
- Runtime: aplicación Tauri nativa visible en Windows, lanzada y controlada mediante el MCP real de Pumarejo por `stdio`; 800×600 y 1920×1032.
- Flujos nativos muestreados: `FLOW-001`, `FLOW-009..013`, `FLOW-015`, entrada de `FLOW-021`, `FLOW-030` parcial y `FLOW-032`.
- Rol de producto observado: `ROLE-001`; procesos de `ROLE-002` no iniciados. `ROLE-005` operó únicamente como driver de auditoría.
- Plataforma observada: `PLAT-001` mediante `PLAT-007`. Linux, WSL, instalador y bundle de release no se ejecutaron.
- Comprobaciones: 54 archivos/714 pruebas Vitest, ESLint, build frontend de producción y `cargo check` con feature `pumarejo`, todos con exit 0.
- Evidencia común: `docs/audits/2026-07-28-product-polish-evidence.md`.
- Feedback separado del arnés: `docs/audits/2026-07-28-pumarejo-usage-feedback.md`.

La cobertura reducida fue suficiente para producir hallazgos sobre las superficies observadas, no para aceptar la puerta de cobertura global. Los límites de snapshot grande y teclado de Pumarejo se trataron como huecos del instrumento, no como defectos de Tinto.

## Quality profile

| # | Dimension | Rating | Confidence | Strongest evidence | Main gap |
|---|---|---:|---|---|---|
| 01 | Alcance y foco | 2 | medium | Dashboard, repo, diff, Timeline y Agents sostienen las dos acciones coprimarias | No Agent/turn real |
| 02 | Coherencia de comportamiento | 1 | medium | Pestaña Resumen enfocada sin activar; menú alternativo sí funciona | Teclado y Linux |
| 03 | Estado y feedback | 2 | medium | Live status, fallback de Gitleaks, vacío filtrado y foco modal | Fallo/reconexión nativos |
| 04 | Estados no ideales | 2 | medium | Sin coincidencias recuperable y reflow 800/1920 | Primer uso, límites y fallos runtime |
| 05 | Protección del usuario | 2 | medium | Consentimiento, transacciones, rollback y checkpoints en Code/Tests | Consecuencias sensibles no ejecutadas |
| 06 | Jerarquía de interfaz | 2 | medium | Dashboard y diff mantienen composición legible | 640×480 y alto volumen |
| 07 | Contenido y lenguaje | 2 | medium | Vacíos, estados y modos de diff son concretos | Estados de error no observados |
| 08 | Rendimiento percibido | 2 | medium | Retención e incrementalidad en Code/Tests; layouts estables observados | Sin mediciones de latencia |
| 09 | Convenciones de plataforma | 2 | medium | Menús, diálogo, árbol y alternativa explícita de reanexado | Linux y ventanas separadas |
| 10 | Accesibilidad incorporada | 1 | medium | Snapshots semánticos muestran tabs sin rol/nombre y controles repetidos | Sin NVDA ni recorrido fiable de teclado |
| 11 | Continuidad de contexto | 2 | medium | Retención de estado, drafts y protocolos de reanudación en Code/Tests | Retorno detalle/lista no ejercitado |
| 12 | Finalización y costuras | 1 | medium | Costura Dockview reproducida y Cronología multidiaria real | Flujos completos de Agent y release |

Las calificaciones no se promedian. Los niveles 1 de las dimensiones 02, 10 y 12 comparten una causa sistémica y determinan el eslabón más débil.

## Weakest links

1. **Renderer compartido de pestañas Dockview:** la misma cabecera imperfecta atraviesa Resumen, Cronología, Agents, proyectos y archivos; combina activación no fiable con ausencia de nombre, rol y selección semántica.
2. **Verdad temporal de Cronología:** el flujo que responde “cuándo” mezcla días, pero sólo muestra horas; la pérdida de contexto puede producir una interpretación falsa de recencia.
3. **Contexto de acciones repetidas:** Actualizar, Tipo de Agent e Iniciar se repiten por repositorio sin identificar su destino en el nombre accesible.

## Prioritized findings

### Now

#### [P1] `POL-10-001` — Las pestañas Dockview centrales carecen de semántica y activación fiables

- Evidencia: Resumen, Cronología, Agents y posteriormente las cabeceras de proyecto/archivo aparecieron como `div` enfocables/clicables sin nombre accesible ni rol `tab`. Al activar Resumen, la cabecera recibió foco pero el panel siguió en Cronología; Ver → Abrir resumen sí hizo la transición (`OBS-2026-07-28-015`, `CODE-2026-07-28-007`).
- Efecto: una navegación frecuente y coprimaria puede comportarse como un control muerto, obliga a descubrir una ruta alternativa y deja opacos el destino y el panel activo para teclado o tecnología asistiva.
- Corrección mínima: adaptar el renderer compartido existente a `tablist`/`tab`/`tabpanel`, con nombre, `aria-selected` y asociación al panel; clic, Enter y Espacio deben invocar la misma selección sin recrear el contenido.
- Verificación: desde cada panel principal y desde pestañas de proyecto/archivo, clic, Enter y Espacio activan el panel esperado en una acción; exactamente una pestaña queda seleccionada, el foco permanece coherente y un filtro activo de Resumen se conserva al volver.
- Afecta: `FLOW-001`, `FLOW-010`, `FLOW-012`, `FLOW-013`, `FLOW-015`, `FLOW-021`, `FLOW-030`; `SURF-004`, `SURF-005`, `SURF-016`, `SURF-017`, `SURF-023`, `SURF-025`; `ROLE-001`; `PLAT-001`.
- Frecuencia: frequent. Esfuerzo: M.

#### [P1] `POL-07-001` — Cronología oculta el día y puede falsear la recencia

- Evidencia: la lista nativa combinó eventos y commits de varias fechas, pero cada fila mostró sólo una hora localizada, sin fecha ni agrupación diaria (`OBS-2026-07-28-009`). Las dos clases de fila usan exclusivamente `toLocaleTimeString()` (`CODE-2026-07-28-001`).
- Efecto: actividad de días distintos puede parecer del mismo día. El usuario no puede responder con confianza “cuándo ocurrió” sin abrir detalles o inferir el cambio de fecha, lo que daña un trabajo central y la confianza en el monitor.
- Corrección mínima: agrupar por fecha localizada cuando cambie el día, conservar la hora en cada fila y exponer fecha/hora completa mediante `<time datetime>`.
- Verificación: con eventos y commits de al menos tres fechas —incluidas horas iguales, cambio de mes y cambio de año— cada fila queda asociada visual y semánticamente a un día inequívoco en 800×600 y 1920×1032; el orden y la apertura de detalle se conservan.
- Afecta: `FLOW-009`, `FLOW-015`; `SURF-023`; `ROLE-001`; `PLAT-001`.
- Frecuencia: frequent. Esfuerzo: S.

### Next

#### [P2] `POL-10-002` — Los controles repetidos del Dashboard no identifican su repositorio

- Evidencia: con dos repositorios, el snapshot expuso dos `Actualizar`, dos `Tipo de Agent` y dos `Iniciar`; abrir y quitar sí incluyeron el repositorio. El código confirma etiquetas estáticas y deja el contexto de fetch sólo en `title` (`OBS-2026-07-28-007`, `CODE-2026-07-28-002`).
- Efecto: una lista de controles, navegación semántica o voz no permite saber qué repositorio recibirá el fetch, proveedor o Agent; aumenta el riesgo de actuar sobre el objetivo equivocado.
- Corrección mínima: usar nombres accesibles únicos con los datos ya presentes: `Actualizar referencias remotas de <repo>`, `Tipo de Agent para <repo>` e `Iniciar <provider> en <repo>`. El texto visual puede permanecer compacto.
- Verificación: con dos o más repositorios y proveedores distintos, cada control tiene nombre único en layouts ancho y estrecho, y una acción simulada apunta sólo al repositorio anunciado.
- Afecta: `FLOW-009`, `FLOW-010`, `FLOW-016`, `FLOW-021`; `SURF-005`, `SURF-025`; `ROLE-001`; `PLAT-001`.
- Frecuencia: frequent. Esfuerzo: S.

### Later

No se aceptaron hallazgos P3 con evidencia suficiente en este pase.

Las propuestas de añadir borrado diferido a conversaciones y desinstalación de Gitleaks dentro de Tinto no se elevaron al backlog: proceden sólo de posibilidades de código raras, las acciones actuales ya tienen fricción proporcional o reversión externa, y no hay evidencia de una necesidad presente que justifique ampliar el producto.

## Flow lifecycle matrix

| Flow | Before | During | After | Failure | Real conditions |
|---|---|---|---|---|---|
| `FLOW-001` bootstrap | Config Pumarejo y perfil existente | Compilación/lanzamiento visible | Shell operativa; cierre `idle` | Retry sólo Code/baseline | Partial |
| `FLOW-009` monitor | Dos repos cargados | Status live y señales | Conteo atribuible a repo | Retención/reconnect Code/Tests | Partial |
| `FLOW-010` filtro/proyecto | Dashboard listo | Coincide/no coincide/reset | Dos repos restaurados; proyecto abierto | Vacío recuperable observado | Covered for sample |
| `FLOW-011` estado repo | Proyecto abierto | Config, señales, cambios, watchlist | Contexto accionable | Missing/error sólo Code | Partial |
| `FLOW-012/013` archivo/diff | Árbol con cambios | Abre `.gitignore` | Diff nombrado y modos visibles | Guardas extremas sólo Code/Tests | Partial |
| `FLOW-015` Timeline | Lista cross-repo | Selección de commit | Detalle visual abierto | Fecha ambigua; refresh/fallo runtime pendiente | Finding/Partial |
| `FLOW-021` Agent | Ningún Agent activo | Lanzadores e historial visibles | No se inició proceso | Runtime ausente/fallo pendiente | Entry only |
| `FLOW-030` vistas/atajos | Shell activa | Menú y diálogo | Escape devuelve foco a Ayuda | Glance/zoom/teclado completo pendientes | Partial |
| Flujos sensibles | Acción explícita | Consentimiento/transaction/checkpoint | Commit o rollback definido | Fault injection Code/Tests | Runtime unverified |

## Keep

- Reflow del Dashboard sin overflow observado a 800×600 y con tabla densa legible a 1920×1032.
- Filtro coincidente, estado sin resultados explicativo y Restablecer disponible.
- Región live que atribuye repositorio, archivos y señales; degradación de Gitleaks que explica el fallback activo.
- Menú Ver con roles, expansión, foco inicial y estados checked.
- Diálogo de atajos con nombre, jerarquía de encabezados, foco de entrada, Escape y retorno de foco.
- Árbol `tree/treeitem`, marcadores de cambio y diff en una región nombrada.
- Selección de commit que abre un detalle visual.
- Historial retenido e incrementalidad de Timeline, y retención de canales/sesiones, demostrados en Code/Tests.
- Transacciones de archivos, manifiestos de recuperación, consentimiento backend y checkpoints de Agent demostrados en Code/Tests.
- Instrumentación Pumarejo limitada a debug + feature, refs generacionales, artefactos no retenidos y cierre normal a `idle`.

## Verification gaps

- Limpiar/confirmar el working tree, actualizar `verified_source_commit` y fingerprint, y repetir el preflight de frescura.
- Recorrer los flujos materiales con NVDA y teclado físico/independiente: Tab/Shift+Tab, F10/Alt, flechas, Enter, Espacio y Escape.
- Probar zoom 200/400 %, movimiento reducido, 640×480 y escalado del SO 125/150/200 %.
- Ejecutar first-run, cero repos, bootstrap lento/fallido y Retry con perfil desechable.
- Provocar un cambio inocuo y medir Dashboard → repo → Live Diff → Timeline; degradar y recuperar cada canal.
- Probar Timeline con varios días, alto volumen, repos rápidos/lentos/fallidos y detalle grande mediante un inspector que no dependa del límite semántico observado de Pumarejo.
- Lanzar un Agent seguro en repo desechable y cubrir starting, running, completed/checkpoint, failed/retry y continuidad del journal.
- Ejecutar file ops, restore/revert y otras consecuencias en repositorio/perfil desechables con fault injection.
- Probar detach/reattach entre ventanas reales.
- Repetir un smoke equivalente en Linux y sobre bundles de release.

## Remediation slices

| Slice | Findings | Expected outcome | Effort | Acceptance checks |
|---|---|---|---|---|
| A — Dock compartido | `POL-10-001` | Navegación principal única, semántica y fiable sin perder contexto | M | Clic/Enter/Espacio, selección única, NVDA, retorno con filtro/pestaña conservados |
| B — Verdad temporal | `POL-07-001` | Timeline multidiaria inequívoca visual y semánticamente | S | Tres fechas, horas iguales, cambio mes/año, 800/1920, detalle intacto |
| C — Acciones contextuales | `POL-10-002` | Cada fetch/selector/lanzamiento identifica su repo/proveedor | S | Nombres únicos con ≥2 repos, ancho/estrecho y dispatch simulado correcto |
| D — Cierre de evidencia | Gaps de cobertura | Atlas fresco y confianza runtime sobre flujos críticos | M | Commit limpio, fingerprint nuevo, NVDA/teclado, Agent/destructivos desechables, Windows/Linux release |

## Handoff

Auditoría completa para el alcance reducido; tres hallazgos quedan abiertos. Esta solicitud autorizó revisión, no implementación, por lo que no se modificó código de producto. El siguiente paso seguro es implementar los slices A–C, actualizar el atlas si cambia la cartografía y repetir la regresión del consejo. El slice D es necesario antes de afirmar que Tinto está pulida de extremo a extremo.

## Remediation addendum — 2026-07-29

Los slices A–C fueron implementados en el working tree:

- `POL-10-001`: adaptador compartido de pestañas con semántica `tablist`/`tab`/`tabpanel`, asociación accesible, estado seleccionado y activación por clic/teclado.
- `POL-07-001`: agrupación local por día y fecha/hora completa mediante `<time datetime>`.
- `POL-10-002`: nombres accesibles únicos por repositorio y proveedor.

La regresión automatizada pasó con 55 archivos/718 pruebas, TypeScript, ESLint y build de producción. La repetición nativa con el Pumarejo actualizado quedó bloqueada antes de iniciar Tinto por `APP_START_FAILED` sin stderr; el MCP cerró en `idle`. Los hallazgos están resueltos en Code/Tests y pendientes de confirmación Observed en runtime nativo. Evidencia: `docs/audits/2026-07-29-product-polish-regression-evidence.md`.
