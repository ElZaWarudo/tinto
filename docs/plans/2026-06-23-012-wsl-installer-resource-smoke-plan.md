---
title: WSL installer resource and smoke closure plan
status: plan-review-passed
date: 2026-06-23
roadmap_item: RDM-006
---

# WSL Installer Resource And Smoke Closure Plan

## U1 - Resource declaration
Add `src-tauri/resources/` as the staging directory and declare `resources/tinto-agent-linux-x86_64` as a Tauri bundle resource copied to the resource root.

## U2 - CI resource wiring
Have Tauri bundle jobs download the `tinto-agent-linux-x86_64` artifact into `src-tauri/resources/` before `npm run tauri build`.

## U3 - Windows bundle proof
Add a Windows Tauri bundle CI job that consumes the Linux agent artifact and uploads the generated Windows bundles.

## U4 - Docs and verification
Update smoke docs, contract/state/package artifacts, run config and work-package checks, and record any real WSL smoke blocker.

## Verification
- `npx tsc --noEmit`
- `cargo test --lib wsl_agent`
- `npm run tauri -- config --help` or equivalent config parse check if available
- `git diff --check`
- Work package checker
