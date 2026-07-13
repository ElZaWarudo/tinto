---
title: Tinto Mobile Companion - nota de viabilidad
status: exploratory
date: 2026-07-10
decision: none
---

# Tinto Mobile Companion - nota de viabilidad

## Propósito y estado

Esta nota conserva una exploración futura sobre dos ideas relacionadas:

1. convertir el frontend de Tinto en una interfaz plenamente adaptable;
2. evaluar una aplicación móvil que se conecte a Tinto Desktop para observar y operar el PC.

No es un roadmap, un diseño técnico ni una decisión arquitectónica. Su objetivo es mantener el razonamiento disponible para cuando exista prioridad de producto para retomarlo.

## Conclusión ejecutiva

Una aplicación móvil compañera es viable y encaja con el producto. La paridad total entre escritorio y teléfono también sería técnicamente posible, pero parece una inversión poco conveniente.

La dirección con mejor relación entre utilidad, riesgo y reutilización sería:

- Tinto Desktop sigue siendo la autoridad sobre repositorios, filesystem, Git, agentes y procesos.
- Tinto Mobile actúa como una ventana autenticada hacia el PC.
- El frontend comparte dominio, contratos, componentes y estado donde resulte útil, pero no intenta reproducir el workspace dockeable completo en un teléfono.

La idea central no sería «Tinto Desktop reducido», sino «un cliente móvil seguro para el Tinto que sigue ejecutándose en el PC».

## Viabilidad de plataforma

Tauri 2 soporta Windows, Linux, macOS, Android e iOS desde una misma base de frontend. React, TypeScript y Vite pueden reutilizarse. Android puede desarrollarse desde Windows; iOS requiere macOS y Xcode.

La compatibilidad de la aplicación no queda garantizada únicamente por compilar el frontend. Los plugins y capacidades nativas deben revisarse por plataforma; algunas funciones móviles pueden necesitar implementaciones específicas en Kotlin o Swift.

Referencias oficiales:

