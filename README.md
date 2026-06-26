# Tinto

> A Colombian tinto keeps you awake. This app does too.

## The name

In Colombia, a **tinto** is not a wine — it's the small cup of black coffee,
strong and short, drunk throughout the day to stay alert. It's the morning
ritual, the mid-afternoon pause, the offer you make to any visitor. Drinking
one is a brief pact: *"I'll be awake a little longer."*

**Tinto** (this app) inherits that pact. It's a desktop application that keeps
you **awake to the changes in your projects**: it watches what your coding
agents are doing, in real time, and shows it to you so you decide where to
look — without opening a heavy editor, without losing the thread.

If your Claude Code, Codex, or OpenCode is silently modifying six files,
Tinto tells you. If it left uncommitted work an hour ago, Tinto flags it.
If an `.env` changed and nobody told you, Tinto highlights it.

Like the morning coffee: small, brief, useful. And it keeps you awake.

## What it does

Tinto is a **non-invasive supervisor** for local git repositories. While
coding agents (Claude Code, Codex, OpenCode, and similar) edit your code,
Tinto watches, classifies, and shows — without silently mutating your work
as part of the monitoring loop.

### What Tinto never does to your repos (the monitoring layer)

Tinto's supervision pipeline is **strictly read-only**:

- It does not commit, stage, branch, merge, or revert.
- It does not modify files, rename them, or delete them as part of watching.
- It does not run your tests, your linters, or your build.
- It does not push, fetch, or talk to any remote.

If Tinto crashes mid-session, the only thing on disk that changes is its own
config file in your OS config directory. Your repos are untouched.

### What Tinto does offer (user-initiated workspace tools)

The workspace includes tools you invoke yourself. These are **separate from
the monitoring pipeline** and Tinto will never trigger them on its own:

- **File operations** with safety nets: delete goes to the OS trash
  (recoverable), copy and move can be undone from the file dock clipboard.
- **Integrated terminal** (xterm.js) for running commands against the open
  repo. Detachable into its own window.
- **External agent terminal** launcher that spawns your agent's CLI in a real
  shell.

Turning any of these off does not affect monitoring. They are conveniences
for you, the human.

### Operating principles

- **Passive.** Tinto does not approve or reject anything. It surfaces
  signals; you decide what to act on.
- **Lightweight.** Designed to stay open all day. Low RAM and CPU.
- **Local.** Only your local filesystem. No cloud, no remote agents
  reporting in.
- **No interpretation.** Tinto does not summarize your changes with AI. It
  shows the facts: diffs, events, timestamps.

## Features

### Workbenches

A **workbench** is a named set of repos you want to monitor together
(e.g. *"Work"*, *"Side projects"*, *"Client X"*). A repo can belong to
multiple workbenches. Repos are added manually or by autodetecting all
`.git` folders under a root directory. Configuration persists across
sessions.

### Two parallel monitoring planes

| Plane       | What it watches                              | What it shows                                                       |
| ----------- | -------------------------------------------- | ------------------------------------------------------------------- |
| **Plane 1** | Git-tracked files                            | `git status`, diffs (inline / side-by-side), branch, ahead/behind   |
| **Plane 2** | Gitignored files on an opt-in watchlist      | create / modify / delete events, size, timestamp                    |

Plane 2 exists because the most sensitive things (`.env`, secrets, generated
configs) usually live in `.gitignore`. You declare what to watch
(`.env`, `dist/**`, `*.log`, etc.) and Tinto surfaces events from those
exact paths without filtering by gitignore.

### Dashboard

One card per repo: current branch, counts of modified / staged / untracked,
ahead-behind, last commit, **live activity indicator**. Compact and
expanded card views.

### Diff viewer

- **Live diff** that updates itself while the agent writes.
- **Inline** and **side-by-side** modes with syntax highlighting (Shiki).
- Full-file view with changes highlighted, not just the isolated hunk, so
  you keep context.
- Overview ruler (changes map) synced with scroll.

### Timeline

Chronological feed across all repos in the active workbench
("14:32 — Project X: 3 files modified"). Commit navigation with their
diffs, no terminal required. Detection of orphan changes (dirty working
tree sitting uncommitted for a while).

### Terminal

Integrated terminal panel (xterm.js) for running commands on the open repo.
Supports a **detached window** so you can keep the terminal on one monitor
while watching the diff on another.

### Project explorer

File tree with persistent collapse state, copy-path, and file clipboard.
File view supports rendered markdown, images, and media.

### Passive signals and notifications

- **Auto-highlights** on changes worth a second look: large deletes,
  sensitive files (`.env`, CI, configs), possible secrets (gitleaks
  notice), test changes.
- **Lightweight metrics**: lines +/- per repo, files touched per session,
  change frequency.
- **Native OS notifications** for relevant events.
- **Glance mode**: compact window with a one-line summary so Tinto can sit
  in a corner of your screen.

### Quality of life

- Filters and search by repo, extension, or time range.
- **Fixed keyboard shortcuts** (see the in-app shortcuts panel for the
  full list; bindings are not user-rebindable yet).
- UI zoom.
- File ops with **undo** (trash-based delete, reversible copy/move).

## Platforms

Windows and Linux. The `tinto-agent` binary runs inside WSL on Windows
hosts to access Linux repos.

## Stack

| Layer          | Choice                                       |
| -------------- | -------------------------------------------- |
| Shell          | Tauri 2 (Rust + webview)                     |
| Frontend       | React 19 + TypeScript + Vite                 |
| Git state      | git2-rs (vendored libgit2)                   |
| File watching  | notify                                       |
| Path classify  | ignore (ripgrep crate) + globset             |
| Async runtime  | tokio                                        |
| Persistence    | TOML in the OS config dir (`dirs` crate)     |

The full design (including the Live Diff flow and the event bus
architecture) is in [tinto-design.md](tinto-design.md).

## Status

The app already ships: dashboard, live diff viewer, file tree, integrated
terminal, timeline, workbenches, WSL support, glance mode, notifications,
and most of the passive signals. The living roadmap is in
[docs/roadmaps/](docs/roadmaps/) and the build/packaging plan in
[docs/build-guide.md](docs/build-guide.md).

## Development

Prerequisites: Node LTS, stable Rust (MSVC on Windows), Tauri 2 platform
prerequisites per OS (see [docs/build-guide.md](docs/build-guide.md) for
the full list).

```bash
npm install          # ignore-scripts=true via .npmrc (skips lifecycle scripts)
npm run tauri dev    # app in dev mode
npm run tauri build  # production binary + installer
```

Quality checks:

```bash
npm run lint         # ESLint
npm run format       # Prettier
npm test             # Vitest (jsdom + Tauri mocks)
cd src-tauri
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## Getting started

1. Create your first workbench.
2. Point it at a folder or autodetect repos.
3. Start a coding agent on one of those repos.
4. Watch the dashboard: that repo's card starts moving, the live diff
   updates by itself, events appear in the timeline.
5. Step away? **Glance mode**. Care about a gitignored file? Add it to the
   Plane 2 watchlist.

Tinto does nothing *to* your repos through the monitoring pipeline. It
keeps you awake while the AI works. That's the deal.
