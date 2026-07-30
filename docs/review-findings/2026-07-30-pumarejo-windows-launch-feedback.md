# Feedback para Pumarejo: lanzamiento de aplicaciones Tauri en Windows restringido

**Fecha:** 30 de julio de 2026  
**Componente afectado:** gestión de procesos en Windows  
**Severidad propuesta:** alta  
**Caso probado:** abrir una segunda instancia de Tinto y controlarla mediante Pumarejo

## Resumen

Pumarejo no pudo completar el lanzamiento controlado de Tinto en un entorno Windows donde WMI/CIM está disponible, pero el proceso agente no tiene permiso para consultar `Win32_Process`.

El bloqueo principal está en la verificación de propiedad del proceso. La implementación consulta `Get-CimInstance Win32_Process` para inspeccionar el proceso iniciado y determinar qué proceso posee el puerto del proveedor. Cuando Windows responde `Acceso denegado`, Pumarejo no puede acreditar la propiedad de la instancia y aborta el flujo.

Fallar de forma cerrada es una decisión de seguridad razonable. El problema es que el usuario no recibe un diagnóstico que identifique esa restricción ni una vía clara para resolverla. Durante la prueba, errores distintos —directorio de artefactos inaccesible, incompatibilidad del comando de lanzamiento y denegación de CIM— terminaron manifestándose como un fallo genérico relacionado con la captura de pantalla. Esto hizo que la investigación se concentrara inicialmente en la aplicación bajo prueba, aunque el bloqueo estaba en el arnés.

## Entorno de la prueba

- Windows.
- Pumarejo ejecutado como servidor MCP sobre `stdio`.
- Aplicación Tauri con una ventana llamada `main`.
- Lanzamiento en modo de desarrollo con la feature `pumarejo`.
- Shell PowerShell.
- Entorno con ejecución de procesos permitida, pero sin permiso para consultar `Win32_Process` mediante CIM.

La consulta que reproduce la restricción es:

```powershell
Get-CimInstance Win32_Process -Filter "ProcessId = <pid>"
```

Resultado:

```text
Acceso denegado
```

## Resultado esperado

Pumarejo debería hacer una de estas dos cosas:

1. iniciar la aplicación, comprobar de forma segura que el proveedor pertenece al árbol de procesos lanzado y continuar con la automatización; o
2. detener el proceso y devolver un error específico que explique que Windows impidió verificar su propiedad.

En el segundo caso, el mensaje debería indicar:

- qué comprobación falló;
- si el proceso hijo llegó a iniciarse;
- si fue terminado durante la limpieza;
- qué permiso o política del host debe revisarse;
- que no se trata de un fallo de WebDriver ni de la captura de pantalla.

## Resultado observado

La aplicación llegó a iniciarse durante algunas variantes de la prueba, pero Pumarejo no pudo completar la comprobación de propiedad. El flujo terminó sin una sesión controlable y el error visible no conservó la causa de bajo nivel.

La dependencia de CIM aparece en `src/platform/windows/process.ts`:

- `inspectSystem(pid)` consulta un proceso concreto con `Get-CimInstance Win32_Process`;
- `providerOwner(rootPid, providerPort)` enumera los procesos mediante la misma interfaz para reconstruir la relación padre-hijo;
- un error de acceso impide distinguir con claridad entre “el proceso no existe” y “el proceso existe, pero no se puede inspeccionar”.

## Hallazgos

### 1. La inspección de procesos no distingue ausencia, denegación y fallo operativo

La API interna devuelve un valor ausente cuando no logra inspeccionar el proceso. Esa representación pierde una diferencia importante:

- el PID ya no existe;
- CIM no está disponible;
- CIM respondió `AccessDenied`;
- la consulta agotó el tiempo;
- la salida no pudo interpretarse.

Estas situaciones exigen respuestas distintas. Tratar una denegación como si el proceso no existiera dificulta el diagnóstico y puede provocar decisiones incorrectas durante la limpieza o la validación de propiedad.

### 2. La verificación depende de una interfaz que suele estar restringida

`Get-CimInstance Win32_Process` puede estar bloqueado en sandboxes, escritorios corporativos, runners de CI y sesiones con políticas endurecidas. En esos entornos, Pumarejo puede crear el proceso, pero no demostrar después que le pertenece.

No se propone omitir la verificación. La mejora consiste en conservar evidencia de propiedad desde el momento del `spawn` y usar CIM como una fuente adicional, no como la única base del flujo en Windows.

### 3. El error final oculta la fase que falló

Un fallo de preparación o lanzamiento puede terminar presentado como `SCREENSHOT_FAILED`. Esa clasificación orienta la investigación hacia WebDriver, la ventana o el renderizado, aunque todavía no exista una sesión válida sobre la que tomar una captura.