- [Tauri 2.0](https://v2.tauri.app/)
- [Prerequisites and mobile targets](https://v2.tauri.app/start/prerequisites/)
- [Mobile plugin development](https://v2.tauri.app/develop/plugins/develop-mobile/)
- [Official features and platform support](https://v2.tauri.app/plugin/)

## Qué significa realmente hacer el frontend adaptable

«100 % responsive» no debería significar comprimir Dockview hasta que quepa en una pantalla estrecha.

El frontend actual está organizado alrededor de paneles simultáneos, docking, exploradores, terminales, diffs y rails de inspección. Esas capacidades dependen del espacio disponible y de la precisión del puntero. En móvil, conservar literalmente la misma composición produciría una interfaz técnicamente adaptable pero difícil de usar.

La adaptación útil debería permitir:

- compartir componentes, contratos, stores, filtros e indicadores;
- mantener splits y docking principalmente en escritorio y, cuando tenga sentido, en tablet;
- presentar una superficie principal por vez en teléfono;
- reemplazar columnas comprimidas por navegación progresiva entre lista, detalle, archivo y conversación;
- considerar touch targets, teclado virtual, scroll, suspensión y reconexión como requisitos propios;
- responder al tamaño real del panel o contenedor, no solo al viewport global.

Esta inversión también mejoraría la aplicación de escritorio: Tinto ya permite redimensionar y dividir paneles, por lo que un componente adaptable es valioso aunque no exista todavía una distribución móvil.

## Capacidades que deben permanecer en el PC

El backend actual depende de recursos propios del host:

- repositorios y filesystem locales;
- Git y watchers;
- WSL;
- PTYs, terminales y procesos de agentes;
- ventanas separadas;
- operaciones de archivos y diálogos del sistema.

En un dispositivo móvil esas capacidades actuarían sobre el teléfono, no sobre el PC. Por ello, el acceso móvil requiere que Tinto Desktop sea también un host remoto. El móvil no debería acceder directamente al disco ni intentar reproducir el backend.

## Base reutilizable existente

El bus ya maneja snapshots, eventos, deltas y revisiones monotónicas. Esa semántica es una buena base para reconexión y sincronización remota:

- contrato: `docs/contracts/bus-contract.md`;
- store y reglas de revisión: `src/bus/store.ts`;
- conexión actualmente acoplada a IPC/eventos Tauri: `src/bus/connection.ts`;
- entrada Tauri preparada para targets móviles: `src-tauri/src/lib.rs`.

Una futura evolución necesitaría separar el dominio del transporte. El frontend debería consumir una interfaz estable de estado y comandos, con al menos dos adaptadores posibles:

- IPC/eventos Tauri para el cliente desktop;
- protocolo remoto autenticado para el cliente móvil.

Esto es una dirección de exploración, no una API decidida.

## Modelo mental de conexión

```text
Móvil -> intención autenticada -> Tinto Desktop -> filesystem / Git / agentes
             ^                         |
             |                         v
       confirmación <- estado, eventos y resultados
```

El PC conserva la fuente de verdad y ejecuta todas las operaciones. El móvil envía intenciones de alto nivel y recibe confirmaciones explícitas; no recibe acceso general a una shell o al filesystem.

## Opciones de conectividad

| Opción | Ventajas | Costes y riesgos |
| --- | --- | --- |
| Red local con emparejamiento | Privacidad, sin infraestructura central, buen punto de partida | Descubrimiento, firewalls, certificados, ambos dispositivos en la misma red |
| Relay remoto | Acceso desde cualquier lugar, notificaciones y conectividad más predecible | Infraestructura, coste, identidad, confianza y una superficie de seguridad mayor |
| VPN personal o Tailscale | Permite validar el concepto rápidamente sin construir un relay | Configuración adicional y dependencia de una herramienta externa |

La inclinación inicial sería validar primero una conexión local con emparejamiento explícito. Un relay propio solo tendría sentido después de demostrar que el acceso fuera de la red local aporta suficiente valor.

WebRTC no parece una primera opción atractiva: reduce algunos problemas de conexión directa, pero introduce señalización, TURN y más complejidad operativa.

## Trabajos móviles con mejor encaje

### Buenos candidatos iniciales

- Ver agentes activos, esperando, bloqueados o terminados.
- Recibir señales críticas y notificaciones.
- Consultar el estado de repositorios y workbenches.
- Inspeccionar Timeline, cambios, diffs y conversaciones.
- Enviar una instrucción a una sesión ya existente.
- Detener una sesión con confirmación.
- Inspeccionar restore points antes de actuar.

### Candidatos que conviene aplazar

- Terminal interactiva completa.
- Operaciones arbitrarias de archivos.
- Drag and drop.
- Navegación intensiva por árboles grandes.
- Diffs side-by-side.
- Configuración avanzada de workbenches.
- Reproducción del docking de escritorio.

No son imposibles, pero elevan la complejidad y el riesgo sin ser necesariamente los trabajos más valiosos en movilidad.

## Seguridad y continuidad

La dificultad principal no sería CSS, sino mantener confianza cuando el teléfono controla otro dispositivo.

Una exploración seria deberá cubrir como mínimo:

- emparejamiento y revocación de dispositivos;
- cifrado del transporte y almacenamiento seguro de credenciales;
- permisos por dispositivo y por tipo de acción;
- ausencia de shell arbitraria por defecto;
- confirmación proporcional para operaciones destructivas;
- identidad del dispositivo que originó cada acción y registro auditable;
- identificadores idempotentes para evitar repetir comandos tras una reconexión;
- snapshots de reanudación y recuperación de eventos perdidos;
- estado explícito de PC conectado, desconectado, bloqueado o fuera de fecha;
- caché offline de solo lectura;
- prohibición de encolar acciones destructivas mientras el PC está desconectado.

Las aplicaciones móviles se suspenden y las conexiones persistentes no son confiables. El modelo debe asumir reconexión frecuente y probablemente usar notificaciones nativas para avisos importantes.

## Riesgos de producto

- Confundir «responsive» con paridad funcional puede producir una aplicación difícil de operar.
- Exponer demasiadas acciones remotas pronto aumenta de forma desproporcionada el riesgo de seguridad.
- Un protocolo remoto puede convertirse en una segunda API pública accidental si no se define una frontera clara.
- La experiencia móvil puede divergir del desktop si se comparten componentes sin compartir contratos de comportamiento.
- El acceso remoto mediante relay es prácticamente una línea de producto propia, con operación e identidad continuas.

## Secuencia razonable si se retoma

Esta secuencia no constituye un plan comprometido; solo conserva dependencias lógicas:

1. Resolver la deuda responsive del frontend actual basándose en paneles y contenedores.
2. Separar los contratos de dominio del transporte Tauri.
3. Probar un cliente móvil principalmente de lectura en red local.
4. Añadir pocas acciones remotas, explícitas y auditables.
5. Evaluar acceso fuera de la red local solo con evidencia de uso.
6. Decidir entonces si Tauri móvil sigue siendo la mejor envoltura o si conviene otra superficie nativa.

## Preguntas abiertas

- ¿El primer usuario objetivo quiere observar, intervenir o ambas cosas?
- ¿El acceso móvil debe funcionar solo en la misma red o también desde Internet?
- ¿Qué acciones son suficientemente seguras para permitirlas remotamente?
- ¿Debe el desktop estar abierto, ejecutarse en segundo plano o instalar un servicio residente?
- ¿Cómo se presenta la presencia de varios PCs y workbenches?
- ¿Qué estado puede conservarse en el móvil sin exponer información sensible del repositorio?
- ¿Tablet y teléfono comparten el mismo alcance o requieren estrategias distintas?
- ¿Qué plugins actuales de Tauri tienen soporte móvil suficiente y cuáles necesitan adaptación?
- ¿Cuál es la política de actualización de protocolo entre clientes móviles y desktop de versiones distintas?

## Cuándo actualizar esta nota

Revisar este documento cuando ocurra cualquiera de estos eventos:

- se priorice oficialmente una aplicación móvil;
- se introduzca una frontera de transporte distinta de IPC Tauri;
- el frontend complete una adaptación por contenedor significativa;
- se valide un prototipo de emparejamiento local;
- se tome una decisión sobre relay, identidad o servicio residente.

Cuando exista una decisión de producto, esta nota debería convertirse en requisitos y, si corresponde, en un ADR. No debe tratarse como fuente de verdad arquitectónica mientras conserve el estado `exploratory`.
