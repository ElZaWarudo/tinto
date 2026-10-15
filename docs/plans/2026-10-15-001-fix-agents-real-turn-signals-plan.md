---
title: Turnos de Agents solo con señales reales - Plan
type: fix
date: 2026-10-15
artifact_contract: ce-unified-plan/v1
artifact_readiness: implementation-ready
product_contract_source: conversación 2026-10-15
execution: code
---

# Goal Capsule

Que un turno de Agents empiece y termine solo cuando el proveedor lo dice, nunca porque Tinto lo adivine. Codex y ACP ya dan esa señal. Claude Code la da con sus hooks. Las terminales sin señal dejan de tener turnos. El marcador `::tinto-turn-done::` desaparece.

# Product Contract

## Summary

Hoy Agents abre un turno con cualquier salida o actividad de archivos, y en las terminales solo lo cierra si el modelo imprime el marcador que le pide `AGENTS.md`. Cuando no lo imprime, el turno no se cierra nunca: la cola de mensajes no envía, no hay puntos de control por turno, no se puede compactar ni reintentar ACP, y todo se acumula en un único turno al salir. Es la causa probable de que una conversación de Claude abierta en Agents "no llegue a aceptar mensajes", lo que impide usarla como coordinador de Delivery.

Los turnos se quedan, porque lo que cuelga de ellos funciona (deshacer por turno, cola, compactación, reintento ACP). Lo que cambia es de dónde salen.

## Problem Frame

Cómo abre y cierra un turno hoy (`src-tauri/src/agent_console/`):

| Proveedor | Abre | Cierra |
|---|---|---|
| Codex (app-server) | escribir el turno, **cualquier salida**, actividad de archivos | `turn/completed` |
| Kimi/OpenCode Windows (ACP) | igual | respuesta de `session/prompt` |
| Claude Code (terminal) | igual | solo el marcador, o al salir/detener |
| Kimi/OpenCode WSL y ACP caído a terminal | igual | solo el marcador, o al salir/detener |

- `record_output_activity` y `FileActivity` llaman a `note_turn_activity` (`session.rs:746`, `session.rs:899`), así que el banner de arranque de Claude ya abre un turno.
- La ruta de silencio (`refresh_turn_checkpoints(now, false)`, `session.rs:1180`) nunca cierra un turno con base de punto de control: solo alterna entre `Working` y `Settling`.
- El marcador lo pide un bloque de `AGENTS.md` que escribe Tinto desde la configuración del repo (`bus/commands.rs:46`). Nada lo escribe en `CLAUDE.md`.
- Claude en Agents no tiene id de sesión ni se puede reanudar (`pty.rs:130`, `pty.rs:405`); el historial cae a `ContextBridge`.

## Requirements

- **R1.** Un turno se abre solo por una señal de inicio del proveedor o porque Tinto envió el mensaje en una sesión con señal de fin. La salida y la actividad de archivos no abren turnos.
- **R2.** Un turno se cierra solo por una señal de fin del proveedor, por una interrupción que Tinto conoce, o porque el proceso termina o se detiene. Los temporizadores de silencio no cambian el estado del turno.
- **R3.** Claude Code (Windows y WSL) da sus señales con hooks que Tinto pasa al lanzarlo. No depende de lo que el modelo decida imprimir.
- **R4.** Claude Code en Agents tiene id de sesión y se reanuda con `--resume`, también desde el historial y desde "Continuar en Agents" en Delivery.
- **R5.** Una sesión sin señal de turnos es una terminal: lo que se envía va directo, sin cola, sin estado "Trabajando" y con deshacer por sesión.
- **R6.** Si los hooks de Claude no llegan (políticas gestionadas, `disableAllHooks`), la sesión pasa a sin turnos y lo dice.
- **R7.** El marcador desaparece del código, del entorno y del `AGENTS.md` que escribe Tinto.

## Scope Boundaries

Incluye: el origen de los turnos en todas las sesiones de Agents, los hooks de Claude, la reanudación de Claude, el modo sin turnos y la retirada del marcador.

No incluye: ACP dentro de WSL para Kimi/OpenCode (con eso ganarían turnos, pero es otro trabajo), avisos por `permission_prompt` de Claude, cambios en Delivery salvo habilitar "Continuar en Agents" con Claude, ni limpiar el bloque del marcador en el `AGENTS.md` de repos ajenos.

