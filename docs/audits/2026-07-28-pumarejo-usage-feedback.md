# Feedback de uso real para Pumarejo

## Contexto

Pumarejo se usó como MCP real por `stdio` para auditar una aplicación Tauri nativa en Windows. El recorrido incluyó lanzamiento visible, snapshots semánticos, capturas, navegación por menús y pestañas, escritura en filtros, teclas, apertura de un repositorio y un diff, y cierre de la sesión.

La base funciona: el modelo de sesión propiedad del driver es comprensible, las referencias generacionales reducen acciones sobre elementos obsoletos, las capturas fueron útiles y el cierre normal terminó en estado `idle`. La integración opcional de Tinto también quedó correctamente aislada detrás de `debug_assertions` y una feature explícita.

Las mejoras siguientes surgen de problemas reproducidos durante ese uso, no de una revisión teórica.

## Mejoras de prioridad alta

### 1. Hacer que los snapshots escalen y protejan contenido sensible

Un detalle grande de Timeline se renderizó y pudo capturarse como imagen, pero el snapshot semántico posterior terminó en `INTERNAL_ERROR`. En otra superficie, los nombres accesibles de conversaciones recientes contenían fragmentos muy largos de transcripciones reales.

Pumarejo debería permitir acotar el resultado:

- `rootRef` o snapshot de un subárbol;
- `maxNodes`, `maxDepth` y `maxTextLength`;
- `visibleOnly`;
- filtros por rol, nombre o tipo;
- omisión opcional de texto y atributos voluminosos;
- paginación o cursor;
- metadatos explícitos de truncado.

También convendría incorporar redacción configurable y límites conservadores por defecto para campos que pueden contener conversaciones, rutas, tokens o contenido de archivos. Un snapshot demasiado grande no debería convertirse en un error opaco: debería devolver una respuesta parcial válida, indicar qué se truncó y sugerir cómo pedir el resto.

### 2. Añadir progreso y un modelo robusto para operaciones largas

El primer lanzamiento nativo tardó cerca de dos minutos. Con el timeout habitual de un cliente MCP, una operación así parece colgada o termina cancelada aunque la aplicación esté compilando y arrancando correctamente.

`tauri_launch` debería publicar progreso por etapas —resolución del comando, compilación, proceso Tauri, WebDriver, ventana encontrada, primer snapshot— y declarar un timeout recomendado. Para operaciones largas, sería mejor devolver una tarea o sesión pendiente que pueda consultarse, en vez de mantener una única petición bloqueada.

La cancelación también debe ser una ruta de primer nivel. Después de cancelar una interacción lenta, una sesión anterior produjo varios `CLOSE_FAILED`; cerrar el transporte terminó limpiándola. `tauri_close` debería:

- ser idempotente incluso durante una cancelación;
- distinguir `closing`, `already closed`, `driver unavailable` y `process cleanup failed`;
- aplicar un fallback controlado de cierre del árbol de procesos;
- devolver diagnósticos de qué recurso quedó vivo;
- permitir reintentar sin convertir el estado intermedio en un fallo definitivo.

### 3. Mejorar la fidelidad del teclado y de las ventanas de escritorio

Al enviar `TAB` repetidamente en Agents, el snapshot siguió informando foco en `Minimizar`. No fue posible determinar si el origen era Tinto, WebDriver o la implementación de `tauri_press_key`.

Pumarejo debería devolver, para cada tecla:

- elemento enfocado antes y después;
- método de despacho utilizado;
- si el evento fue cancelado;
- si el foco cruzó chrome, WebView, diálogo o ventana.

El contrato actual sólo admite teclas individuales. Para auditar aplicaciones de escritorio hacen falta modificadores y chords como `Ctrl+Shift+D`, `Alt`, `F10` o `Ctrl+Tab`. También faltan operaciones habituales para QA responsive y de escritorio:

- redimensionar o maximizar/restaurar con dimensiones explícitas;
- desplazar una región;
- hover;
- doble clic;
- menú contextual;
- seleccionar opciones cuando el click genérico no sea suficiente.

Estas capacidades deben seguir siendo pequeñas y concretas; no hace falta convertir Pumarejo en una API genérica de WebDriver.

### 4. Verificar el efecto de una acción, no sólo su despacho

`tauri_click` devolvió éxito al pulsar la pestaña Resumen, pero el panel no cambió: el elemento recibió foco y la navegación sólo funcionó mediante Ver → Abrir resumen.

El resultado de una acción debería distinguir:

- evento despachado;
- elemento enfocado;
- navegación o mutación DOM observada;
- cambio de ventana/panel;
- ausencia de efecto observable.

Una opción como `snapshotAfter: true` podría devolver el foco posterior y un delta semántico mínimo. Eso reduciría round trips, evitaría usar refs caducadas y permitiría afirmar “click enviado, sin cambio observable” en vez de dar un éxito que parece garantizar el resultado funcional.

## Mejoras de prioridad media

### 5. Conservar más estados ARIA en el snapshot

El código del visor de archivos expone `aria-pressed` en los botones Cambios, Archivo completo, En línea y Lado a lado, pero esos estados no aparecieron en el payload semántico observado.