El error público debería conservar la etapa:

```text
configuration
artifacts
launch
process-inspection
provider-discovery
webdriver-connection
window-selection
screenshot
```

### 4. El comando `npm` configurado no fue portable en Windows

El perfil original usaba:

```json
{
  "launch": {
    "command": "npm",
    "args": [
      "run",
      "tauri",
      "--",
      "dev",
      "--features",
      "pumarejo",
      "--config",
      "{tauriConfig}"
    ]
  }
}
```

El mismo comando funcionaba desde la shell, pero el resolvedor de Pumarejo no pudo interpretar el `npm.cmd` moderno de la instalación probada. Fue necesario apuntar a un ejecutable real y ejecutar directamente el CLI de Tauri.

Conviene probar el resolvedor de shims contra las variantes actuales de `npm.cmd`, incluidas las que usan variables y rutas indirectas.

### 5. La validación de prerrequisitos debería ocurrir antes del lanzamiento

También se detectaron dos incompatibilidades antes de llegar al bloqueo principal:

- el directorio configurado para artefactos no era escribible;
- una capability creada para una versión anterior no coincidía con el identificador y los permisos que esperaba la versión actual.

Ambas condiciones pueden validarse antes de crear procesos. Un chequeo previo habría reducido la prueba a un único error accionable.

## Propuesta de mejora

### Cambio mínimo recomendado

1. Sustituir el retorno opcional de la inspección de procesos por un resultado tipado:

   ```text
   found | not-found | access-denied | unavailable | timed-out | invalid-response
   ```

2. Mantener el comportamiento fail-closed cuando no se pueda demostrar la propiedad.
3. Devolver un error público `PROCESS_INSPECTION_DENIED` cuando CIM responda `AccessDenied`.
4. Incluir la causa original y la fase del flujo en el error MCP.
5. Validar, antes del `spawn`, que:

   - el directorio de artefactos existe o puede crearse;
   - puede escribirse un archivo temporal en él;
   - la capability requerida está instalada y asociada a la ventana;
   - el comando de lanzamiento se resuelve a un ejecutable aceptado.

### Mejora posterior

Registrar el PID y el identificador del proceso devueltos por el `spawn`, conservar el handle mientras viva la sesión y contrastar esa información con el propietario del puerto. Si CIM no está disponible, Pumarejo puede intentar otra fuente nativa de información; si ninguna permite demostrar la relación, debe abortar con el error específico anterior.

La alternativa no debería relajar la garantía de propiedad ni aceptar cualquier proceso que escuche en el puerto esperado.

## Criterios de aceptación

- Si CIM responde `AccessDenied`, Pumarejo devuelve `PROCESS_INSPECTION_DENIED` y no `SCREENSHOT_FAILED`.
- El error indica que la aplicación pudo haberse iniciado y confirma si Pumarejo la terminó.
- Un PID inexistente se informa como `PROCESS_NOT_FOUND`, no como una denegación.
- Un directorio de artefactos sin permisos falla antes de lanzar la aplicación con `ARTIFACTS_DIRECTORY_NOT_WRITABLE`.
- Una capability incompatible falla antes del lanzamiento e identifica el archivo y los permisos esperados.
- El resolvedor de Windows soporta el `npm.cmd` generado por versiones actuales de npm.
- Ningún fallback permite conectarse a un proveedor que no pueda vincularse con el árbol de procesos creado por Pumarejo.

## Pruebas sugeridas

1. Simular `Get-CimInstance` con salida válida para un proceso hijo.
2. Simular `AccessDenied`, timeout, PID inexistente y JSON inválido.
3. Comprobar que cada caso produce un código distinto y conserva la causa.
4. Iniciar un proceso legítimo y otro ajeno que escuche en el puerto esperado; Pumarejo debe rechazar el segundo.
5. Ejecutar el flujo con un directorio de artefactos de solo lectura.
6. Probar fixtures de `npm.cmd` de distintas instalaciones soportadas.
7. Probar una capability antigua y verificar que el preflight explica cómo actualizarla.
8. Confirmar que todos los procesos creados durante un fallo quedan terminados o se reportan explícitamente como supervivientes.

## Impacto

El problema impide usar Pumarejo en entornos Windows restringidos aun cuando la aplicación Tauri y WebDriver estén correctamente configurados. Además, la pérdida de causalidad aumenta mucho el tiempo de diagnóstico: una restricción de CIM parece un problema de captura, una ruta no escribible parece un fallo de la aplicación y un shim incompatible obliga a experimentar con gestores de paquetes.

La prioridad no debería ser permitir el lanzamiento a cualquier precio. La mejora más valiosa es que Pumarejo falle en la fase correcta, preserve la causa y diga con precisión qué necesita del host.
