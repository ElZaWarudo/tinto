# Changelog

Each release's notes, newest first. `scripts/release.mjs` writes a section
from the conventional commits since the previous tag; see
[RELEASING.md](RELEASING.md).

## [1.3.0] - 2026-11-10

### Features

- **delivery:** take «te necesita» to what waits, and surface proposed guide rules

### Fixes

- **delivery:** name every review thread still to answer once its fix is pushed
- **delivery:** open Delivery's settings as a page, not a cramped dialog
- **delivery:** claim steps, lock the store, re-arm limit retries and other review fixes
- **delivery:** give git push, fetch and ls-remote a timeout
- **delivery:** read every run of a wait_run up to one event (B14)
- **delivery:** open a new stage's log, and show errors swallowed until now
- **delivery:** keep edited commit, PR and merge texts until approved
- **delivery:** say once who a row waits on, in plain Spanish
- **delivery:** add a slow job-log read only once
- **delivery:** do not notify again when the user settles a QA problem
- **delivery:** keep a float open while its confirm is answered
- **delivery:** count only open QA problems as waiting on the user
- **delivery:** save repo settings over the current ones, and fail visibly
- **delivery:** keep a restack paused when its continue fails
- **agents:** do not fail a PTY stop whose process was reaped
- **agents:** say why a queue is stuck and confirm deletes in the app
- **agents:** do not stop a session moved to another window
- **agents:** keep the saved layout when Agents was open at exit
- **agents:** count the session time limit from the last activity
- **agents:** keep files the snapshot could not read on undo
- **agents:** keep a live session's checkpoints out of retention
- **agents:** pause the queue when a queued message fails to send
- **agents:** refuse an undo while another session works in the repo
- **agents:** keep the dock mounted in Glance mode
- **agents:** deny a Claude Code tool when its permission is cancelled
- **delivery:** embed only QA screenshots inside the QA folder
- **delivery:** publish fixes before the replies and review that describe them
- **files:** operate on symlinks as links instead of following them
- **dashboard:** show the repo filter set from the explorer
- **wsl:** keep login-profile output off the WSL agent's stdout
- **files:** keep markdown links from navigating the app window
- **relay:** cap the sockets, and so the rooms, the relay holds
- **mobile:** withdraw presence when the other side goes quiet
- **mobile:** approve a text decision from the phone only when it was shown whole
- **mobile:** refuse a hello from a peer session already replaced
- **mobile:** allow a permission from the phone only when it shows all it grants
- **notifications:** announce a repo error or signal once, not per revision
- **files:** move instead of copying on delete and purge old undo copies
- **watcher:** see commit, stage and checkout in linked worktrees
- **watcher:** keep a missing root dead after a failed retry
- **files:** ignore an OS file drop outside the explorer's own tree
- **relay:** answer 400 instead of crashing on an unparsable upgrade path
- **delivery:** show a merged task's step left and say «espera al»
- **delivery:** name the step left on a merged PR for the coordinator

### Performance

- **delivery:** post a review's replies a few at a time
- **bus:** cap what the secret scan copies and the untracked diff reads

## [1.2.6] - 2026-11-10

### Fixes

- **delivery:** show what waits on the user as theirs, not as blocked

## [1.2.5] - 2026-11-09

### Features

- **delivery:** record history made outside Tinto and stop a push on a moved remote
- **delivery:** pause a conflicting restack and continue it inside Tinto

### Fixes

- **delivery:** say when the coordinator waits for the user's permission
- **delivery:** run one step at a time in a task's worktree
- **delivery:** leave git's index and unrunnable checks to Tinto in every job
- **delivery:** hold the PR step until a reviewer is chosen
- **delivery:** pad a running merge like the step card it stands in
- **delivery:** read a QA section that points at a problem as no problem
- **delivery:** give each package of an issue its own {task} value
- **delivery:** refuse tool arguments the tool does not declare

## [1.2.4] - 2026-11-09

### Features

- **delivery:** make the quota stop a setting and add per-stage fallback agents

## [1.2.3] - 2026-11-09

### Features

- **delivery:** show the PR's unanswered review threads in its panel
- **delivery:** hold the PR back from review while a review thread has no reply
- **delivery:** pair a phone through a blind relay to follow and act on batches
- **agents:** send diff line notes to an agent and show who needs you

### Fixes

- **delivery:** keep coordinator results under the size the client reads

## [1.2.2] - 2026-11-05

### Features

- **delivery:** send the fields a Jira transition requires
- **delivery:** mark tests that fail only because of the environment

### Fixes

- **delivery:** start tasks from the remote base and set up again after a sync
- **delivery:** put back lockfiles a setup or check command rewrote
- **delivery:** restack a package from its predecessor's merged head

## [1.2.1] - 2026-11-05

### Features

- **delivery:** run an approved merge apart and show its checks, progress and errors
- **delivery:** state machines for delivery steps and PR reviewers

## [1.2.0] - 2026-11-04

### Features

- **delivery:** start the coordinator with full access
- **delivery:** make scoping a first-class Alcance stage

## [1.1.2] - 2026-11-03

### Features

- **delivery:** explain recommended decisions and let the user ask why

## [1.1.1] - 2026-11-03

### Features

- **delivery:** hold a package's pr until its predecessor merges

### Fixes

- **delivery:** merge ready prs without syncing the rest of the batch

## [1.1.0] - 2026-10-30

### Features

- **delivery:** lighter panel header and overlays that cannot be missed
- **delivery:** Tinto takes the deterministic steps of a batch itself

### Fixes

- **agents:** a crashed agent shows as failed at once and can resume

### Performance

- **watcher:** watch only the directories git does not ignore
- **watcher:** poll only WSL drive roots and drop the unused tick event

## [1.0.3] - 2026-10-30

### Fixes

- **delivery:** a check's long output no longer freezes the panel

## [1.0.2] - 2026-10-29

### Features

- **delivery:** CI policy for Tinto's pushes, cancelled runs and path-filtered workflows
- **delivery:** `{job}` in check_env gives each job its own value

### Fixes

- **delivery:** publish flow survives restacks, stacked merges and rewrites
- **delivery:** retry_job warns when a check that failed at once will fail again
- **delivery:** a coordinator reads only the batch it coordinates
- **delivery:** next_stage leaves QA out when there is nothing for it to verify
- **delivery:** Tinto stops starting agent jobs near the account's limit
- **delivery:** job agents get only the tools their stage needs
- **delivery:** changes asked on a PR open their round as they arrive
- **delivery:** a checks round that a re-run fixed closes on its own
- **delivery:** a Tinto restart no longer throws away long check runs
- **delivery:** only the checks configured now hold a task, and a failing one can be accepted
- **delivery:** jobs, checks and setup run without the AppImage's environment
- **delivery:** a failed check's summary quotes the failure, not a Node warning
- **delivery:** a coordinator's overview fits, and autonomy reaches waiting steps
- **agents:** show Claude Code's permission requests in the conversation

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
