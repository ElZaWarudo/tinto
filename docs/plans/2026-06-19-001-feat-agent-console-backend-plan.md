---
title: feat(agent-console): backend PTY runtime + agent process lifecycle
plan_id: 2026-06-19-001-feat-agent-console-backend-plan
created: 2026-06-19
updated: 2026-06-19
status: draft
roadmap_item: ACI-001
depends_on: []
blocks: [ACI-002, ACI-003]
---

# ACI-001: Backend PTY Runtime + Agent Process Lifecycle

## Summary

Implementar el backend Rust para gestión de sesiones de agentes de codificación con terminales PTY. Esto incluye:
- Integración de `portable-pty` para terminales interactivas
- Módulo `agent_console` con registro de sesiones
- Comandos Tauri: `start_agent_session`, `stop_agent_session`, `list_agent_sessions`
- Extensión del contrato bus con tipos de sesión
- Validación de binarios (allowlist: claude, codex, opencode)
- Limpieza de procesos multiplataforma (Unix/Windows/WSL)

## Problem Frame

Tinto necesita lanzar agentes de codificación (Claude Code, Codex, OpenCode) en terminales interactivas para que el usuario pueda interactuar con ellos. El backend debe:
1. Gestionar el ciclo de vida de procesos PTY
2. Validar que los binarios existan antes de lanzar
3. Limpiar procesos correctamente al detener sesiones o cerrar la app
4. Exponer comandos para que el frontend pueda controlar las sesiones

## Requirements

- **R1**: Lanzar sesiones de agentes con PTY interactivo en el directorio del repo
- **R2**: Validar existencia de binarios antes de lanzar (allowlist: claude, codex, opencode)
- **R3**: Exponer comandos Tauri: start, stop, list
- **R4**: Extender contrato bus con tipos de sesión (AgentSession, AgentSessionStatus, AgentSessionError)
- **R5**: Limpieza de procesos multiplataforma (kill tree, no zombies)
- **R6**: Eventos de output para streaming al frontend (ACI-002)

## Key Technical Decisions

### KTD1: portable-pty sobre std::process::Command
**Decisión**: Usar `portable-pty` crate en lugar de `std::process::Command` con pipes manuales.
**Rationale**: portable-pty proporciona terminales PTY reales (no solo pipes), necesario para agentes que esperan un TTY. Soporta resize, input/output bidireccional, y limpieza automática.

### KTD2: Allowlist hardcoded de binarios
**Decisión**: Hardcodear allowlist de binarios (claude, codex, opencode) en lugar de configuración dinámica.
**Rationale**: Seguridad y simplicidad. El usuario no debe poder lanzar binarios arbitrarios. Nuevos agentes requieren cambios en el código.

### KTD3: Session registry in-memory con cleanup explícito
**Decisión**: Mantener registro de sesiones en memoria (HashMap) con cleanup explícito en stop/shutdown.
**Rationale**: Las sesiones son efímeras (duran mientras el usuario interactúa). No hay necesidad de persistencia. Cleanup explícito evita procesos zombie.

### KTD4: Process tree kill multiplataforma
**Decisión**: Implementar process tree kill usando APIs nativas (killpg en Unix, TerminateProcess + enumeración en Windows).
**Rationale**: Los agentes pueden spawnar subprocess. Solo matar el proceso padre dejaría zombies. WSL requiere consideraciones especiales.

## Scope

### In Scope
- Módulo `src-tauri/src/agent_console/` con:
  - `mod.rs`: registro de sesiones
  - `session.rs`: estructura AgentSession
  - `pty.rs`: wrapper de portable-pty
  - `validation.rs`: validación de binarios
- Extensión de contrato en `src-tauri/src/bus/contract.rs`:
  - `AgentSession` struct
  - `AgentSessionStatus` enum
  - `AgentSessionError` struct
  - `EVENT_AGENT_SESSION_OUTPUT` constante
- Comandos Tauri en `src-tauri/src/agent_console/commands.rs`:
  - `start_agent_session`
  - `stop_agent_session`
  - `list_agent_sessions`
