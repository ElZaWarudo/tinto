# Changelog

Each release's notes, newest first. `scripts/release.mjs` writes a section
from the conventional commits since the previous tag; see
[RELEASING.md](RELEASING.md).

## [1.0.1] - 2026-10-29

The first stable release of Tinto: from now on each version is tagged,
published as a GitHub Release, and installed Tinto updates itself. (1.0.0 was
tagged, but its release build failed and it was never published.)

### Monitoring

- **Workbenches**: named sets of repos, added by hand or found under a folder.
- **Dashboard**: a card per repo with its branch, changes, ahead/behind, last
  commit and live activity.
- **Live diff**: inline or side by side, syntax highlighted, updating as an
  agent writes.
- **Watchlist** for ignored files (`.env`, build output): their creation,
  changes and removal.
- **Timeline** across the workbench's repos, with commits and their diffs, and
  changes left uncommitted.
- **Signals and notifications**: large deletes, sensitive files, possible
  secrets and test changes, as OS notifications; and **glance mode**.
- **Project explorer** with rendered markdown, images and media, and file
  operations that can be undone.

### Agents

- Codex, Claude Code and Muse Code conversations on a repo, local or in WSL,
  with their turns, commands, checkpoints and changed files, in the app or in
  their own windows.
- Claude Code runs over stream-json: permissions are answered in the
  conversation, and a failing hook of its configuration is reported once.
- Conversations that were live when Tinto closed resume when it starts.

### Delivery

- A batch of issues, each in its own worktree and branch, taken through tests,
  implementation, review and QA by background Codex, Claude Code, Muse Code or
  OpenCode jobs, and coordinated by an agent over the `tinto-delivery` MCP
  server.
- Decisions, permissions and QA problems wait for you in Tinto; commit, push,
  PR, merge and Jira steps show exactly what they will publish, and each repo
  chooses which of them Tinto takes on its own.
- QA records with desktop and mobile screenshots, and rounds that send work
  back with what to address.
- Tinto runs each task's checks itself, only those a stage's changes reach,
  and follows the PR's CI, reviews and comments after it opens.
- Visual and technical guides distilled from the repo's review comments reach
  every job of the stage they apply to.

### Updates

- Tinto checks for a new release when it starts and installs it on request,
  verifying its signature.

## [1.0.0] - 2026-10-29

Tagged but not published: its release build failed at clippy. Its contents
shipped in 1.0.1.
