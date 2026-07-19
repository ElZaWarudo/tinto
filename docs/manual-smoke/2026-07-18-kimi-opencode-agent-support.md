---
title: Kimi Code and OpenCode ACP probe and platform evidence
status: recorded-with-limitations
date: 2026-07-18
roadmap_item: RDM-022
requirements: R18, R21, KTD10
---

# Kimi Code and OpenCode ACP probe and platform evidence

## Alcance

Este registro conserva la evidencia del probe acotado de los CLI actuales exigido por R21 y la
matriz de seis celdas exigida por R18. Distingue resultados observados de limitaciones: una celda no
se declara compatible con ACP por extrapolación, y una limitación de plataforma no sustituye las
garantías funcionales ni de reaping de procesos.

Los probes se ejecutaron sin instalar paquetes globalmente, contra un repositorio temporal vacío y
con perfiles temporales sin credenciales suministradas por Tinto. Los paquetes se inspeccionaron
antes de ejecutarlos; no se observaron scripts de instalación en los tarballs usados. Los perfiles y
procesos temporales se limpiaron al terminar.

## Identidad e integridad de los paquetes

Los metadatos siguientes se resolvieron desde el registro oficial de npm antes de ejecutar los
entry points no globales:

| Paquete | Versión | Integridad npm (sha512) | `shasum` npm (sha1) |
| --- | --- | --- | --- |
| `@moonshot-ai/kimi-code` | `0.27.0` | `wLrPTz4OlyqLIMG0FtftyeQj5rohoqhyS3rZdZAeBoDoG/DAgXGxCnzm7SUJ9KW/fYceK5FEWKKZvvW643TygA==` | `7975a56bfd6ce0da084dbc8a89bdedb85cdcb30a` |
| `opencode-ai` | `1.18.3` | `HnItl/+uhSpj7JV9x6ITiE0XFq4b/PKF5OM03TIyiFoFiLw3MQoJOAXZFTEzC7IOgAIYcysRQBBmCmlXILkxww==` | `291f0fc9e32c7c2a44c470588930f665b3811db5` |
| `opencode-windows-x64` | `1.18.3` | `V/Q4VkyZ7TkotLbpTCX/s5wP8MoFj5UnRqBHFJ8WGptb/+oYw36kamUUt1P1wN8iRjmw+8vwYhuYdAe+3xbgpQ==` | `11ef10dd94cf35ab3e63ba86eeffe7767f8bbed5` |

No se usó un instalador persistente ni se reutilizó un perfil personal para estos probes.

## Probe de Kimi Code 0.27.0

### Procedimiento reproducible

1. Crear un directorio raíz temporal y, dentro de él, un repositorio vacío con ruta absoluta.
2. Resolver e inspeccionar `@moonshot-ai/kimi-code@0.27.0` con npm; verificar versión, integridad y
   ausencia de scripts de instalación antes de extraerlo en el perfil temporal.
3. Redirigir al directorio temporal la configuración, caché y datos que use el proceso. No copiar
   credenciales ni iniciar un login desde Tinto.
4. Ejecutar el entry point del paquete como `kimi acp` con stdin/stdout conectados a un harness ACP
   JSON-RPC y stderr capturado sólo en memoria.
5. Enviar `initialize` para ACP v1 y, tras la respuesta, enviar `session/new` con la ruta absoluta
   del repositorio vacío.
6. Aplicar un límite temporal al probe, terminar el árbol del proceso y eliminar el perfil temporal.

### Evidencia observada

- `initialize` completó el handshake ACP con `protocolVersion: 1`.
- El proveedor se identificó como `Kimi Code CLI` versión `0.27.0`.
- La respuesta anunció `loadSession: true`; prompt con imagen y contexto embebido, sin audio; MCP
  HTTP y SSE; y capacidades de listado y reanudación de sesiones.
- `session/new`, usando un `cwd` absoluto válido, respondió exactamente con el código ACP
  `-32000` y el mensaje `Authentication required`.
- No se proporcionaron credenciales al proceso ni Tinto intentó recogerlas o persistirlas.
- El bloqueo de autenticación ocurrió antes de obtener una sesión de proveedor válida. Por ello no
  se ejercitaron prompt, updates, permisos ni cancelación, y este registro no les atribuye un
  resultado positivo.

Conclusión: el CLI actual demuestra compatibilidad de negociación ACP v1, pero este probe sin
credenciales no demuestra una sesión ACP lista ni su comportamiento posterior a `session/new`.