- Tests unitarios y de integración
- Documentación de contrato en `docs/contracts/bus-contract.md`

### Out of Scope
- Streaming de output al frontend (ACI-002)
- UI de lanzamiento (ACI-003)
- Checkpoints y revert (ACI-004)
- Auto-splitting (ACI-005)
- Límites de recursos (ACI-006)

## Implementation Units

### U1: Añadir dependencia portable-pty

**Files**:
- `src-tauri/Cargo.toml`

**Approach**:
Añadir `portable-pty = "0.8"` a `[dependencies]`. Esta crate proporciona abstracción multiplataforma de PTY.

**Test scenarios**:
- `cargo build` compila sin errores

**Verification**:
- `cargo check` pasa
- `cargo test` pasa (sin tests nuevos aún)

---

### U2: Definir tipos de contrato para sesiones

**Files**:
- `src-tauri/src/bus/contract.rs`
- `docs/contracts/bus-contract.md`

**Approach**:
Añadir al contrato:
```rust
pub const EVENT_AGENT_SESSION_OUTPUT: &str = "tinto://agent-session-output";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionStatus {
    Starting,
    Running,
    Exited,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentSession {
    pub id: String,
    pub repo: PathBuf,
    pub agent_type: String,
    pub status: AgentSessionStatus,
    pub pid: Option<u32>,
    pub started_at_ms: u64,
    pub exit_code: Option<i32>,
    pub error: Option<AgentSessionError>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentSessionError {
    pub category: String,
    pub message: String,
}
```

Documentar en `docs/contracts/bus-contract.md` la nueva sección "Agent Console Contract".

**Test scenarios**:
- `AgentSession` serializa a JSON con el shape esperado
- `AgentSessionStatus` usa snake_case en JSON
- `AgentSessionError` incluye category y message

**Verification**:
- Tests de serialización pasan
- Documento de contrato está actualizado

---

### U3: Implementar módulo agent_console::session

**Files**:
- `src-tauri/src/agent_console/mod.rs`
- `src-tauri/src/agent_console/session.rs`

**Approach**:
Crear estructura `AgentSession` interna (separada del tipo de contrato) que maneja:
- ID único (UUID)
- PTY handle (portable-pty)
- Estado (starting/running/exited/error)
- Metadata (repo, agent_type, pid, timestamps)

Métodos:
- `new(id, repo, agent_type) -> Self`
- `start(pty_handle) -> Result<(), AgentSessionError>`
- `stop() -> Result<(), AgentSessionError>`
- `status() -> AgentSessionStatus`
- `to_contract() -> AgentSession` (convierte a tipo de contrato)

**Test scenarios**:
- Crear sesión genera ID único
- Transición de estados: starting -> running -> exited
- stop() en sesión running la marca como exited
- stop() en sesión ya exited no falla

**Verification**:
- Tests unitarios pasan
- Transiciones de estado son correctas

---

### U4: Implementar validación de binarios

**Files**:
- `src-tauri/src/agent_console/validation.rs`

**Approach**:
Allowlist hardcoded:
```rust
const ALLOWED_AGENTS: &[&str] = &["claude", "codex", "opencode"];

pub fn validate_agent_binary(agent_type: &str) -> Result<PathBuf, AgentValidationError> {
    // 1. Verificar que agent_type está en allowlist
    if !ALLOWED_AGENTS.contains(&agent_type) {
        return Err(AgentValidationError::UnsupportedAgent(agent_type.to_string()));
    }
    
    // 2. Buscar binario en PATH (no shell expansion, no aliases)
    let binary_path = which::which(agent_type)
        .map_err(|_| AgentValidationError::BinaryNotFound(agent_type.to_string()))?;
    
    // 3. Verificar que es ejecutable
    if !binary_path.is_file() {
        return Err(AgentValidationError::BinaryNotFound(agent_type.to_string()));
    }
    
    Ok(binary_path)
}
```

Añadir `which = "6.0"` a Cargo.toml para búsqueda en PATH.