## Acceptance Examples

- **AE1.** Claude en Agents (Windows): al arrancar, la sesión queda en espera, no "Trabajando". Al enviar un mensaje pasa a "Trabajando"; al terminar Claude vuelve a espera con un punto de control del turno. Un mensaje encolado mientras trabajaba se envía al terminar.
- **AE2.** Lo mismo con Claude en WSL.
- **AE3.** Interrumpir desde el botón de Tinto cierra el turno como interrumpido, con su punto de control. Un Esc escrito en la terminal lo cierra el siguiente mensaje o el aviso de inactividad de Claude.
- **AE4.** Tras `/clear`, Tinto guarda el nuevo id de sesión. Reabrir la conversación desde el historial la reanuda con `claude --resume <id>`, sin resumen de contexto.
- **AE5.** Kimi en WSL: Enter envía al momento, no aparece "Trabajando" ni la cola, y deshacer funciona por sesión.
- **AE6.** Codex: sin cambios visibles. Una salida sin turno en curso no abre uno.
- **AE7.** Con `disableAllHooks` en los ajustes del usuario, la sesión de Claude se queda sin turnos y lo indica.
- **AE8.** Un Claude abierto en Agents acepta mensajes y llama a `delivery_overview` del servidor `tinto-delivery`.

# Key Technical Decisions

- **KTD1 (R1, R2). Señal de turnos por sesión.** Cada sesión declara su fuente de turnos: `provider` (Codex, ACP), `hooks` (Claude) o `none`. Con `none` no hay turnos. `note_turn_activity` deja de llamarse desde la salida y desde `FileActivity`. La ruta de silencio solo refresca el registro de cambios; si `Settling` queda sin uso, se elimina del modelo y de la vista.
- **KTD2 (R3). Hooks por `--settings` en línea.** Al lanzar `claude`, Tinto pasa `--settings` con hooks `SessionStart`, `UserPromptSubmit`, `Stop`, `StopFailure`, `Notification` (`idle_prompt`) y `SessionEnd`. No `SubagentStop`: Claude lo dispara por sus propias sugerencias internas (U0). `Stop` se empareja con su `UserPromptSubmit` por `prompt_id`. Se suman a los del usuario, no los sustituyen. Cada uno es `async` y en forma exec (`args`, sin shell): `tinto.exe --agent-hook <archivo de eventos>`. Así no depende de Git Bash ni de comillas en Windows.
- **KTD3 (R3). Eventos por archivo, no por red.** `tinto.exe --agent-hook` lee el JSON del hook por stdin y añade una línea al archivo de eventos de la sesión, en la carpeta de datos de Tinto. Tinto lee las líneas nuevas en el tick de estado que ya existe (500 ms). Sin puerto ni token. En WSL el hook ejecuta `tinto.exe` por su ruta `/mnt/c/…`, como ya hace `--delivery-mcp-proxy`, y el archivo se pasa como ruta de Windows.
- **KTD4 (R2, AE3). Interrupción.** `Stop` no se dispara cuando el usuario interrumpe. El turno interrumpido se cierra con lo primero que llegue: el `interrupt_turn` de Tinto para Claude (escribe Esc y cierra el turno como interrumpido), el siguiente `UserPromptSubmit` (cierra el anterior y abre otro), `Notification` `idle_prompt`, `StopFailure` o la salida del proceso.
- **KTD5 (R4). Id de sesión elegido por Tinto.** Tinto lanza `claude --session-id <uuid>` y lo guarda como id del proveedor. `SessionStart` con `source: clear` o `fork` actualiza el id. Reanudar usa `claude --resume <id>`.
- **KTD6 (R6). Sin hooks, sin turnos.** Una sesión de Claude empieza como `none` y pasa a `hooks` cuando llega `SessionStart`. Sin plazo: `SessionStart` solo llega después del diálogo de confianza de la carpeta, que el usuario puede tardar en contestar (U0). Si no llega nunca, la sesión sigue sin turnos y lo indica.
- **KTD7 (R5). Modo sin turnos.** Kimi/OpenCode en WSL, ACP caído a terminal y Claude sin hooks. El compositor envía directo. Compactar sigue sin soporte, y el reintento ACP se permite en cualquier momento.
- **KTD8 (R7). Retirada del marcador.** Se quitan `TINTO_TURN_DONE_MARKER`, su variable de entorno (`pty.rs:757`, `wsl_agent/shell_env.rs:72`), su detección (`acp.rs:3817-3900`, `commands.rs:3702-3725`), su ocultación en `TerminalPanel.tsx`, la sección de `AGENTS.md` con su opción en la configuración del repo y la petición `CreateAgentsMdConfig` del helper de WSL (siguiendo la convención de versión del protocolo). También el bloque de este repo. Los `AGENTS.md` ya escritos en otros repos no se tocan: el `printf` de una variable vacía es inofensivo.

