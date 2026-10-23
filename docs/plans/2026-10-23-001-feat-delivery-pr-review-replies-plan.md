---
title: Delivery - Revisores del PR y respuestas a comentarios de revisión
type: feat
date: 2026-10-23
artifact_readiness: partes 1, 2 y 3 hechas (sin commit)
execution: code
---

# Objetivo

Que la conversación con los revisores de un PR pase por Tinto como el resto de la entrega: el
usuario elige quién revisa y vuelve a pedirlo desde la tarea, y las respuestas a los comentarios
de revisión se aprueban y se publican desde Tinto, con su URL guardada, en vez de vivir en el chat
del coordinador.

# Parte 1 — Cajas de texto que crecen (hecho)

**Problema.** Los textareas de Delivery tenían filas fijas (`rows={2}`), así que una respuesta, un
mensaje de commit o un cuerpo de Jira se leían con scroll interno aunque el panel tuviera sitio.

**Cambio.** `src/panels/delivery/GrowingTextarea.tsx`: un textarea que ajusta su altura a su texto en
cada cambio (`scrollHeight` más los bordes). `rows` queda como mínimo. Sustituye a los 16 textareas
de `DeliveryDecisions`, `DeliveryDialogs`, `DeliveryCommitPlan`, `DeliveryQaProblems` y
`DeliveryPanel`. Se usa JS y no `field-sizing: content` porque Tinto se compila también para Linux y
macOS, cuyos webviews (WebKit) no lo soportan.

# Parte 2 — Revisores del PR abierto (hecho)

**Antes.** Los revisores solo se elegían al aprobar el paso PR, y Tinto se los pedía una vez. No
había forma de volver a pedir la revisión, de añadir a alguien después ni de quitar una petición.

**Cambio.**

- `prs.rs`: `PR_FIELDS` lee `reviewRequests`. `DeliveryPr.requested_reviewers` guarda a quién espera
  GitHub; los equipos (sin `login`) se omiten.
- `GitHub::remove_reviewers` (`gh pr edit --remove-reviewer`), junto a `request_reviewers`.
- `DeliveryService::change_pr_reviewers(task_id, request, remove)`: solo con el PR abierto
  (`pr_not_open` si no). Quita primero, pide después y relee el PR.
- Comando Tauri `delivery_change_pr_reviewers`, cliente `changeDeliveryPrReviewers`.
- UI (`DeliveryPr.tsx`, `PrReviewers`): bajo los datos del PR abierto, un desplegable
  «Revisores: esperando a …». Al abrirlo lista a quien puede revisar (lee GitHub la primera vez),
  marcados «pedida» o «aprobó». Se marcan personas y se elige «Pedir revisión» (sirve para pedir por
  primera vez y para volver a pedir) o «Quitar petición» (solo a quien la tiene pedida).
- Test: `the_user_asks_for_a_review_again_or_takes_a_request_back_on_the_open_pr`.

**Pendiente de verificar en un PR real:** que `gh pr edit --add-reviewer` sobre alguien que ya
revisó vuelva a notificarle (el «re-request» de GitHub). Si no, usar
`gh api -X POST repos/<o>/<r>/pulls/<n>/requested_reviewers -f "reviewers[]=<login>"`.

# Parte 3 — Paso `pr_reply` (hecho)

## Cómo funciona hoy y por qué falla

Tinto no tiene paso para responder. El coordinador pide una decisión `text` con `request_decisions`,
con el comentario en `references` (por eso la tarjeta muestra el comentario y su diff), y tras la
aprobación publica él mismo con `gh`, fuera de Tinto. Problemas:

1. **Bloquea la tarea.** Las decisiones bloquean por defecto (`decisions_pending`): ninguna etapa se
   lanza hasta contestar una respuesta que no tiene nada que ver con las etapas.
2. **En ejecución desatendida se publica sin el usuario.** Tras `end_brainstorm`, Tinto acepta los
   textos tal como se proponen (`auto`): una respuesta saldría con la cuenta del usuario sin que la vea.
3. **«Aceptar recomendadas» la incluye.** `PendingDecisions` cuenta cada texto como aceptable.
4. **Tinto no lo registra.** No guarda la URL publicada ni evita publicar dos veces si el
   coordinador reintenta.

## Qué construir

Un peldaño de aprobación nuevo, `pr_reply`, pedido con `request_approval`.

- **Campos.** `comment_url`: el comentario de revisión (`…/pull/<n>#discussion_r<id>`) o el PR para
  un comentario general. `body`: el texto exacto. `resolve_thread` (opcional): resolver el hilo tras
  responder.
