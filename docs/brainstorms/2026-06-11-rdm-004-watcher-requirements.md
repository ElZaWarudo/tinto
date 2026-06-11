---
date: 2026-06-11
topic: rdm-004-watcher
---

# RDM-004 — Watcher con debounce y throttling por repo: requisitos

## Summary

Construir el productor de eventos del sistema (§7): un watcher basado en `notify` que observa los working dirs de los repos del **workbench activo**, clasifica cada evento de FS con el `PathClassifier` ya entregado (RDM-003), agrupa ráfagas con debounce (~200–400 ms) y limita la frecuencia de emisión por repo (throttling). Entrega lotes de eventos clasificados por repo a través de un canal interno que RDM-006 (bus) consumirá. Sin recálculo de git, sin emisión al frontend, sin UI.

## Key Decisions

- **Scope: solo el workbench activo.** El watcher monta watches únicamente sobre los repos del workbench activo (`WorkbenchStore::active_workbench()`, RDM-005) y desmonta/remonta al conmutar. Mínimo consumo de handles/RAM (principio liviano §1). La opción "todos los workbenches, configurable" del diseño §7 se difiere hasta necesidad real. Decisión de usuario 2026-06-11.
- **Boundary con RDM-006 (resuelve la open question del roadmap):** capas distintas, sin duplicación. **RDM-004 (debounce + throttle)** opera sobre **eventos de FS crudos**: agrupa ráfagas por repo en un lote tras un periodo de calma (~200–400 ms) y limita la frecuencia de lotes por repo. **RDM-006 (coalescing + emit-throttle)** opera sobre **deltas calculados de git**: fusiona deltas y limita la emisión al frontend. El watcher nunca recalcula git; el bus nunca debounce-a eventos de FS.
- **El clasificador ya decide; el watcher enruta.** Cada repo observado lleva su `PathClassifier` (construido con el `fs_watch` del repo). El watcher descarta `GitInternal`/`Ignored`/`OutsideRepo` y pasa `GitMeta` (señal de commit/branch), `Plane1` y `Plane2` en el lote. Ante eventos sobre archivos `.gitignore`, el watcher **reconstruye el clasificador** del repo (contrato documentado del módulo `paths`).
- **Entregable: subsistema con canal interno, no integración.** RDM-006 no existe aún; el watcher expone un API de suscripción/canal que el bus consumirá. **Por el canal viajan mensajes con dos variantes: lotes de eventos clasificados por repo, y estados de error por repo** (un solo mecanismo de entrega; sin store de estado vivo aparte — eso es territorio de RDM-006). La forma exacta del canal la fija el plan; el contrato de consumo se congela en RDM-006.
- **Disparador del remount: API explícito, wiring diferido.** El watcher expone un API explícito de montaje/remontaje por workbench (los tests lo ejercen directamente); cablear ese API al comando de conmutación de RDM-005 es integración y pertenece a RDM-006. No se modifican los comandos de RDM-005 en este item.

## Requirements

**Montaje y ciclo de vida**

- R1. El watcher observa recursivamente los working dirs de todos los repos del workbench activo, usando `notify` (backend nativo por plataforma).
- R2. Al conmutar de workbench activo (vía el API de remontaje del watcher), el watcher desmonta los watches del anterior y monta los del nuevo, sin reiniciar la app.
- R3. El fallo de watch de un repo individual se reporta **como mensaje de error de ese repo por el mismo canal de R11**, sin tumbar los watches de los demás repos ni el watcher global. Dos vías: los fallos de montaje (path inexistente, permiso denegado) los devuelve `watcher.watch()` como `Result`; la remoción en caliente del repo **no produce un error de notify** (inotify remueve el watch silenciosamente) — el watcher la **sintetiza** detectando el borrado del root del repo.
- R4. El watcher se apaga limpiamente (sin leaks de handles ni threads colgados) al cerrar la app o al desmontarse.

**Clasificación y filtrado**

- R5. Cada evento de FS se clasifica con el `PathClassifier` del repo afectado: `GitInternal`, `Ignored` y `OutsideRepo` se descartan; `GitMeta`, `Plane1` y `Plane2` se incluyen en el lote con su clasificación, tipo de evento (creado/modificado/borrado), path y timestamp. **Regla de normalización de kinds de notify:** los renames se descomponen en borrado(path origen) + creado(path destino), cada mitad clasificada por su propio path (cubre saves atómicos de editores/agentes); kinds desconocidos/`Any` se mapean conservadoramente a "modificado"; eventos `Access` se descartan antes de clasificar. Esta taxonomía alimenta el contrato que RDM-006 congelará.
- R6. Un evento sobre un archivo `.gitignore` del repo dispara la reconstrucción del `PathClassifier` de ese repo (los eventos posteriores se clasifican con las reglas nuevas). La reconstrucción hace I/O (walk del árbol): se coalesce dentro de la ventana de debounce (una ráfaga que toca `.gitignore` varias veces produce una reconstrucción, no N).
- R7. El watcher no hace `stat` en el hot path por evento: el flag `is_dir` se deriva del `EventKind` de notify **cuando el backend lo provee** (Create/Remove File|Folder en inotify); en su ausencia (eventos Modify, kinds `Any` de Windows) se usa `false` como default documentado — consistente con el contrato de `classify` para borrados, y correcto para patrones de directorio porque el clasificador evalúa los ancestros como directorios. Edge aceptado: eventos sobre el directorio mismo sin kind tipado.