# Implementation Units

## U0 — Verificación con la CLI real

**Goal:** confirmar lo que la documentación no garantiza, antes de escribir código.

**Approach:** con el `claude` instalado, en una PTY en Windows y en WSL, lanzado con `--session-id` y `--settings` en línea con hooks que escriben a un archivo:

- Llegan `SessionStart`, `UserPromptSubmit` y `Stop` sin aceptar ningún diálogo extra. Comprobar también en un worktree nuevo, sin confianza previa en la carpeta.
- Qué llega con Esc, con `/clear` (¿cambia el id?), con un comando local (`/help`) y con un subagente.
- Cuánto tarda `idle_prompt`.
- El hook en forma exec desde WSL puede ejecutar `tinto.exe` por `/mnt/c/…`.
- Reproducir en la app que Claude en Agents no acepta mensajes y confirmar que el turno queda abierto por el banner.

**Exit:** si los hooks por `--settings` no llegan en modo interactivo, se para aquí y se replantea (alternativa: transporte `stream-json`).

**Resultado (2026-10-15).** Con un arnés PTY (`portable-pty`, el mismo crate que Agents) que lanza `claude` como lo hace Tinto, con `--session-id` y `--settings` en línea, y hooks `async` en forma exec que apuntan a un `.exe` de Windows que añade el JSON a un archivo:

- **WSL, Claude Code 2.1.285: funciona.** Los hooks llegan sin ningún diálogo extra, desde WSL a un `.exe` de Windows por `/mnt/c/…` y con un archivo de eventos en ruta de Windows. Latencia: `UserPromptSubmit` unos 0,4 s después del Enter.
- **`SessionStart` llega después del diálogo de confianza** de una carpeta nueva, no al arrancar. Por eso KTD6 no usa plazo.
- **Esc no dispara nada.** Ni `Stop` ni ningún otro evento hasta el siguiente.
- **`/clear`** dispara `SessionEnd` (`reason: clear`) y `SessionStart` (`source: clear`) con un id nuevo. No dispara `UserPromptSubmit`.
- **`--resume <id>`** dispara `SessionStart` (`source: resume`) con el mismo id y conserva la conversación.
- **`idle_prompt`** llega unos 60 s después del último `Stop`.
- **`SubagentStop`** llega sin subagentes del usuario, por las sugerencias internas de Claude. Se ignora.
- **Cerrar el proceso** dispara `SessionEnd` (`reason: other`).
- Cada `Stop` lleva el `prompt_id` de su `UserPromptSubmit`.
- **Windows nativo: pendiente.** El `claude.exe` que instala la app de escritorio (2.1.295) no tiene sesión iniciada como CLI: arranca en el asistente de inicio y en el login. Hace falta que el usuario ejecute `claude auth login` con ese binario. Además, ConPTY pide la posición del cursor (`ESC[6n`) antes de mostrar nada; xterm.js ya la contesta.
- **Claude atascado en Agents: confirmado en el código, no en la app.** El banner de arranque llama a `record_output_activity`, que abre el turno. Con punto de control base y sin cambios de archivos, la ruta de silencio nunca vuelve a `Waiting` (lo fija el test `output_only_turn_stays_working_until_provider_completes_it`, `session.rs:3019`). El compositor, con el turno activo, encola el primer mensaje (`TerminalPanel.tsx:1597`), y la cola solo envía con `turn_status === "waiting"` (`TerminalPanel.tsx:991-1004`). El primer mensaje no sale nunca.

## U1 — Turnos solo con señal

**Requirements:** R1, R2, KTD1.

