---
title: RDM-006 final installer resource code and security review
status: passed
date: 2026-06-23
package: docs/work-packages/RDM-006-final-wsl-installer-resource-smoke/2026-06-23-012-wsl-installer-resource-smoke-work-package.md
review_unit: RU1
---

# RDM-006 Final Installer Resource Code And Security Review

## Result
Passed. No remaining P0-P2 findings.

## Scope Reviewed
- `.github/workflows/ci.yml`
- `.gitignore`
- `src-tauri/tauri.conf.json`
- `src-tauri/resources/.gitkeep`
- `docs/manual-smoke/2026-06-23-windows-ubuntu-wsl-agent-bootstrap.md`
- `docs/contracts/bus-contract.md`

## Checks
- Tauri bundles the `src-tauri/resources/` directory to the resource root, so CI-staged `tinto-agent-linux-x86_64` lands where the packaged launcher searches first.
- CI downloads the Ubuntu-built `tinto-agent-linux-x86_64` artifact before Ubuntu and Windows Tauri bundle jobs.
- CI verifies the resource file exists before bundle execution; missing artifacts fail the workflow instead of producing a silent no-agent package.
- The generated Linux agent binary is ignored by git and only `.gitkeep` is committed for the staging directory.
- Manual smoke now validates packaged-installer launch without `TINTO_WSL_AGENT_LINUX_BIN`, file operations, local+WSL coexistence, and WSL Agent Console checkpoint/revert.

## Verification
- `npm run tauri -- info` parsed the Tauri config; local environment reported missing Visual Studio Build Tools/MSVC.
- `cargo test --lib wsl_agent` passed 26/26.
- `cargo build --bin tinto-agent` passed with pre-existing warnings only.
- `npx tsc --noEmit` passed.
- `npx prettier --check .github/workflows/ci.yml src-tauri/tauri.conf.json docs/manual-smoke/2026-06-23-windows-ubuntu-wsl-agent-bootstrap.md docs/contracts/bus-contract.md` passed.
- `git diff --check` passed with CRLF warnings only.
- Work package checker passed.

## Residual Risk
Local Windows installer build was not run because this machine does not have Visual Studio Build Tools/MSVC installed. The new GitHub Actions Windows bundle job is the CI proof path, and the manual Windows+Ubuntu WSL smoke remains the final real-machine release gate.