## Probe de OpenCode 1.18.3

### Procedimiento reproducible

1. Crear un directorio raíz temporal y un repositorio vacío con ruta absoluta.
2. Resolver e inspeccionar `opencode-ai@1.18.3` y su binario de plataforma
   `opencode-windows-x64@1.18.3`; verificar versión, integridad y ausencia de scripts de instalación.
3. Generar para este único lanzamiento una contraseña aleatoria de 256 bits y pasarla sólo mediante
   `OPENCODE_SERVER_PASSWORD` en el entorno hijo. No imprimir ni persistir su valor.
4. Ejecutar:

   ```text
   opencode.exe acp --cwd <REPOSITORIO_VACIO_ABSOLUTO> --hostname 127.0.0.1 --port 0 --no-mdns
   ```

5. Enviar `initialize` ACP v1 y luego `session/new` con el `cwd` absoluto.
6. Mientras vive el mismo proceso, inspeccionar sus sockets TCP/UDP y comprobar el endpoint
   interno con credencial ausente, errónea y correcta. Tinto no usa ese endpoint como transporte.
7. Terminar y reapear el proceso, y volver a inspeccionar los sockets para comprobar su limpieza.

### Evidencia observada

- `initialize` completó el handshake ACP v1.
- El proveedor anunció load de sesión, MCP HTTP/SSE, prompt con imagen y contexto, operaciones de
  cierre, fork, listado y reanudación de sesión, y el método de autenticación `opencode-login`.
- `session/new` con un `cwd` absoluto válido no respondió dentro del límite del probe. En
  consecuencia no se ejercitaron updates, permisos ni cancelación y no se les atribuye un éxito.
- El proceso abrió un único listener TCP propiedad del mismo proceso en `127.0.0.1:4096`.
- Una petición sin credencial obtuvo HTTP `401`; una credencial errónea también obtuvo `401`; la
  credencial aleatoria correcta obtuvo HTTP `200`.
- No se observó publicación mDNS ni socket UDP en el puerto `5353`.
- Tras terminar y reapear el proceso, el listener dejó de existir.

### Resultado de KTD10

El lanzamiento forzó loopback, `--port 0`, mDNS desactivado y autenticación efímera, aunque el valor
efectivo observado fue `4096`, no un puerto efímero asignado por el sistema operativo. Las
comprobaciones de loopback, autenticación, ausencia de mDNS, propiedad del proceso y limpieza
resultaron positivas.

Decisión posterior aprobada el 2026-07-19: Tinto confía en haber solicitado `--port 0` y no trata el
puerto efectivo `4096` como un fallo de contención por sí solo. OpenCode puede intentar el handshake
ACP estructurado mediante stdio; Tinto no consume el endpoint HTTP interno. Un fallo real de proceso,
handshake o protocolo previo a sesión continúa entrando en compatibilidad PTY visible y recuperable.
Este probe no demostró una sesión `ACP ready`, porque `session/new` no respondió dentro de su límite.

### Evidencia automatizada de la decisión

- `opencode_descriptor_forces_the_approved_loopback_controls_and_ephemeral_secret` fija loopback,
  `--port 0`, `--no-mdns` y secreto efímero fuera de argv.
- `opencode_attempts_acp_when_launched_with_the_requested_ephemeral_port` demuestra que OpenCode
  intenta ACP con el supervisor común y que un fallo real de arranque degrada a PTY sin aplicar el
  veto estático de `127.0.0.1:4096`.
- Las entradas observadas y `redacted:true` de ambos fixtures omiten `authMethods` en vez de
  inventar o conservar descripciones no válidas. `opencode-login` queda sólo como observación del
  probe, no como payload literal del fixture.

## Cobertura del probe R21

| Área exigida | Kimi Code 0.27.0 | OpenCode 1.18.3 |
| --- | --- | --- |
| Startup | `kimi acp` iniciado desde paquete no global y perfil temporal | `opencode.exe acp` iniciado con el descriptor KTD10 y perfil temporal |
| Handshake | ACP v1 observado | ACP v1 observado |
| Capabilities | Observadas y resumidas arriba | Observadas y resumidas arriba |
| Updates | No alcanzados: `session/new` exigió autenticación | No alcanzados: `session/new` no respondió dentro del límite |
| Permisos | No alcanzados por el mismo bloqueo | No alcanzados por el mismo bloqueo |
| Cancelación | No alcanzada por el mismo bloqueo | No alcanzada por el mismo bloqueo |

