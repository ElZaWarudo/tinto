# Tinto

Aplicación de escritorio (Windows/Linux) de **monitoreo read-only** de repositorios git locales mientras son editados por agentes de código (Claude Code, Codex, etc.). Tinto nunca escribe sobre los repos: solo observa y presenta.

El diseño completo vive en [tinto-design.md](tinto-design.md); el roadmap de entrega en [docs/roadmaps/](docs/roadmaps/).

## Stack

- **Shell:** Tauri 2 (Rust + webview)
- **Frontend:** React 19 + TypeScript (Vite)
- **Backend:** Rust — git2-rs, notify, ignore, tokio (en items posteriores del roadmap)

## Desarrollo

Prerequisitos: Node LTS, Rust estable (MSVC en Windows), prerequisitos de Tauri 2 por plataforma.

```bash
npm install          # ignore-scripts=true vía .npmrc (no corre lifecycle scripts)
npm run tauri dev    # app en modo desarrollo
npm run tauri build  # binario de producción
```

Calidad:

```bash
npm run lint         # ESLint
npm run format       # Prettier
npm test             # Vitest (jsdom + mocks de Tauri)
cd src-tauri
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```

## Estado

Esqueleto inicial: ventana Tauri con puente webview↔Rust de humo (comando `ping` + evento `tick`). Las capacidades de monitoreo (git engine, watcher, workbenches, dashboard) se construyen según el roadmap en `docs/`.
