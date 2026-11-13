# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

Desktop application (Tauri 2, a WebView per window) for Windows, with WSL repos supported. A separate mobile companion app pairs with it; it is not covered here.

## Users

Any developer who works with coding agents (Claude Code, Codex, Muse Code, OpenCode). Confirmed 2026-11-13: the primary audience is open, not only the author and his team, so a user may arrive without knowing Delivery or its vocabulary.

Their situation: several agents work on their repos at once, for minutes or hours, while they do something else. They come back to Tinto to see what moved and to answer what only they can answer.

## Product Purpose

Tinto lets a developer work with coding agents without babysitting them.

The job that matters most (confirmed 2026-11-13) is **Delivery**: following a batch of issues, each taken from a branch to a merged PR in its own worktree, and answering only what needs the user: decisions, approvals, QA problems.

Two other jobs live in the same app and rank after it: watching what agents change in local repos, live and read-only (files, diffs, secrets, activity), and talking to agents in the Agents panel.

Delivery exists to give observability while every step keeps moving, with the fewest possible stops. Success is a batch that advances on its own and a user who can tell, at a glance, what changed and what waits on them.

## Positioning

- Each issue runs in its own worktree and branch, through stages (scope, tests, implementation, review, QA), by headless agent jobs.
- A coordinator agent runs the batch over the `tinto-delivery` MCP server; the user can take the batch over or hand it to another conversation at any time.
- Nothing is committed, pushed, posted or merged until the user approves that exact step and its exact text, unless they told Tinto to take that kind of step on its own.
- The monitoring layer is strictly read-only: it never commits, stages, edits or runs anything in the user's repos.
- It is local: no account, no tenant; the phone link is an end-to-end encrypted relay.

## Operating Context

- A long-lived desktop window, usually beside an editor and terminals, on a machine that is often busy running the agents' own jobs.
- Work arrives on its own: jobs finish, checks fail, reviewers comment, a coordinator stops. The user is frequently looking elsewhere when it happens.
- Delivery follows GitHub (PRs, checks, reviews) and Jira (issues, transitions) and shows the user the text it will publish there.
- Vocabulary in use: lote (batch), tarea, paquete, etapa, ronda, decisión, aprobación, coordinador, guías, QA, worktree.
- Panels: Delivery, the repo dashboard, repo and file views with diffs, the timeline, the Agents consoles, the project explorer.

## Capabilities and Constraints

- Built with React and plain CSS inside Tauri; panels are docked with dockview and can be detached into their own windows.
- One Tinto at a time holds the Delivery state.
- Several themes exist today (dark by default, plus light and others).
- Not binding for a redesign (asked 2026-11-13, left unmarked by the user): the square, no-rounded-corners look and the high information density of the current interface. Both are open decisions.

## Brand Commitments

- The name **Tinto** and its coffee idea: a Colombian tinto is the small black coffee drunk through the day to stay alert. "A Colombian tinto keeps you awake. This app does too." Small, brief, useful.
- The interface is in Spanish.

## Evidence on Hand

- A collaborator described the current look as "muy agresivo" (reported by the user, 2026-11-13). What screen and what in it prompted that is not recorded yet.
- The user reports ongoing cognitive overload in the interface (2026-11-13), after an earlier round aimed at reducing what the Delivery panel asks of them (v1.3.4).
- Earlier audits: `docs/audits/2026-07-17-product-polish-audit.md`, `docs/audits/2026-07-28-product-polish-audit.md`, `docs/product/application-atlas.md`.
- No user research, usage data or testimonials exist; none may be invented.

## Product Principles

1. **Keep it moving.** Prefer letting work continue over stopping it; reserve blocking prompts for what is truly the user's call.
2. **Show what changed, and what is yours.** State is visible without taking over the screen; what waits on the user is unmistakable and everything else recedes.
3. **The user approves what is published.** The exact step and the exact text, every time, unless they delegated that kind of step.
4. **Watching never touches.** Monitoring is read-only; anything that mutates a repo happens only by explicit action.
5. **Readable by someone new.** The audience is any developer with agents, so the interface cannot rely on knowing Delivery's vocabulary beforehand.
