---
title: Windows Ubuntu WSL agent bootstrap manual smoke
status: agent-smoke-passed-ui-pending
date: 2026-06-24
roadmap_item: RDM-002/RDM-006
---

# Windows Ubuntu WSL Agent Bootstrap Manual Smoke

## Scope

Validate the Windows host to Ubuntu WSL `tinto-agent` bootstrap on a real Windows host. RDM-006 makes packaged Linux agent discovery/install the primary path and keeps the RDM-002 dev-source command as an explicit fallback. This smoke also confirms local and WSL repos can coexist in one workbench before final release.

## Preconditions

- Windows host with WSL 2 enabled.
- Ubuntu distro installed and visible in `wsl.exe -l -v`.
- A packaged Tinto Windows build produced after CI downloaded `tinto-agent-linux-x86_64` into `src-tauri/resources/`, or a Linux `tinto-agent` binary available on the Windows host for the explicit override path.
- Optional fallback only: Tinto repository and Rust/Cargo available inside Ubuntu.

## Steps

1. From Windows PowerShell, confirm Ubuntu is WSL 2:

   ```powershell
   wsl.exe -l -v
   ```

2. Preferred packaged-installer path: launch the packaged Tinto build without `TINTO_WSL_AGENT_LINUX_BIN` and keep dev fallback disabled:

   ```powershell
   Remove-Item Env:\TINTO_WSL_AGENT_LINUX_BIN -ErrorAction SilentlyContinue
   Remove-Item Env:\TINTO_WSL_AGENT_ALLOW_DEV_SOURCE -ErrorAction SilentlyContinue
   ```

   If testing an unpackaged dev build, use the explicit override instead:

   ```powershell
   $env:TINTO_WSL_AGENT_LINUX_BIN="C:\path\to\tinto-agent-linux-x86_64"
   Remove-Item Env:\TINTO_WSL_AGENT_ALLOW_DEV_SOURCE -ErrorAction SilentlyContinue
   ```

3. Launch Tinto and add one Windows repo plus one Ubuntu WSL repo (`/home/...`) to the same workbench.

4. Confirm the WSL repo reaches a non-terminal state: tree, status, diff, log, and file content load without requiring the source checkout inside Ubuntu.

5. In the WSL repo, drag or paste a small file into the Explorer, then delete, restore, redo, and export it. Confirm the same operations still work on the Windows repo.

6. Start and stop an Agent Console session for the WSL repo. Create or modify a test file during the session, stop the session, confirm a change log is shown, then use Revert with explicit consent and confirm the WSL repo returns to the checkpoint state.

7. Confirm the agent was installed in Ubuntu:

   ```powershell
   wsl.exe -d Ubuntu -- sh -lc 'test -x "$HOME/.local/share/tinto/agents/0.1.0/tinto-agent" && echo ok'
   ```

8. Confirm compatible handshake with the installed agent:

   ```powershell
   '{"type":"handshake","protocol_version":1,"client_version":"manual-smoke"}' |
     wsl.exe -d Ubuntu -- sh -lc 'exec "$HOME/.local/share/tinto/agents/0.1.0/tinto-agent"'
   ```

9. Confirm incompatible protocol fails safely:

   ```powershell
   '{"type":"handshake","protocol_version":999,"client_version":"manual-smoke"}' |
     wsl.exe -d Ubuntu -- sh -lc 'exec "$HOME/.local/share/tinto/agents/0.1.0/tinto-agent"'
   ```

10. Optional development fallback: unset the packaged binary and explicitly enable fallback:

   ```powershell
   Remove-Item Env:\TINTO_WSL_AGENT_LINUX_BIN -ErrorAction SilentlyContinue
   $env:TINTO_WSL_AGENT_ALLOW_DEV_SOURCE="1"
   '{"type":"handshake","protocol_version":1,"client_version":"manual-smoke"}' |
     wsl.exe -d Ubuntu -- cargo run --manifest-path /path/to/tinto/src-tauri/Cargo.toml --bin tinto-agent
   ```