El esquema debería preservar de forma consistente:

- `pressed`;
- `selected`;
- `current`;
- `checked`;
- `expanded`;
- `invalid`;
- `required`;
- relaciones `labelledby`, `describedby`, `controls` y `owns`.

Cuando WebDriver no pueda obtener un estado, conviene marcarlo como `unknown` o indicar la fuente utilizada, en lugar de omitirlo silenciosamente.

### 6. Hacer `doctor` más exacto y accionable

`doctor` informó que `npm` no estaba disponible y que WebView2 no se encontraba, aunque la sesión sí pudo lanzarse al proporcionar el runtime Node correcto y la aplicación funcionó con WebView2.

Sería útil:

- mostrar el `PATH` efectivo y el ejecutable resuelto;
- permitir `launch.env` o una ruta explícita de runtime;
- diferenciar `missing`, `not detected`, `not on PATH` y `verified by successful launch`;
- volver a evaluar heurísticas después de una sesión exitosa;
- incluir el comando exacto que se intentará ejecutar, sin secretos.

Los falsos negativos en `doctor` erosionan confianza precisamente en la herramienta que debería explicar por qué un entorno no arranca.

### 7. Facilitar la conexión desde clientes MCP

El servidor existe y su contrato es claro, pero en esta sesión no estaba registrado como herramienta MCP directa; fue necesario localizar el proyecto e iniciar un cliente SDK con transporte `stdio`.

Pumarejo podría ofrecer:

- `pumarejo mcp print-config --project …` para generar la entrada de clientes conocidos;
- una salida `doctor --json` con comando, argumentos y entorno requerido;
- documentación corta de conexión para Codex y otros hosts MCP;
- una comprobación de versión entre CLI, manifest de integración y plugin Tauri.

Esto no requiere añadir un endpoint de red. `stdio` es una buena decisión para una herramienta local y sensible.

### 8. Mejorar los errores y preservar evidencia parcial

Los errores estructurados como `ELEMENT_NOT_INTERACTABLE` fueron útiles porque incluían fase, posibilidad de reintento y sugerencia. Ese estándar debería cubrir también:

- tamaño máximo de snapshot;
- timeout de compilación/lanzamiento;
- ventana no encontrada;
- ref obsoleta;
- acción sin efecto;
- cleanup parcial.

Cuando haya una captura válida pero falle el árbol semántico, la respuesta debería conservar la captura, generación, ventana, foco y causa del fallo en vez de degradar toda la operación a `INTERNAL_ERROR`.

## Mejoras de ergonomía

### 9. Ofrecer acción y snapshot atómicos

Como toda acción invalida las referencias, el patrón correcto fue siempre:

1. snapshot;
2. acción;
3. snapshot nuevo.

Es seguro, pero costoso. Una variante atómica que ejecute la acción y devuelva el snapshot posterior —o un delta con nuevas refs— reduciría errores y latencia sin relajar el modelo generacional.

### 10. Exponer diagnósticos de sesión compactos

Un comando o recurso de estado podría informar:

- fase actual;
- PID del proceso propiedad de Pumarejo;
- ventana seleccionada;
- puerto WebDriver;
- generación vigente;
- última acción;
- recursos pendientes de cleanup.

Debe ser compacto y no incluir contenido de la aplicación por defecto.

## Qué conviene conservar

- MCP local por `stdio`, sin servidor de red.
- Superficie pequeña de herramientas con esquemas estrictos.
- Sesión y procesos propiedad de Pumarejo.
- Referencias ligadas a una generación y caducidad explícita.
- Capturas en memoria con opción de no retener artefactos.
- Integración Tauri opcional y ausente de builds normales.
- Errores estructurados con fase, `retryable` y sugerencia cuando están disponibles.
- Cierre explícito que, en el camino normal, deja la sesión en `idle`.

## Criterios de aceptación sugeridos

Una siguiente versión quedaría sensiblemente más sólida si demuestra:

1. Un snapshot de una vista con texto masivo devuelve contenido paginado o truncado, nunca un `INTERNAL_ERROR` sin evidencia parcial.
2. Los textos sensibles pueden omitirse o redactarse antes de salir del proceso.
3. Un lanzamiento de más de 30 segundos publica progreso y sobrevive a clientes con timeouts cortos mediante una tarea consultable.
4. Cancelar durante launch/click permite ejecutar `tauri_close` de forma idempotente y deja cero procesos propiedad del driver.
5. `TAB` y un chord con modificadores muestran foco anterior/posterior correcto en una app Tauri.
6. El cliente puede fijar 640×480, 800×600 y 1920×1032 y confirmar el tamaño efectivo.
7. Una acción que sólo enfoca, pero no activa, se reporta como “sin cambio observable”.
8. `pressed`, `selected`, `current` y relaciones ARIA aparecen de forma consistente.
9. `doctor` deja de declarar ausente un requisito que una sesión posterior ha verificado.
10. El flujo acción + snapshot posterior puede ejecutarse atómicamente sin reutilizar refs obsoletas.
