---
title: Windows Ubuntu WSL agent bootstrap manual smoke
status: passed-with-noted-agent-console-gap
date: 2026-06-24
roadmap_item: RDM-002/RDM-006
---

# Windows Ubuntu WSL Agent Bootstrap Manual Smoke

## Scope

Validate the Windows host to Ubuntu-family WSL `tinto-agent` bootstrap on a real Windows host.
RDM-006 makes packaged Linux agent discovery/install the primary path and keeps the RDM-002
dev-source command as an explicit fallback. This smoke also confirms local and WSL repos can coexist
in one workbench before final release.

## Preconditions

- Windows host with WSL 2 enabled.
- Ubuntu-family distro installed and visible in `wsl.exe -l -v` (`Ubuntu`, `Ubuntu-24.04`,
  `Ubuntu-22.04`, or `Ubuntu-20.04`).
- A packaged Tinto Windows build produced after CI downloaded `tinto-agent-linux-x86_64` into
  `src-tauri/resources/`. For `Ubuntu-24.04` UI smoke, use CI run `28087369767` or newer; earlier
  artifacts exposed fixed issues in distro routing, app-binary selection, and WSL install shell
  variable escaping.
- Alternatively, a Linux `tinto-agent` binary available on the Windows host for the explicit
  override path.
- Optional fallback only: Tinto repository and Rust/Cargo available inside Ubuntu.

## Steps

1. From Windows PowerShell, confirm the target Ubuntu-family distro is WSL 2:

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

3. Launch Tinto and add one Windows repo plus one Ubuntu-family WSL repo (`/home/...`) to the same
   workbench. Select the actual distro name reported by `wsl.exe -l -v`; on the current smoke host
   this is `Ubuntu-24.04`.

4. Confirm the WSL repo reaches a non-terminal state: tree, status, diff, log, and file content load without requiring the source checkout inside Ubuntu.

5. In the WSL repo, drag or paste a small file into the Explorer, then delete, restore, redo, and export it. Confirm the same operations still work on the Windows repo.

6. Start and stop an Agent Console session for the WSL repo. Create or modify a test file during the session, stop the session, confirm a change log is shown, then use Revert with explicit consent and confirm the WSL repo returns to the checkpoint state.

7. Confirm the agent was installed in Ubuntu:

   ```powershell
   wsl.exe -d Ubuntu-24.04 -- sh -lc 'test -x "$HOME/.local/share/tinto/agents/0.1.0/tinto-agent" && echo ok'
   ```

8. Confirm compatible handshake with the installed agent:

   ```powershell
   '{"type":"handshake","protocol_version":1,"client_version":"manual-smoke"}' |
     wsl.exe -d Ubuntu-24.04 -- sh -lc 'exec "$HOME/.local/share/tinto/agents/0.1.0/tinto-agent"'
   ```

9. Confirm incompatible protocol fails safely:

   ```powershell
   '{"type":"handshake","protocol_version":999,"client_version":"manual-smoke"}' |
     wsl.exe -d Ubuntu-24.04 -- sh -lc 'exec "$HOME/.local/share/tinto/agents/0.1.0/tinto-agent"'
   ```

10. Optional development fallback: unset the packaged binary and explicitly enable fallback:

   ```powershell
   Remove-Item Env:\TINTO_WSL_AGENT_LINUX_BIN -ErrorAction SilentlyContinue
   $env:TINTO_WSL_AGENT_ALLOW_DEV_SOURCE="1"
   '{"type":"handshake","protocol_version":1,"client_version":"manual-smoke"}' |
     wsl.exe -d Ubuntu-24.04 -- cargo run --manifest-path /path/to/tinto/src-tauri/Cargo.toml --bin tinto-agent
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
- Local Windows and selected Ubuntu-family WSL repos can be tracked together.
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

Final packaged UI/backend smoke passed on 2026-06-24 against CI run `28087369767` and commit
`3a994efce47ba2ff27349ac31ac6a838456ec9ac`.

- GitHub Actions CI run `28087369767` passed: Frontend, Rust, Linux Tauri bundle, and Windows Tauri
  bundle.
- Windows bundle log confirmed the packaged application binary is
  `D:\a\tinto\tinto\src-tauri\target\release\tinto.exe`. Earlier run `28084574214` exposed that
  Tauri had packaged `tinto-agent.exe`; fixed by `46f0d4d`.
- Downloaded final artifacts:
  - `.ci-artifacts/28087369767/tinto-windows-bundle/nsis/Tinto_0.1.0_x64-setup.exe`
    sha256 `a7cb1f6d750ec6b8c9d30f3acf48a41bb8ce9086fca4acefdfc64c2e48c7515c`.
  - `.ci-artifacts/28087369767/tinto-windows-bundle/msi/Tinto_0.1.0_x64_en-US.msi`
    sha256 `7948185c36175a4c5e3b73224f8edc645bb5f25a65b6ebbfaaed5734ffe73c0d`.
  - `.ci-artifacts/28087369767/tinto-agent-linux-x86_64/tinto-agent-linux-x86_64`
    sha256 `c22bbd71c00918afd9256f4015376869365cf53804a218d4fe4418d816e9a140`.
- MSI administrative extraction succeeded. Extracted image contained
  `PFiles/Tinto/tinto.exe`, `PFiles/Tinto/tinto-agent.exe`, and
  `PFiles/Tinto/tinto-agent-linux-x86_64`; the extracted Linux agent matched the downloaded
  `tinto-agent-linux-x86_64` byte-for-byte.
- Extracted `tinto.exe` launched and responded with main window title `Tinto`.
- Packaged UI smoke used a temporary workbench with one Windows repo and one `Ubuntu-24.04` WSL repo.
  Window capture:
  `C:\Users\User\AppData\Local\Temp\tinto-ui-smoke-final-28087369767.png`.
- The packaged UI showed both repos in a healthy non-terminal Dashboard state:
  `master`, `no upstream`, `1M 0S 1U`, and the expected initial commits for both Windows and WSL
  repos.
- During the first packaged UI smoke, the WSL repo failed with
  `no se pudo instalar tinto-agent dentro de Ubuntu WSL`. Reproduction showed `wsl.exe` launched from
  a Windows process consumed unescaped shell variables in the `sh -lc` script. Fixed by `3a994ef`
  (`fix(wsl): preserve shell vars during agent install`) and verified in run `28087369767`.
- Final agent install command with escaped shell variables succeeded through `wsl.exe` and installed
  the final Linux agent in `Ubuntu-24.04`.
- Installed final agent handshake passed:
  `{"type":"handshake","protocol_version":1,"agent_version":"0.1.0","status":"ok"}`.
- Final WSL backend operation smoke passed on `/tmp/tinto-ui-smoke-wsl-28085823633`: `repo_snapshot`
  returned `modified=["changed.txt"]` and `untracked=["untracked.txt"]`; `repo_tree` returned
  `README.md`, `changed.txt`, and `untracked.txt`; `file_content` returned the expected README text;
  `worktree_diff` returned the added `modified` line; `commit_log` returned `initial wsl smoke`; and
  `delete_from_repo`, `restore_deleted_from_repo`, `redo_deleted_from_repo`, then restore again all
  returned successfully.

Remaining honest gap:

- Agent Console start/stop/change-log/revert was not completed interactively in the packaged UI on
  this host because Codex could not drive the native desktop reliably beyond window capture and
  coordinate smoke. Backend agent checkpoint/file-operation parity had passed earlier agent-level
  smoke; the remaining gap is explicit UI operation evidence for Agent Console.
