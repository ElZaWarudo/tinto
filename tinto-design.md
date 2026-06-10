# Tinto — Documento de diseño

> Estado: brainstorm cocido. Define el qué y el cómo a nivel arquitectura. No es spec de implementación todavía.

## 1. Qué es Tinto

Tinto es una **aplicación de escritorio de monitoreo** para supervisar, en tiempo real y de forma pasiva, los cambios que ocurren en varios repositorios git locales mientras son editados por **agentes de código** (Claude Code, Codex, OpenCode y similares).

El usuario no edita código en Tinto. Su rol es de **supervisor**: ver qué tocaron los agentes, dónde y cuándo, sin tener que abrir un editor pesado como VS Code para algo que no va a editar a mano.

### Principios de diseño

- **Read-only.** Tinto nunca modifica los repos. No commitea, no stagea, no revierte. Solo observa y presenta.
- **Pasivo.** No interrumpe ni aprueba flujos de trabajo. Muestra señales; el usuario decide qué mirar.
- **Liviano.** Pensado para estar abierto todo el día mirando. Bajo consumo de RAM y CPU.
- **Local.** Solo repos en el sistema de archivos local. Sin remoto, sin nube, sin agentes que reporten.
- **Sin interpretación.** No genera resúmenes en lenguaje natural de los cambios. Muestra los hechos (diffs, eventos), no opiniones sobre ellos.

### Plataformas

- Windows
- Linux

## 2. Concepto central: Workbenches

Un **workbench** es un conjunto nombrado de repositorios que el usuario quiere monitorear en conjunto.

- El usuario puede tener **varias workbenches** configuradas (ej. "Trabajo", "Side projects", "Cliente X") y conmutar entre ellas.
- Un mismo repo puede pertenecer a **más de una** workbench.
- Se agregan proyectos a un workbench de dos formas:
  - Seleccionando una carpeta de repo puntual.
  - Apuntando a una carpeta raíz y **autodetectando** todos los `.git` dentro.
- La configuración (qué repos, alias, orden, agrupación) **persiste** entre sesiones.

## 3. Los dos planos de monitoreo

Tinto observa cada repo en dos planos paralelos según la relación del archivo con git.

### Plano 1 — Git-tracked (con diffs)

Archivos versionados o trackeables por git. Aquí Tinto aprovecha todo git:

- `git status`: modificados / staged / untracked.
- Diffs con syntax highlighting (inline y side-by-side).
- Branch actual, ahead/behind vs remote.
- Historial de commits navegable, con sus diffs.

### Plano 2 — FS-tracked (sin diffs, solo eventos)

Archivos **ignorados por git** pero que igual interesa vigilar (logs, artefactos de build, `.env`, archivos generados). No hay diff de git, pero sí:

- Tipo de evento + timestamp: creado / modificado / borrado.
- Tamaño y delta de tamaño.
- Metadata: extensión, mtime, permisos.

El Plano 2 es **opt-in por patrones**. Sin una lista explícita de qué vigilar, queda vacío (para no barrer carpetas gigantes como `node_modules` o `target`). Con la lista, observa exactamente lo configurado.

> Nota: muchos archivos sensibles (`.env`, secrets) están justamente gitignoreados, por lo que el Plano 2 es donde más valor tienen las alertas de archivos sensibles.

### Clasificación de cada evento de FS

```
evento de FS
   │
   ├─ ¿está en .git/?              → descartar (salvo HEAD / index)
   │
   ├─ ¿git-tracked o trackeable?   → Plano 1: recalcular git status/diff
   │
   ├─ ¿gitignored pero en
   │   watchlist FS-tracked?       → Plano 2: emitir evento de archivo
   │
   └─ resto (gitignored y no
       vigilado)                   → descartar
```

## 4. Funcionalidades

### Dashboard (por workbench)

- Una card por repo: branch actual, conteo de modificados / staged / untracked, ahead-behind vs remote, último commit.
- Indicador de **actividad en vivo**: de un vistazo, qué repo se está moviendo ahora.
- Vista compacta vs. expandida por card.

### Visualización de cambios

- Diff viewer cómodo: syntax highlighting, modos inline y side-by-side.
- **Live diff**: el diff se actualiza solo mientras el agente escribe, vía file watcher.
- Vista de archivo completo con cambios resaltados (no solo el hunk aislado), para tener contexto.
- Sección separada por repo para **archivos vigilados** (Plano 2): lista plana con icono de evento (creado / modificado / borrado), timestamp y tamaño. Sin pretensión de diff.

### Timeline / historial

- Feed cronológico de actividad cruzando todos los repos del workbench (ej. "14:32 — Proyecto X: 3 archivos modificados").
- Navegación por commits con sus diffs, sin terminal.
- Detección de **cambios huérfanos**: working tree sucio hace rato sin commit.

### Señales pasivas

- **Highlights automáticos**: marcar visualmente cambios que conviene mirar — deletes grandes, archivos sensibles (`.env`, CI, configs), posibles secrets, cambios en tests.
- **Métricas livianas**: líneas +/- por repo, archivos tocados por sesión, frecuencia de cambios.

