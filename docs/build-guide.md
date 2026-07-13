# Tinto — Build Guide

This guide covers every way Tinto can be built: the production installer, the
standalone desktop app, the `tinto-agent` helper, the frontend bundle, quality
checks, and brand asset generation. It reflects the same workflow used by CI
(`.github/workflows/ci.yml`).

## Table of contents

1. [Prerequisites](#prerequisites)
2. [Repository layout](#repository-layout)
3. [Install JS dependencies](#install-js-dependencies)
4. [Quality checks](#quality-checks)
5. [Build the frontend](#build-the-frontend)
6. [Build the desktop app (debug)](#build-the-desktop-app-debug)
7. [Build the tinto-agent](#build-the-tinto-agent)
8. [Build the production installer](#build-the-production-installer)
9. [Run the app in dev mode](#run-the-app-in-dev-mode)
10. [Regenerate brand assets](#regenerate-brand-assets)
11. [Clean build artifacts](#clean-build-artifacts)
12. [Troubleshooting](#troubleshooting)

---

## Prerequisites

Tinto is a Tauri 2 app: a Rust shell hosting a React 19 + Vite webview.

- **Node.js 24** — required by `package.json`; CI uses the same major.
- **Rust** — stable toolchain (CI uses `dtolnay/rust-toolchain@stable`).
  - **Windows**: install the **MSVC** target (`rustup default stable-msvc`).
  - **Linux**: install the build deps below.
  - **macOS**: Xcode Command Line Tools.
- **Tauri 2 platform dependencies** — see <https://v2.tauri.app/start/prerequisites/>.
  - **Linux (Debian/Ubuntu)**:
    ```bash
    sudo apt-get update
    sudo apt-get install -y \
      build-essential curl file \
      libayatana-appindicator3-dev librsvg2-dev libssl-dev \
      libwebkit2gtk-4.1-dev libxdo-dev patchelf wget
    ```
  - **Windows**: WebView2 runtime (preinstalled on Windows 10/11) and the
    "Desktop development with C++" Visual Studio workload.
  - **macOS**: nothing extra beyond Xcode CLT.
- **Python 3** — only required to regenerate brand assets
  (`scripts/generate_brand_assets.py`).
- **WiX Toolset** (Windows) / **NSIS** (Windows) — auto-installed by Tauri's
  bundler on first run, but can be installed ahead of time for offline builds.

> Note: `.npmrc` sets `ignore-scripts=true` so npm lifecycle scripts are
> intentionally skipped. Re-enable them only if you trust the transitive
> dependencies you install.

---

## Repository layout

| Path                     | Purpose                                                          |
| ------------------------ | ---------------------------------------------------------------- |
| `src/`                   | React + TypeScript frontend (Vite)                               |
| `src-tauri/src/main.rs`  | `tinto` binary entry point (default)                             |
| `src-tauri/src/lib.rs`   | `tinto_lib` — shared Rust library                                |
| `src-tauri/src/bin/tinto-agent.rs` | `tinto-agent` binary (WSL agent runtime helper)         |
| `src-tauri/tauri.conf.json` | Tauri configuration (window, bundle, identifiers, icons)     |
| `src-tauri/Cargo.toml`   | Rust manifest; defines two `[[bin]]` targets                     |
| `src-tauri/icons/`       | App icons for every platform (`.ico`, `.icns`, `.png`)           |
| `src-tauri/resources/`   | Files bundled into the installer at build time                   |
| `scripts/`               | Build-time helpers (e.g. brand asset generation)                 |
| `dist/`                  | Vite frontend output (created by `npm run build`)                |
| `src-tauri/target/`      | Cargo build output (debug + release)                             |

The Tauri manifest defines two binaries:

```toml
[[bin]]
name = "tinto"
path = "src/main.rs"

[[bin]]
name = "tinto-agent"
path = "src/bin/tinto-agent.rs"
```

The bundle config (in `src-tauri/tauri.conf.json`) ships a `tinto-agent` helper
from `src-tauri/resources/` into the installer for WSL scenarios.

---

## Install JS dependencies

```bash
npm install
```

This installs the frontend toolchain (Vite, React, Tauri JS API, etc.). Rust
dependencies are downloaded separately by Cargo on the first Rust or Tauri
build.

To install with the lockfile (reproducible / CI mode):

```bash
npm ci
```

---

## Quality checks

Run these before opening a PR. They match the read-only quality gates in CI.

### Frontend

```bash
npm run contract:check  # Generated Rust/TypeScript bus contract drift
npm run lint            # ESLint
npm run format:check    # Prettier (read-only)
npm test                # Vitest, jsdom + Tauri mocks
```

To auto-fix frontend formatting, run `npm run format` separately.

### Rust

```bash
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

To auto-fix Rust formatting:

```bash
cd src-tauri
cargo fmt
```

---

## Build the frontend

The frontend is a Vite SPA. The build does TypeScript type-checking followed
by a production Vite bundle.

```bash
npm run build
```

This runs `tsc && vite build` and writes the static assets to `dist/`. The
Tauri app loads these files in production (see `frontendDist` in
`tauri.conf.json`).

To preview the built frontend in a browser (no Tauri shell):

```bash
npm run preview
```

---

## Build the desktop app (debug)

A debug build produces a runnable binary in
`src-tauri/target/debug/tinto(.exe)`. It is **not** an installer.

```bash
cd src-tauri
cargo build
# or, with the npm wrapper (also runs `npm run build` first):
# npm run tauri build -- --debug
```

Output:

- Linux:   `src-tauri/target/debug/tinto`
- Windows: `src-tauri/target/debug/tinto.exe`
- macOS:   `src-tauri/target/debug/tinto`

Run it directly to smoke-test without installing.

---

## Build the tinto-agent

`tinto-agent` is a small CLI binary that implements the WSL agent runtime
contract (stdin/stdout JSON over `wsl_agent::runtime::respond_to_request_line`).
It is built and shipped as a Tauri resource so the main app can invoke it
from WSL on Windows hosts.

### Release build (what CI does)

```bash
cd src-tauri
cargo build --release --bin tinto-agent
```

Output:

- Linux:   `src-tauri/target/release/tinto-agent`
- Windows: `src-tauri/target/release/tinto-agent.exe`
- macOS:   `src-tauri/target/release/tinto-agent`

### Bundle the agent as a Tauri resource

Tauri's `bundle.resources` config copies everything from `src-tauri/resources/`
into the installer. To ship a prebuilt agent, drop it there under a stable
name (CI uses the host triple):

```bash
# Linux host (the canonical CI flow):
cp src-tauri/target/release/tinto-agent src-tauri/resources/tinto-agent-linux-x86_64
```

The Windows CI job downloads this same artifact and reuses it when bundling
on Windows, because the agent is meant to run *inside* WSL on a Windows
machine.

> You can also use the helper script CI uses, by mirroring it locally:
> `cargo build --release --bin tinto-agent && cp target/release/tinto-agent ../tinto-agent-linux-x86_64`

### Debug build

```bash
cd src-tauri
cargo build --bin tinto-agent
# output: src-tauri/target/debug/tinto-agent(.exe)
```

### Quick manual test

The agent reads one JSON request line from stdin and writes one response
line. Example (POSIX):

```bash
printf '%s\n' '<some-json-request>' | ./src-tauri/target/release/tinto-agent
```

Refer to `src-tauri/src/wsl_agent/protocol.rs` and
`src-tauri/src/wsl_agent/runtime.rs` for the exact request shape and
error categories.

---

## Build the production installer

The installer is produced by Tauri's bundler. Because
`tauri.conf.json` sets `bundle.targets` to `"all"`, the bundler picks the
right format for the host OS:

- **Windows**: `.msi` (WiX) and/or NSIS `.exe`
- **Linux**:   `.deb`, `.AppImage`
- **macOS**:   `.dmg` (and `.app`)

### One-shot (frontend + Rust release + bundle)

This is what you want for a release. It runs `npm run build` first
(`beforeBuildCommand`) and then compiles Rust in release mode and bundles.

```bash
npm run tauri build
```

### Required Tauri resource

If `src-tauri/resources/tinto-agent-linux-x86_64` is missing, the bundler
will still succeed but the in-app WSL agent feature will be unavailable. CI
produces this file in the `rust` job and downloads it into
`src-tauri/resources/` before bundling. To reproduce locally:

```bash
cd src-tauri
cargo build --release --bin tinto-agent
cp target/release/tinto-agent resources/tinto-agent-linux-x86_64
cd ..
npm run tauri build
```

### Where the installers land

All installers are written under `src-tauri/target/release/bundle/`:

```
src-tauri/target/release/bundle/
├── msi/        # Windows .msi  (WiX)
├── nsis/       # Windows NSIS .exe
├── deb/        # Debian/Ubuntu .deb
├── appimage/   # Linux AppImage
├── dmg/        # macOS .dmg
└── macos/      # macOS .app bundle
```

The standalone release binary (no installer) is at:

```
src-tauri/target/release/tinto(.exe)
```

### Cross-platform notes

- **Building Windows installers from Linux/macOS** is not supported by
  Tauri's bundler. Use a Windows host (or a Windows VM/CI runner).
- **Building `.msi` requires WiX 3.x**, **NSIS requires NSIS 3.x**. The
  Tauri bundler downloads them on first run into
  `src-tauri/target/release/`. Make sure outbound HTTPS to
  `github.com` and `nsis.sourceforge.io` works the first time.
- **Code signing** is not configured. For signed installers, add
  `bundle.windows.signCommand` / `bundle.macOS.signingIdentity` in
  `tauri.conf.json` and set the matching env vars at build time.

### Tauri CLI flags worth knowing

```bash
# Build only for a single target on the current OS:
npm run tauri build -- --bundles appimage   # Linux only
npm run tauri build -- --bundles msi        # Windows only

# Build a debug installer (faster, unsigned):
npm run tauri build -- --debug

# Pick a custom output directory:
npm run tauri build -- --output-dir ./out
```

---

## Run the app in dev mode

Dev mode launches Vite + the Tauri shell together with hot reload for the
frontend and incremental Rust rebuilds.

```bash
npm run tauri dev
```

For Windows development that needs repos inside WSL, start Tinto from
Windows PowerShell with the WSL-aware wrapper:

```bash
npm run tauri:dev:wsl
```

`npm run tauri:dev:wsl` looks for a downloaded
`tinto-agent-linux-x86_64` under `.ci-artifacts/` or `src-tauri/resources/`
and passes it to the app through `TINTO_WSL_AGENT_LINUX_BIN`. This avoids
compiling the Linux agent from source inside WSL during `tauri dev`.

If no agent artifact is present, the script falls back to source mode
(`TINTO_WSL_AGENT_ALLOW_DEV_SOURCE=1`). That fallback requires the Linux
Tauri/GTK build dependencies inside the WSL distro; without them, WSL repo
loading can fail with missing `gdk-3.0` or `cairo` pkg-config errors. For a
quick Windows dev smoke, download the CI-built agent first:

```powershell
gh run download <run-id> `
  --repo ElZaWarudo/tinto `
  --name tinto-agent-linux-x86_64 `
  --dir .ci-artifacts\<run-id>

npm run tauri:dev:wsl
```

To run the Vite dev server without the Tauri shell (browser-only preview):

```bash
npm run dev
```

### Native Tauri E2E without desktop input control

Use the embedded WebDriver smoke when the frontend must exercise real Tauri
commands without taking over the machine's mouse or keyboard:

```bash
npm run test:e2e:tauri
```

The runner builds a test-only Tauri binary with the `e2e-wdio` feature,
starts its loopback WebDriver server, executes the smoke through WebdriverIO,
and captures `artifacts/tauri-e2e/native-shell.png`. The current smoke calls
the real Rust `ping` command, creates and activates an isolated workbench, and
waits for the native dashboard.

The E2E process is isolated from the normal Tinto profile: config, home,
Codex home, local data, temporary files, and the WebView profile all live
under a disposable `tinto-e2e-*` directory. The feature build uses a separate
Cargo target, requires an explicit runner marker and WebDriver port, and its
top-level executable is removed after the run. The runner also verifies that
the workbench and WebView writes landed under the disposable profile.

On Windows, the controlled Tauri window can still become visible while the
test runs; WebView2 does not provide a true invisible headless mode here. It
does not use global desktop input. The first run may download the matching
`msedgedriver`, so network access is required unless that driver is already
available.

`npm run dev` and `npm run preview` remain browser-only surfaces. They do not
execute Rust commands; use them for responsive visual QA and fixtures, and
use `npm run test:e2e:tauri` when native IPC is part of the scenario. The
current E2E smoke does not cover WSL or Agent Console process execution.

---

## Regenerate brand assets

Brand icons, wordmarks, and color tokens are produced from
`scripts/generate_brand_assets.py` (Pillow + a few stdlib helpers). The
generated PNGs end up under `brand/` and are consumed by the Tauri icon set
and the frontend.

```bash
python3 scripts/generate_brand_assets.py
# or, via the npm alias:
npm run brand:generate
```

Re-run this whenever you change the brand source files in `brand/source/` or
the design tokens in `tinto-design.md`.

---

## Clean build artifacts

```bash
# Frontend
rm -rf dist
rm -rf node_modules

# Rust
cd src-tauri
cargo clean
```

`cargo clean` removes both `target/debug` and `target/release`, so use it
sparingly — a full Rust rebuild can take several minutes the first time.

---

## Troubleshooting

- **`npm run tauri build` fails on WiX/NSIS download**: run
  `npm run tauri build` once with network access so the bundler can fetch
  the tools into `src-tauri/target/release/`. Subsequent offline builds
  will reuse them.
- **`link.exe` not found on Windows**: install the "Desktop development with
  C++" Visual Studio workload and the Windows SDK, then run the build from
  a "Developer Command Prompt for VS" or after
  `vcvarsall.bat x64` has been sourced.
- **`webkit2gtk-4.1` not found on Linux**: install the package listed in the
  prerequisites section, or upgrade to a distro release that ships it
  (Ubuntu 22.04+, Fedora 38+, etc.).
- **Frontend build fails on TypeScript**: `npm run build` runs `tsc` with
  `noEmit` semantics. Read the error locations; they are precise.
- **`tinto-agent` not bundled**: ensure
  `src-tauri/resources/tinto-agent-linux-x86_64` exists **before** running
  `npm run tauri build`. The file is read by the bundler at bundle time, not
  at runtime.
- **App is unsigned / SmartScreen warns on Windows**: expected for local
  builds. Configure code signing (see Cross-platform notes above) for
  releases.

---

## Summary of commands

| Goal                              | Command                                                                 |
| --------------------------------- | ----------------------------------------------------------------------- |
| Install JS deps                   | `npm install` (or `npm ci`)                                             |
| Contract / lint / format / test   | `npm run contract:check` / `npm run lint` / `npm run format:check` / `npm test` |
| Frontend production bundle        | `npm run build`                                                         |
| Desktop app debug build           | `cd src-tauri && cargo build`                                           |
| `tinto-agent` release build       | `cd src-tauri && cargo build --release --bin tinto-agent`               |
| Ship the agent to the installer   | `cp src-tauri/target/release/tinto-agent src-tauri/resources/tinto-agent-linux-x86_64` |
| Production installer              | `npm run tauri build`                                                   |
| Single-target installer (example) | `npm run tauri build -- --bundles msi`                                   |
| Dev mode (Tauri + Vite)           | `npm run tauri dev`                                                     |
| WSL-safe dev mode                 | `npm run tauri:dev:wsl`                                                 |
| Isolated native Tauri E2E         | `npm run test:e2e:tauri`                                                |
| Rust checks                       | `cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` |
| Regenerate brand assets           | `npm run brand:generate`                                                |
| Clean everything                  | `rm -rf dist node_modules && cd src-tauri && cargo clean`               |