**Approach:** fuente de turnos por sesión. Quitar la apertura por salida y por `FileActivity`. Abrir al escribir un turno solo si la fuente no es `none`. La ruta de silencio deja de tocar `turn_status`. Revisar los 18 usos de `Settling` y quitarlo si queda sin sentido.

**Test scenarios:** la salida sin turno no abre uno (Codex, ACP); un `TurnCompleted` cierra con punto de control; con fuente `none` escribir no abre turno; la ruta de silencio no cambia el estado.

**Hecho (2026-10-15).** La fuente de turnos es `AgentProcess::reports_turns`: Codex sí; ACP solo en modo ACP, no en su terminal de reserva; la terminal no. Un turno se abre con `TurnStarted`, que es el `turn/started` raíz de Codex o el `session/prompt` enviado en ACP (antes, `FileActivity`), o al enviar un turno desde Tinto si el proceso da turnos. La entrada cruda, la salida y los cambios de archivos ya no abren turnos. `refresh_turn_checkpoints` pasó a ser `close_turn`, que solo llaman el fin del proveedor, un error o el fin del proceso. Cerrar sin turno abierto no registra ninguno. `Settling` salió del contrato y de la vista. Mientras no llegue U2, Claude no tiene turnos: el primer mensaje ya no se queda en cola, pero tampoco hay deshacer por turno.

## U2 — Hooks de Claude

**Requirements:** R3, R6, KTD2–KTD4, KTD6. **Dependencies:** U0, U1.

**Approach:** `--settings` con los hooks al lanzar Claude en Windows y en WSL (`pty.rs`, junto a `wiring::claude_args`). Modo `--agent-hook` en `main.rs`, que añade la línea y sale. Lectura del archivo de eventos en el tick y traducción a abrir, cerrar o interrumpir el turno. `interrupt_turn` para Claude. La sesión pasa de `none` a `hooks` con `SessionStart`.

**Test scenarios:** cada evento produce la transición esperada; `UserPromptSubmit` con un turno abierto cierra el anterior como interrumpido; `SubagentStop` no cambia nada; una línea parcial o corrupta se ignora sin perder las siguientes; sin `SessionStart` la sesión sigue en `none`.

**Hecho (2026-10-15).** `agent_console/claude_hooks.rs` crea el archivo de eventos en `<config de Tinto>/agent-hooks/<uuid>.jsonl`, que se borra con la sesión. También arma el `--settings` y traduce las líneas en `TurnStarted` y `TurnCompleted`. `PtyHandle` lo añade al lanzar Claude, en Windows y en WSL. Da turnos (`reports_turns`) desde el primer hook, que llega tras el diálogo de confianza, y los lee en `drain_events`. Enviar un turno no abre ninguno (`turn_starts_on_send` es falso): lo abre `UserPromptSubmit`, así un comando local como `/help` no deja un turno colgado. Interrumpir desde Tinto escribe Esc y cierra el turno en la siguiente lectura. El monitor de salida de cada sesión vigila además el tamaño del archivo y refresca la vista cuando crece, porque `Stop` e `idle_prompt` pueden llegar sin salida alrededor. `tinto.exe --agent-hook <archivo>` lee el JSON por stdin y lo añade compactado en una línea.

Comprobado con la CLI real en WSL (2.1.285): Claude, lanzado con esos hooks contra el `tinto.exe` de desarrollo por `/mnt/c/…`, dejó una línea por evento. Fueron `SessionStart`, el `UserPromptSubmit` y el `Stop` del primer mensaje, el `UserPromptSubmit` del mensaje interrumpido con Esc (sin `Stop`) y el par del siguiente, que según los tests cierra el interrumpido antes de abrir el nuevo. Falta comprobarlo dentro de la app y en Windows nativo.

## U3 — Reanudar Claude

**Requirements:** R4, KTD5. **Dependencies:** U2.

**Approach:** `--session-id` al lanzar, `provider_session_id` con el id vigente, `resume_agent`/`resume_wsl_agent` aceptan Claude con `--resume`. El historial reanuda de forma nativa. En Delivery se muestra "Continuar en Agents" para trabajos de Claude, que ya corren con `--session-id`.

**Test scenarios:** argumentos de lanzamiento y de reanudación en Windows y WSL; `/clear` actualiza el id guardado; el journal guarda el id.