**Test scenarios**:
- validate_agent_binary("claude") con claude en PATH retorna Ok(path)
- validate_agent_binary("claude") sin claude en PATH retorna Err(BinaryNotFound)
- validate_agent_binary("unknown") retorna Err(UnsupportedAgent)
- validate_agent_binary("rm") retorna Err(UnsupportedAgent) (no está en allowlist)

**Verification**:
- Tests unitarios pasan
- Allowlist funciona correctamente

---

### U5: Implementar PTY wrapper con portable-pty

**Files**:
- `src-tauri/src/agent_console/pty.rs`

**Approach**:
Wrapper sobre portable-pty que:
1. Crea CommandBuilder con binary_path y working_directory (repo)
2. Configura environment (sanitizado, sin secrets)
3. Spawnea proceso PTY
4. Retorna handle para lectura/escritura/resize

```rust
pub struct PtyHandle {
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    process: Box<dyn portable_pty::Child + Send>,
}

impl PtyHandle {
    pub fn spawn(binary_path: &Path, working_dir: &Path) -> Result<Self, PtyError> {
        let cmd = CommandBuilder::new(binary_path);
        cmd.cwd(working_dir);
        // Sanitizar environment
        cmd.env_clear();
        cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
        cmd.env("HOME", std::env::var("HOME").unwrap_or_default());
        
        let pair = PtyPair::new(size)?;
        let child = pair.slave.spawn_command(cmd)?;
        
        Ok(Self {
            reader: pair.master.try_clone_reader()?,
            writer: pair.master.take_writer()?,
            process: child,
        })
    }
    
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), PtyError> {
        self.process.resize(PtySize { rows, cols, .. })?;
        Ok(())
    }
    
    pub fn kill(&mut self) -> Result<(), PtyError> {
        self.process.kill()?;
        Ok(())
    }
}
```

**Test scenarios**:
- spawn() con binary_path válido crea PTY
- spawn() con binary_path inválido retorna error
- resize() cambia tamaño de terminal
- kill() termina proceso

**Verification**:
- Tests unitarios pasan (mockeando portable-pty si es necesario)
- PTY se crea correctamente

---

### U6: Implementar registro de sesiones

**Files**:
- `src-tauri/src/agent_console/mod.rs`

**Approach**:
Estructura `AgentSessionRegistry` que mantiene HashMap de sesiones activas:

```rust
pub struct AgentSessionRegistry {
    sessions: HashMap<String, AgentSession>,
}

impl AgentSessionRegistry {
    pub fn new() -> Self {
        Self { sessions: HashMap::new() }
    }
    
    pub fn start_session(&mut self, repo: PathBuf, agent_type: String) -> Result<String, AgentSessionError> {
        // 1. Validar binario
        let binary_path = validate_agent_binary(&agent_type)?;
        
        // 2. Crear sesión
        let id = Uuid::new_v4().to_string();
        let mut session = AgentSession::new(id.clone(), repo, agent_type);
        
        // 3. Spawear PTY
        let pty = PtyHandle::spawn(&binary_path, &session.repo)?;
        session.start(pty)?;
        
        // 4. Registrar
        self.sessions.insert(id.clone(), session);
        
        Ok(id)
    }
    
    pub fn stop_session(&mut self, session_id: &str) -> Result<(), AgentSessionError> {
        let session = self.sessions.get_mut(session_id)
            .ok_or_else(|| AgentSessionError::not_found(session_id))?;
        
        session.stop()?;
        Ok(())
    }
    
    pub fn list_sessions(&self) -> Vec<AgentSession> {
        self.sessions.values().map(|s| s.to_contract()).collect()
    }
    
    pub fn cleanup_all(&mut self) {
        for session in self.sessions.values_mut() {
            let _ = session.stop();
        }
        self.sessions.clear();
    }
}
```

Añadir `uuid = "1.0"` a Cargo.toml para generación de IDs.