11. Confirm stdout is one line of JSON with:

   ```json
   {"type":"handshake","protocol_version":1,"agent_version":"0.1.0","status":"ok"}
   ```

12. Confirm stderr includes `protocol_mismatch` and does not print host environment variables, secrets, or Windows filesystem details.

## Expected Result

- Compatible handshake succeeds.
- Incompatible handshake fails with a safe category.
- Packaged Linux agent path works without the Ubuntu source checkout.
- Dev-source launch works only when explicitly enabled for development.
- Local Windows and Ubuntu WSL repos can be tracked together.
- WSL file operations preserve copy/delete/restore/redo/export behavior.
- WSL Agent Console sessions launch inside Ubuntu, expose a real checkpoint, and revert through the Linux agent after explicit consent.
- No Linux desktop WSL UI or frontend surface is introduced by this smoke.

## Evidence

Partial agent-level smoke passed on 2026-06-24 against CI run `28082169551` and commit
`9fe58fa8959261b93f8bfec3820361bdc54d8cc6`.

- GitHub Actions CI run `28082169551` passed: Frontend, Rust, Linux Tauri bundle, and Windows
  Tauri bundle.
- Downloaded artifacts:
  - `.ci-artifacts/28082169551/tinto-windows-bundle/nsis/Tinto_0.1.0_x64-setup.exe`
    sha256 `689e1a3d87a8058eeffbafd588cb50214da41e45a1365a9f11588ab39b056f85`.
  - `.ci-artifacts/28082169551/tinto-windows-bundle/msi/Tinto_0.1.0_x64_en-US.msi`
    sha256 `da95854ec5b7ec0a5dd07423cccbd55ffdfa86d6ebe07ee6941a04415c8466aa`.
  - `.ci-artifacts/28082169551/tinto-agent-linux-x86_64/tinto-agent-linux-x86_64`
    sha256 `60e596fc0ab3b0bcb944bf388772dd69080fc37c1655cd39559863b00fce0bcc`.
- Host WSL evidence: `wsl.exe -l -v` reported `Ubuntu-24.04` running on WSL version `2`.
  The checklist name `Ubuntu` was adapted to the actual distro name for this host.
- MSI administrative extraction succeeded. Extracted image contained
  `PFiles/Tinto/tinto-agent-linux-x86_64` and `PFiles/Tinto/tinto-agent.exe`; the extracted
  Linux agent matched the downloaded `tinto-agent-linux-x86_64` byte-for-byte.
- Compatible handshake passed:
  `{"type":"handshake","protocol_version":1,"agent_version":"0.1.0","status":"ok"}`.
- Incompatible protocol failed safely with `protocol_mismatch: version de protocolo incompatible`.
- Installed-path check passed after placing the packaged agent at
  `$HOME/.local/share/tinto/agents/0.1.0/tinto-agent`: `test -x ... && echo ok`, followed by a
  successful compatible handshake.
- Agent repo operations passed on temporary Linux repo `/tmp/tinto-agent-smoke-28082169551`:
  `repo_tree`, `file_content`, and `repo_snapshot_with_fs_events` returned expected tree/content,
  clean repo status, and file fingerprints.
- Agent file operations passed on the same repo: `copy_within_repo`, `delete_from_repo`,
  `restore_deleted_from_repo`, and `redo_deleted_from_repo`.
- Agent checkpoint smoke passed: `agent_checkpoint_create` produced a `git_ref` checkpoint,
  `agent_checkpoint_scan` reported `README.md` modified and `new-file.txt` created, and
  `agent_checkpoint_revert` restored `README.md` and removed `new-file.txt`.
- Extracted Windows executable started and stayed running for 5 seconds from PowerShell
  (`running:<pid>`), then was stopped by the smoke harness.

Pending interactive UI evidence:

- Launch the packaged Tinto UI and add one Windows repo plus one `Ubuntu-24.04` WSL repo to the
  same workbench.
- Confirm the WSL repo tree, status, diff, log, and file content through the Tinto UI.
- Confirm drag/paste/delete/restore/redo/export from the UI for both Windows and WSL repos.
- Confirm Agent Console start/stop/change log/revert through the Tinto UI.
