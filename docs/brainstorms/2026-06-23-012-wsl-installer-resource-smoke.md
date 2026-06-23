---
title: WSL installer resource and smoke closure
status: reviewed
date: 2026-06-23
roadmap_item: RDM-006
---

# WSL Installer Resource And Smoke Closure

## Problem
RDM-006 made the WSL launcher packaged-first and CI builds a Linux `tinto-agent-linux-x86_64` artifact, but the Tauri bundle does not yet include that artifact as an application resource. A serious Windows release also needs CI proof that the Windows installer can be built with the Linux agent resource present.

## Decision
Bundle the Linux agent as a Tauri resource named `tinto-agent-linux-x86_64` at the resource root. CI downloads the Ubuntu-built artifact into `src-tauri/resources/` before every Tauri bundle job. Add a Windows bundle job so the release path is not inferred from an Ubuntu-only bundle.

## Requirements
- The repository must not commit the generated Linux binary.
- Tauri config must declare the resource path so packaged apps include it.
- CI must download the Linux agent artifact before Tauri bundle builds.
- Windows CI must build a Tauri bundle with the resource present.
- Manual smoke remains the final real-machine proof for Ubuntu WSL operations.

## Acceptance
- Tauri config validates with the resource declaration.
- CI has a resource download step before bundle builds.
- Windows bundle job uploads installer artifacts.
- Docs/state record that final manual Windows+Ubuntu smoke is still required unless run on this machine.