**Debounce y throttling**

- R8. Los eventos clasificados de un repo se agrupan en un lote que se emite tras un periodo de calma de ~200–400 ms (debounce por repo): una ráfaga de N escrituras del agente produce un lote, no N emisiones.
- R9. Throttling por repo: aun con actividad continua (sin calma), el watcher emite como máximo un lote por repo por intervalo configurado (p. ej. ≥ el debounce), acumulando lo demás en el lote siguiente. Ningún evento incluible se pierde: se difiere.
- R10. Los lotes de repos distintos son independientes: la actividad intensa de un repo no retrasa los lotes de otro.

**Entrega**

- R11. El watcher expone un mecanismo de suscripción interno (canal) por el que un consumidor (RDM-006) recibe los lotes `(repo, Vec<evento clasificado>)` en orden de emisión.
- R12. Hay tests que cubren: clasificación/enrutado de eventos por bucket (incl. normalización de renames), agrupación por debounce, throttling bajo actividad continua, independencia entre repos, reconstrucción del clasificador ante `.gitignore`, fallo aislado de un repo, **conmutación de workbench activo (desmonte/remonte vía el API)**, orden de entrega del canal y desmontaje limpio. Las aserciones de timing (lotes por debounce/throttle) se hacen sobre **conteo de lotes con tolerancia**, no sobre timing exacto, para no producir tests flaky.

## Acceptance Examples

- AE1. **Covers R8.** Con el watcher montado sobre un repo de fixture, escribir 20 archivos en <100 ms produce **un** lote que contiene los 20 eventos clasificados (no 20 lotes).
- AE2. **Covers R5.** Tocar `src/main.rs` (trackeable), `target/x.o` (ignorado), `.env` (gitignoreado + en `fs_watch`) y `.git/HEAD` produce un lote con `Plane1`, `Plane2` y `GitMeta` — sin el `Ignored`.
- AE3. **Covers R9, R10.** Actividad continua sobre el repo A (sin calma) produce lotes a ritmo acotado (≤1 por intervalo) mientras un evento único en el repo B se entrega en su propio lote sin esperar al throttle de A.
- AE4. **Covers R2.** Conmutar el workbench activo desmonta los watches del workbench anterior (sus eventos dejan de llegar) y los eventos del nuevo workbench llegan.
- AE5. **Covers R3.** Borrar del disco uno de los repos observados produce un mensaje de error de ese repo por el canal (sintetizado por el watcher al detectar el borrado del root — notify no emite error); los demás repos siguen produciendo lotes. Montar un repo con path inexistente produce el mensaje de error de montaje sin afectar a los demás.
- AE6. **Covers R6.** Añadir un patrón a `.gitignore` que ignora `logs/` hace que eventos posteriores bajo `logs/` dejen de aparecer como `Plane1`.

## Scope Boundaries

- **Incluye:** watcher `notify` sobre el workbench activo, clasificación vía `PathClassifier`, debounce + throttling por repo, canal interno de lotes, manejo de errores por repo, tests.
- **Excluye:** recálculo de git status/diff (lo dispara RDM-006 con el git engine), emisión de eventos al frontend (RDM-006), UI (RDM-007+), edición de patrones `fs_watch` (RDM-009), watching de workbenches no activos (diferido), persistencia de eventos (memoria solamente).

## Dependencies / Assumptions

- Depende de RDM-003 (`PathClassifier`) ✅ y RDM-005 (`WorkbenchStore` / workbench activo) ✅, ambos en `develop`. RDM-001 ✅.
- Asume `notify` con sus backends nativos (inotify en Linux, ReadDirectoryChangesW en Windows). Límites de inotify watches (`max_user_watches`) en repos enormes son un riesgo conocido a documentar, no a resolver aquí.
- El tuning fino de debounce/throttle (valores exactos dentro de ~200–400 ms) es del plan/ejecución; los valores quedan en constantes ajustables.
- Verificación en Linux (máquina actual); Windows best-effort hasta CI (D2).

## Outstanding Questions

- **Deferred to Planning:** forma exacta del canal (tokio mpsc vs broadcast; quién posee el receiver), estructura del tipo de mensaje (lote | error de repo), y dónde vive el estado del watcher (managed state de Tauri vs struct propio inicializado en `setup`). Nota: tokio necesitará la feature `sync` (hoy solo `time`).
- **Deferred to Planning:** estrategia de debounce: implementación propia sobre canal + timer vs `notify-debouncer-*`. **Criterio de evaluación obligatorio: testabilidad determinista** — los `notify-debouncer-*` corren threads propios sobre reloj de pared (incompatibles con `tokio::time::pause`), lo que pesa fuerte a favor de la implementación propia sobre timers de tokio; evaluar también contra el requisito de throttling por repo.
- **Deferred to Planning:** qué hace el watcher en el arranque si no hay workbench activo (esperar señal de RDM-005/006 vs montar vacío).
- **Cross-item (RDM-006):** el contrato exacto de consumo del canal se congela en RDM-006 junto con el contrato de eventos backend↔frontend.