**Test scenarios**:
- start_session() con repo válido y agent_type válido retorna session_id
- start_session() con agent_type inválido retorna error
- stop_session() con session_id válido detiene sesión
- stop_session() con session_id inexistente retorna error
- list_sessions() retorna todas las sesiones
- cleanup_all() detiene todas las sesiones

**Verification**:
- Tests unitarios pasan
- Registro funciona correctamente

---

### U7: Implementar comandos Tauri

**Files**:
- `src-tauri/src/agent_console/commands.rs`

**Approach**:
Comandos Tauri que exponen el registro:

```rust
#[tauri::command]
pub async fn start_agent_session(
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    repo: PathBuf,
    agent_type: String,
) -> Result<String, CommandError> {
    let mut registry = registry.lock().await;
    registry.start_session(repo, agent_type)
        .map_err(|e| CommandError::new("agent_session_error", e.message))
}

#[tauri::command]
pub async fn stop_agent_session(
    registry: State<'_, Mutex<AgentSessionRegistry>>,
    session_id: String,
) -> Result<(), CommandError> {
    let mut registry = registry.lock().await;
    registry.stop_session(&session_id)
        .map_err(|e| CommandError::new("agent_session_error", e.message))
}

#[tauri::command]
pub async fn list_agent_sessions(
    registry: State<'_, Mutex<AgentSessionRegistry>>,
) -> Result<Vec<AgentSession>, CommandError> {
    let registry = registry.lock().await;
    Ok(registry.list_sessions())
}
```

**Test scenarios**:
- start_agent_session con parámetros válidos retorna session_id
- stop_agent_session con session_id válido detiene sesión
- list_agent_sessions retorna lista de sesiones

**Verification**:
- Tests unitarios pasan
- Comandos son invocables desde frontend

---

### U8: Integrar registro en Tauri app

**Files**:
- `src-tauri/src/lib.rs`

**Approach**:
1. Crear registro en `run()`:
```rust
let agent_registry = AgentSessionRegistry::new();
```

2. Gestionar registro con Tauri:
```rust
.manage(Mutex::new(agent_registry))
```

3. Registrar comandos:
```rust
.invoke_handler(tauri::generate_handler![
    // ... comandos existentes
    agent_console::commands::start_agent_session,
    agent_console::commands::stop_agent_session,
    agent_console::commands::list_agent_sessions,
])
```

4. Cleanup en shutdown:
```rust
.run(|app_handle, event| {
    if let tauri::RunEvent::Exit = event {
        // Cleanup de sesiones
        let registry = app_handle.state::<Mutex<AgentSessionRegistry>>();
        let mut registry = registry.lock().unwrap();
        registry.cleanup_all();
    }
})
```

**Test scenarios**:
- App arranca sin errores
- Comandos son accesibles
- Shutdown limpia sesiones

**Verification**:
- `cargo build` pasa
- `cargo test` pasa
- App arranca y cierra correctamente

---

### U9: Implementar process tree kill multiplataforma

**Files**:
- `src-tauri/src/agent_console/pty.rs`

**Approach**:
Extender PtyHandle con process tree kill:

**Unix/Linux/macOS**:
```rust
pub fn kill_tree(&mut self) -> Result<(), PtyError> {
    if let Some(pid) = self.process.process_id() {
        // Matar grupo de proceso
        unsafe {
            libc::killpg(pid as i32, libc::SIGTERM);
        }
        // Esperar un poco
        std::thread::sleep(Duration::from_millis(100));
        // Forzar si es necesario
        unsafe {
            libc::killpg(pid as i32, libc::SIGKILL);
        }
    }
    self.process.kill()?;
    Ok(())
}
```

**Windows**:
```rust
pub fn kill_tree(&mut self) -> Result<(), PtyError> {
    if let Some(pid) = self.process.process_id() {
        // Usar taskkill para matar árbol de procesos
        std::process::Command::new("taskkill")
            .args(&["/F", "/T", "/PID", &pid.to_string()])
            .output()?;
    }
    self.process.kill()?;
    Ok(())
}
```

**WSL**:
- Detectar si estamos en WSL (verificar `/proc/version` contiene "Microsoft")
- Usar killpg como en Unix (WSL soporta señales POSIX)