### Calidad de vida

- Notificaciones nativas del SO ante eventos relevantes.
- Filtros y búsqueda por repo, extensión o rango temporal.
- Modo "glance": ventana compacta / item de barra con el estado resumido.

## 5. Stack técnico

| Capa | Elección | Motivo |
|------|----------|--------|
| Shell de app | **Tauri 2** (Rust + webview) | Binario chico, RAM baja, Windows/Linux real |
| Frontend | Svelte o SolidJS (React opcional) | Reactividad eficiente para datos con updates frecuentes |
| Estado git | **git2-rs** (libgit2) | Lee estado sin spawnear procesos git |
| File watching | **notify** | Watcher nativo multiplataforma |
| Clasificación de paths | **ignore** (crate de ripgrep) | Respeta `.gitignore` para clasificar, no solo filtrar |
| Async runtime | **tokio** | Coordina watcher + tareas de git |
| Persistencia config | TOML/JSON vía **dirs** | Config dir del SO; sin DB al inicio |

### Matiz sobre git2-rs

git2-rs es ideal para status, branch, ahead/behind, log y lectura de blobs. Su punto débil conocido es que el cálculo de diffs/status en repos muy grandes (monorepos) puede ser más lento que el binario `git` nativo, que está muy optimizado (índice, fsmonitor).

**Decisión:** git2-rs por defecto, abstraído detrás de un trait, dejando la puerta abierta a *shellear* `git` para operaciones puntuales si aparece lentitud. No es una decisión a cerrar ahora.

## 6. Arquitectura

```
┌─────────────────────────────────────────┐
│              Frontend (webview)          │
│   Workbench UI · Dashboard · Diff viewer │
└───────────────▲──────────────┬───────────┘
                │ events        │ commands (invoke)
                │ (emit)        ▼
┌───────────────┴──────────────────────────┐
│            Tauri Rust backend             │
│                                           │
│  ┌──────────┐  ┌──────────┐  ┌─────────┐ │
│  │ Workbench│  │  Git      │  │ Watcher │ │
│  │  manager │  │  engine   │  │ (notify)│ │
│  │ (config) │  │ (git2-rs) │  │         │ │
│  └──────────┘  └─────▲─────┘  └────┬────┘ │
│                      │             │       │
│              ┌───────┴─────────────▼─────┐ │
│              │   State / Event bus       │ │
│              │  (debounce + diff state)  │ │
│              └───────────────────────────┘ │
└───────────────────────────────────────────┘
```

## 7. Flujo de Live Diff (núcleo)

Es el corazón del sistema. Los agentes escriben en ráfagas (muchos archivos en segundos), así que el flujo debe coalescer y no saturar la UI.

1. `notify` observa los working dirs de los repos del workbench activo.
2. Los eventos de FS entran a un canal y pasan por **debounce** (~200–400 ms) para agrupar ráfagas.
3. Al estabilizarse, se recalcula `git status` **solo del repo afectado** (no de todos).
4. Si cambió el set de archivos o sus diffs, se hace `emit` de un evento al frontend con el delta.
5. El frontend actualiza solo las cards/diffs afectados.

### Detalles que evitan dolores

- **Ignorar `.git/`** en el watcher (ruido infinito), salvo `.git/HEAD` y `.git/index` si se quiere detectar commits/cambios de branch.
- **Respetar `.gitignore`**: no observar `node_modules`, `target`, `dist`. notify no lo hace solo; se filtra con el crate `ignore`.
- **Throttling por repo**: límite de frecuencia de recálculo por repo, porque un agente puede tocar cientos de archivos.
- **Watcher solo del workbench activo** (o de todos, configurable) para no malgastar handles del SO.

## 8. Persistencia

- Config de workbenches en JSON/TOML en el config dir del SO (crate `dirs`).
- Estructura sugerida:

```toml
[[workbench]]
name = "Trabajo"

  [[workbench.repos]]
  path = "/home/yo/proyecto-x"
  alias = "Proyecto X"

    # Plano 2: archivos gitignoreados a vigilar igual (opt-in)
    fs_watch = [".env", "dist/**", "*.log", "build/output.json"]
```

- Sin base de datos al inicio. El estado de monitoreo en vivo vive en memoria.
- Si más adelante se quiere un timeline histórico persistente, SQLite (`rusqlite`) sería el paso siguiente.

## 9. Fuera de alcance (explícito)

- Edición de código o cualquier escritura sobre los repos.
- Operaciones git (commit, stage, branch, merge, revert).
- Aprobación / rechazo de cambios o control de agentes.
- Resúmenes en lenguaje natural de los cambios.
- Repos remotos o agentes que reportan desde otra máquina.
- Integración directa con las CLIs de agentes (Tinto observa el FS, no se acopla a ninguna herramienta).

## 10. Posibles pasos siguientes

- Esqueleto del proyecto Tauri 2.
- Trait de la capa de git (con impl git2-rs y escape hatch a CLI).
- Clasificador de paths (los tres buckets).
- Watcher con debounce y throttling por repo.
- Comando para listar/cargar los repos de un workbench.