El probe acotado de ambos CLI queda registrado con sus bloqueos externos exactos. Esta evidencia no
convierte las operaciones no alcanzadas en comportamiento soportado; su validación corresponde a
fixtures deterministas y a un smoke autenticado posterior cuando exista un perfil autorizado.

## Matriz R18: seis celdas

| Plataforma y proveedor | Evidencia | Prerrequisito para ejecutar la celda | ACP readiness | Compatibilidad PTY | Resultado o limitación explícita |
| --- | --- | --- | --- | --- | --- |
| Windows nativo — Kimi Code | Probe real de `@moonshot-ai/kimi-code@0.27.0` | CLI actual instalado o entry point oficial no global; perfil Kimi autenticado para superar `session/new` | Handshake v1 observado; sesión ready no demostrada por `Authentication required` | No ejecutada en este probe | Limitada por ausencia deliberada de credenciales; no se declaran updates, permisos, cancelación ni reaping de una sesión válida |
| Windows nativo — OpenCode | Probe real de `opencode-ai@1.18.3` y `opencode-windows-x64@1.18.3` | CLI actual y descriptor con loopback, `--port 0`, no mDNS y contraseña efímera | Elegible para ACP bajo la decisión del 2026-07-19; handshake v1 observado, pero sesión ready no demostrada porque `session/new` agotó el límite | Fallback visible solo ante un fallo real previo a sesión; smoke interactivo no ejecutado | Listener `127.0.0.1:4096`, auth, ausencia de mDNS y limpieza observados; el puerto fijo se acepta como riesgo residual explícito |
| Linux nativo — Kimi Code | No ejecutado | CLI Kimi actual disponible en el host Linux y perfil autenticado | No determinada | No determinada | Limitación R18: no hubo runner Linux nativo ni perfil de proveedor; no se extrapola el resultado de Windows |
| Linux nativo — OpenCode | No ejecutado | CLI OpenCode actual disponible en el host Linux y el mismo descriptor seguro | No determinada | Fallback PTY ante un fallo real previo a sesión; smoke no ejecutado | Limitación R18: no hubo runner Linux nativo; listener, auth, mDNS y limpieza deben medirse allí |
| Ubuntu WSL — Kimi Code | No ejecutado | CLI Kimi actual instalado dentro de la distro seleccionada, perfil autenticado y captura/reaping del PID o grupo de procesos de la distro | No soportada hasta demostrar handshake, sesión válida y reaping acotado dentro de WSL | No determinada | Limitación R18: no se ejecutó el CLI en Ubuntu WSL ni se probó limpieza del árbol Linux; R9 no puede darse por satisfecho |
| Ubuntu WSL — OpenCode | No ejecutado | CLI OpenCode actual instalado dentro de la distro, descriptor seguro y captura/reaping del PID o grupo de procesos de la distro | No soportada hasta demostrar reaping acotado WSL | Ruta conservadora PTY; smoke no ejecutado | Limitación R18: no hubo probe en Ubuntu WSL; no se extrapolan listener, auth, mDNS ni limpieza de Windows |

## Estado de los criterios cubiertos por este registro

- **R17:** `kimi-acp-v1.jsonl` y `opencode-acp-v1.jsonl` conservan por separado la negociación
  observada y etiquetan como sintéticos los eventos de update, permiso, cancelación y finalización
  que el probe sin credenciales no alcanzó. Ambos pasan la misma validación contra el esquema ACP.
- **R21:** el probe acotado registra startup, handshake y capabilities de ambos CLI actuales, y
  nombra el bloqueo exacto que impidió alcanzar updates, permisos y cancelación. No presenta esos
  comportamientos como probados.
- **KTD10:** loopback, autenticación, ausencia de mDNS y limpieza se observaron, pero el puerto
  efectivo fijo incumple la condición de puerto efímero. El resultado obligatorio es compatibilidad
  PTY visible, nunca `ACP ready`, para OpenCode 1.18.3.
- **R18:** las seis celdas están inventariadas. Sólo Windows nativo contiene ejecución real; las
  otras cuatro celdas y los comportamientos no alcanzados en Windows conservan limitaciones
  explícitas. Ninguna limitación se usa para declarar soporte estructurado ni para dispensar R9.
