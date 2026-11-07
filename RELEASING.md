# Releasing Tinto

A release is a version tag. The [Release workflow](.github/workflows/release.yml)
builds the Windows installer for a tag on a clean runner into a draft GitHub
Release, checks its update, and publishes it: every installed Tinto then offers
the signed update on its next start.

## Cut a release

1. Merge what ships into `develop` and wait for CI to pass on it.
2. Pick the kind ([semver](https://semver.org)): `patch` for fixes, `minor`
   for features, `major` for changes that break how people use Tinto. The
   version is the next one after the latest stable `vX.Y.Z` tag. A release
   candidate (`rc`: `1.3.0-rc.1`, `-rc.2`, …) is published as a prerelease
   and no installed Tinto updates to it.
3. In the repo's Actions tab, run **Release cut**
   ([release-cut.yml](.github/workflows/release-cut.yml)) with `kind`, `rc`
   and `ref` (default `develop`), **dry run** checked. It refuses unless
   CI's latest push run on that commit passed, the version is newer than every
   tag and the app's version, and its tag does not exist yet; the log shows the
   version, the files it changes and the CHANGELOG.md section it would add.
   The same plan, locally:

   ```bash
   node scripts/release.mjs --kind patch --dry-run
   ```

4. Read that section. Commits that are not `feat`, `fix` or `perf` leave
   it empty; to word it yourself, commit a `## [1.3.0] - YYYY-MM-DD` section
   to CHANGELOG.md on `develop` first: a section already there is kept.
5. Run **Release cut** again with dry run unchecked. It sets the version in
   `package.json`, `package-lock.json`, `src-tauri/tauri.conf.json`,
   `Cargo.toml` and `Cargo.lock`, adds the section, commits
   `chore(release): v1.3.0` on the ref, tags `v1.3.0` and pushes both at
   once (nothing is pushed if the branch moved meanwhile). A branch protection
   rule that blocks `github-actions[bot]` from pushing to `develop` makes
   this step fail; allow it or cut by hand (below).
6. It then calls **Release**. A tag pushed with the workflow's own
   `GITHUB_TOKEN` triggers no other workflow, so Release is called as a
   reusable workflow instead of waiting for the tag push. Release checks that
   the tag matches the app's version, runs lint, the frontend tests and
   clippy, builds into a draft, checks the update and publishes. Follow it in
   the Actions tab. It builds with the Rust version pinned in `release.yml`
   (`dtolnay/rust-toolchain@…`); raise the pin when you update Rust.

**Release cut** runs the workflow file of the branch you pick under *Use
workflow from*; GitHub lists a manually run workflow only once its file is on
the default branch.

### By hand

`node scripts/release.mjs 1.3.0` (or `--kind minor`) does step 5 locally:
it bumps, adds the section, commits and tags, without pushing. Check it, then
`git push origin HEAD v1.3.0`: the tag push starts Release the same way.

## The update check

Before the draft is published, `scripts/check-update.mjs` reads what an
installed Tinto will read and fails the release if:

- `tauri.conf.json`'s updater endpoint is not
  `https://github.com/<repo>/releases/latest/download/latest.json`;
- `latest.json` is not this tag's version, or not newer than the published
  release's `latest.json`;
- it has no `windows-x86_64` platform;
- a platform's URL is not an asset of this release;
- a platform's signature does not verify, with the `pubkey` in
  `tauri.conf.json`, over the uploaded installer's bytes (minisign Ed25519,
  as the updater checks it).

It does not install the previous release and let it update itself: the update
is offered in Tinto's window and installed by the NSIS installer, which a
headless runner cannot drive reliably, and a draft's assets cannot be
downloaded without a token, as installed Tinto would. After a release, the
manual check is to start an older Tinto and accept the update.

A failed check leaves the draft release, the tag and the release commit in
place, and nothing published. If the cause was the runner or a secret, fix it
and use *Re-run failed jobs* on the Release run. If it needs a code change,
delete the draft, keep the tag (the next cut counts from it, so `v1.3.0` is
skipped, never published) and cut again once the fix is on `develop`.

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