**Hecho (2026-10-15), con un cambio respecto a KTD5.** Tinto no fija el id con `--session-id`, sino que lo toma de los hooks. Cada evento lleva su `session_id`, y el id que se guarda es el de la última conversación que recibió un mensaje (`UserPromptSubmit`). Así una sesión que no pasó del diálogo de confianza no deja un id sin conversación detrás, y tras `/clear` el id nuevo solo cuenta cuando esa conversación tiene algo. Al reanudar, el id de partida es el reanudado. `PtyHandle` lo expone como `provider_session_id`, y el journal ya lo guarda. `PortablePtyFactory::resume_agent` y `resume_wsl_agent` lanzan Claude con `--resume <id>` y los hooks. Delivery muestra "Continuar en Agents" también en trabajos de Claude. El backend lo rechaza, con su motivo, si el trabajo corrió en WSL sobre un repo de Windows, porque Agents ejecuta el agente de ese repo en Windows y Claude guarda sus conversaciones donde corrió. Si la conversación ya no existe, Claude lo dice en su terminal; no hay vuelta automática al resumen de contexto.

Comprobado en WSL (2.1.285): un trabajo sin interfaz como los de Delivery (`claude -p --session-id`) se reanudó en interactivo con `--resume` y los hooks contra el `tinto.exe` de desarrollo. Llegó `SessionStart` (`source: resume`) con el mismo id, Claude recordó su respuesta anterior y el turno nuevo dejó su `UserPromptSubmit` y su `Stop`. Una carpeta nueva pide el diálogo de confianza también al reanudar. Falta probarlo dentro de la app.

## U4 — Sesiones sin turnos

**Requirements:** R5, KTD7. **Dependencies:** U1.

**Approach:** la vista lee la fuente de turnos. Con `none`: envío directo, sin cola, sin "Trabajando", deshacer por sesión, reintento ACP siempre disponible.

**Test scenarios:** el compositor no encola con fuente `none`; el estado no muestra "Trabajando"; restaurar a una sesión sigue funcionando.

**Hecho (2026-10-15).** Casi todo el comportamiento ya venía de U1. Una sesión sin turnos queda siempre en `waiting`, así que el compositor envía directo y sin cola, "Trabajando" no aparece y el reintento ACP, que pide el turno en espera, está siempre disponible. Lo que faltaba era decirlo. El contrato de la sesión lleva `reports_turns`, que es verdadero mientras corre y el proceso da turnos; se omite si es falso. La vista muestra una sesión en marcha sin turnos como "Terminal sin turnos" ("Lo que envíes llega directo al agente; los cambios se deshacen por sesión"), y su estado como "Sin turnos" en lugar de "En espera". Deshacer por sesión es la reversión que ya existía.

## U5 — Retirar el marcador

**Requirements:** R7, KTD8. **Dependencies:** U2, U4.

**Approach:** borrar todo lo listado en KTD8 y sus tests. Quitar `agents_md_configured` del contrato y la opción de la configuración del repo.

**Test scenarios:** ninguna referencia a `TINTO_TURN_DONE_MARKER` ni a `tinto-turn-done`; la configuración del repo ya no ofrece AGENTS.md.

# Risks

- **Versiones de Claude Code.** Los hooks cambian entre versiones; U0 fija la versión comprobada y KTD6 cubre el caso en que no lleguen.
- **Hooks `Stop` del usuario que hacen continuar a Claude.** Corren en paralelo con el de Tinto, que cierra el turno en el primer `Stop`; la continuación cae en el siguiente turno. Se documenta como límite.
- **Política gestionada `allowManagedHooksOnly`.** Bloquea los hooks de Tinto; la sesión queda sin turnos (KTD6).
- **Arranque de `tinto.exe` por evento.** Es `async`, así que no retrasa a Claude; si U0 muestra una latencia excesiva en WSL, se revisa.

# Verification Contract

Cada unidad termina con sus tests, `cargo clippy --all-targets -- -D warnings` y la suite del frontend. Al final, AE1–AE8 comprobados en la app: Claude en Windows y en WSL, Codex, y Kimi en WSL.

# Definition of Done

Ningún turno de Agents depende de lo que imprima el modelo. Claude en Agents acepta mensajes, cierra sus turnos, se reanuda y puede coordinar Delivery. Las terminales sin señal funcionan como terminales.