**Test scenarios**:
- kill_tree() en Unix mata proceso padre e hijos
- kill_tree() en Windows mata proceso padre e hijos
- kill_tree() en WSL funciona como Unix

**Verification**:
- Tests unitarios pasan (mockeando si es necesario)
- Process tree kill funciona en cada plataforma

---

### U10: Tests de integración end-to-end

**Files**:
- `src-tauri/src/agent_console/tests.rs`

**Approach**:
Tests de integración que:
1. Lanzan sesión con `echo "hello"` (simulando agente)
2. Verifican que session_id es retornado
3. Verifican que sesión aparece en list_sessions()
4. Detienen sesión con stop_session()
4. Verifican que sesión ya no está running

```rust
#[tokio::test]
async fn test_agent_session_lifecycle() {
    let registry = AgentSessionRegistry::new();
    
    // Start session
    let session_id = registry.start_session(
        PathBuf::from("/tmp/test-repo"),
        "echo".to_string(), // Usar echo para testing
    ).unwrap();
    
    // List sessions
    let sessions = registry.list_sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, session_id);
    assert_eq!(sessions[0].status, AgentSessionStatus::Running);
    
    // Stop session
    registry.stop_session(&session_id).unwrap();
    
    // Verify stopped
    let sessions = registry.list_sessions();
    assert_eq!(sessions[0].status, AgentSessionStatus::Exited);
}
```

**Test scenarios**:
- Ciclo de vida completo: start -> running -> stop -> exited
- Múltiples sesiones concurrentes
- Error handling en cada paso

**Verification**:
- Tests de integración pasan
- Ciclo de vida funciona correctamente

## Dependencies

- **portable-pty**: PTY multiplataforma
- **which**: Búsqueda de binarios en PATH
- **uuid**: Generación de IDs únicos
- **libc** (Unix): Process tree kill
- **tokio**: Async runtime (ya existe)

## Risks

### R1: portable-pty no soporta todas las plataformas
**Mitigación**: portable-pty tiene soporte para Linux, macOS, Windows. WSL funciona como Linux. Si hay problemas, fallback a std::process::Command con pipes (menos ideal pero funcional).

### R2: Process tree kill puede matar procesos incorrectos
**Mitigación**: Usar process groups en Unix (killpg) para asegurar que solo matamos procesos lanzados por la sesión. En Windows, taskkill /T mata el árbol completo.

### R3: Binarios de agentes pueden no estar en PATH
**Mitigación**: Validación clara con error descriptivo. El usuario debe instalar el agente y asegurar que está en PATH.

## Open Questions

### Q1: ¿Cómo manejar el output del PTY?
**Status**: Defer to ACI-002
**Context**: ACI-001 solo lanza y gestiona sesiones. El streaming de output al frontend se implementa en ACI-002 con eventos Tauri.

### Q2: ¿Cómo manejar el input del usuario?
**Status**: Defer to ACI-002
**Context**: ACI-001 solo lanza y gestiona sesiones. El input del usuario se implementa en ACI-002.

### Q3: ¿Cómo manejar resize de terminal?
**Status**: Defer to ACI-002
**Context**: ACI-001 proporciona el mecanismo (PtyHandle::resize). La integración con xterm.js se implementa en ACI-002.

## Success Criteria

- [ ] `cargo build` compila sin errores
- [ ] `cargo test` pasa todos los tests
- [ ] `cargo clippy` no reporta warnings
- [ ] Comandos Tauri son invocables desde frontend
- [ ] Sesiones se lanzan correctamente con binarios válidos
- [ ] Sesiones se detienen correctamente sin procesos zombie
- [ ] Process tree kill funciona en Unix/Windows/WSL
- [ ] Contrato está documentado en bus-contract.md

## References

- Roadmap: `docs/roadmaps/2026-06-19-002-agent-console-integration.md`
- Contrato actual: `docs/contracts/bus-contract.md`
- portable-pty docs: https://docs.rs/portable-pty
- which crate: https://docs.rs/which
