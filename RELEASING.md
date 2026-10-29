# Releasing Tinto

A release is a version tag. Pushing `vX.Y.Z` makes the
[Release workflow](.github/workflows/release.yml) build the Windows installer
on a clean runner and publish it as that version's GitHub Release, with the
signed update that every installed Tinto offers on its next start.

## Cut a release

1. Merge what ships into `develop` and pull it.
2. Pick the version ([semver](https://semver.org)): `PATCH` for fixes, `MINOR`
   for features, `MAJOR` for changes that break how people use Tinto. A
   prerelease (`1.2.0-beta.1`) is published as one and no installed Tinto
   updates to it.
3. Run:

   ```bash
   node scripts/release.mjs 1.2.0
   ```

   It sets the version in `package.json`, `package-lock.json`,
   `src-tauri/tauri.conf.json`, `Cargo.toml` and `Cargo.lock`, adds the
   version's section to [CHANGELOG.md](CHANGELOG.md) from the `feat`, `fix`
   and `perf` commits since the last tag, commits and tags `v1.2.0`. A section
   already written by hand is kept as it is.

4. Read the new CHANGELOG.md section; fix it with `git commit --amend` and
   `git tag -f -a v1.2.0 -m "Tinto 1.2.0"` if needed.
5. Publish:

   ```bash
   git push origin HEAD v1.2.0
   ```

   The workflow checks that the tag matches the app's version, runs lint, the
   frontend tests and clippy, then builds and publishes. Follow it in the
   repo's Actions tab. It builds with the Rust version pinned in
   `release.yml` (`dtolnay/rust-toolchain@…`): run clippy locally with that
   same version before tagging, or a newer lint fails the release; raise the
   pin when you update Rust.

## Secrets

Set in the repo's Settings → Secrets and variables → Actions:

| Secret | What it is |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Signs each update. Its public key is in `tauri.conf.json` (`plugins.updater.pubkey`): Tinto installs only updates it signed. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The key's password; unset for a key without one. |
| `WINDOWS_CERTIFICATE` | Optional: a code-signing certificate (`.pfx`, base64). Without it the installer is unsigned and Windows SmartScreen warns about it. |
| `WINDOWS_CERTIFICATE_PASSWORD` | The certificate's password. |

The update key lives outside the repo, in `~/.tauri/tinto-updater.key`. Keep a
copy somewhere safe (a password manager): **if it is lost, installed Tinto
cannot update any more** and has to be reinstalled by hand with a new key.

## Local builds

`npm run tauri build` still builds an installer for your own machine, without
update files or a version change. It is not a release: nothing records which
commit it came from.