- **Varios por tarea.** Hoy cada peldaño admite una petición abierta por tarea (`approval_exists` en
  `coordination.rs`). `pr_reply` debe admitir una por `comment_url`: la segunda para el mismo
  comentario se rechaza, otra para otro comentario no.
- **No cuenta en la escalera.** La tarea sigue entregada con commit, push, pr, merge y Jira;
  `pr_reply` no se exige ni impide nada.
- **No bloquea etapas** y **nunca se aprueba solo**, ni en ejecución desatendida (como pr, merge y
  jira hoy). No entra en «Aceptar recomendadas».
- **Tarjeta.** El comentario con su diff (reutilizar `CodeThreads` y la lectura de `threads.rs`), el
  texto editable con `GrowingTextarea` y «Aprobar y responder» / «Rechazar». Si el usuario lo edita,
  el texto editado es el que se publica y se guarda.
- **Publicación al aprobar.** Hilo de revisión:
  `gh api repos/<o>/<r>/pulls/<n>/comments/<id>/replies -f body=…`. Comentario general:
  `gh pr comment <url> --body-file -`. Con `resolve_thread`: buscar el hilo del comentario en
  `reviewThreads` (GraphQL) y `resolveReviewThread`. El `outcome` del paso es la URL publicada.
- **Idempotencia.** Antes de publicar, leer el hilo (`pr_review_comments` ya existe) y, si hay una
  respuesta del usuario (`in_reply_to_id` = id) con el mismo texto, dar el paso por hecho con esa
  URL en vez de publicar otra.
- **Errores claros, sin reintento automático.** Comentario borrado (404), hilo bloqueado o `gh` sin
  sesión: el paso falla con un motivo en lenguaje llano.
- **Instrucciones MCP.** `request_approval` documenta `pr_reply`; `request_decisions` dice que una
  respuesta a un revisor no es una decisión `text`.

## Fuera de alcance (por ahora)

- Campo `commits` en la respuesta: el texto puede citar el SHA.
- Listar los hilos sin resolver en `read_pr` / `delivery_overview` y un evento de `wait_run` por
  comentario nuevo: hoy ya llegan como novedades del PR y abren una ronda con «cambios pedidos». Se
  añade si los coordinadores siguen perdiendo comentarios.

## Dónde mirar

- `src-tauri/src/delivery/mcp.rs`: `request_approval` (enum de `rung`, ~línea 658 y su descripción),
  `request_decisions`.
- `src-tauri/src/delivery/coordination.rs`: creación de aprobaciones y `approval_exists`.
- `src-tauri/src/delivery/prs.rs`: trait `GitHub`, `GhCli`, patrón de `request_pr_reviewers`.
- `src-tauri/src/delivery/threads.rs`: parseo de `comment_url` y lectura del comentario y su diff.
- `src-tauri/src/delivery/model.rs`: `DeliveryRung`, `DeliveryApproval`.
- `src/panels/delivery/DeliveryPanel.tsx`: tarjeta de aprobación (`aria-label="Aprobación …"`).
- `src/panels/delivery/DeliveryDecisions.tsx`: `CodeThreads`, `TextCard`, `PendingDecisions`.

## Hecho cuando

El coordinador pide una respuesta a un comentario de revisión con `pr_reply`. El usuario la ve en
Tinto con el comentario y su diff, la edita y la aprueba. Tinto la publica una sola vez en el hilo
correcto y la URL queda en la tarea, sin bloquear sus etapas y sin aprobarse nunca sola.

## Cómo quedó la parte 3

- `DeliveryRung::PrReply`; `DeliveryApproval` guarda `comment_url`, `resolve_thread` y `thread` (el
  comentario y su diff leídos al pedirla). `approval_exists` compara también `comment_url`.
- `phase.rs` no mira las respuestas: no ponen la tarea en `approval` ni frenan etapas.
- `DeliveryService::post_reply` (`prs.rs`): responde en el hilo del primer comentario (GitHub no
  admite respuestas a una respuesta), o `gh pr comment --body` para un comentario general (`--body`
  y no `--body-file -`, porque `GhCli` ejecuta `gh` sin stdin). Si resolver el hilo falla, el paso
  queda hecho con un `notice`.
- La idempotencia solo cubre los hilos de revisión: un comentario general no se busca antes de
  publicar.
- UI: `ReplyBanner` en `DeliveryPanel.tsx`; las respuestas publicadas o fallidas se listan en
  «Resultados de la entrega» con su URL.
- Tests: `a_reply_to_a_reviewer_waits_for_the_user_and_is_posted_once_in_its_thread`,
  `a_reply_github_refused_says_why_in_plain_words` y el test de panel de la tarjeta.
- Pendiente de verificar en un PR real: la consulta GraphQL de `resolveReviewThread` y que
  `gh pr comment` imprima la URL del comentario.
