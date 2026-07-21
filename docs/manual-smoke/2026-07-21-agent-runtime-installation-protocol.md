# Agent runtime installation protocol smoke

Date: 2026-07-21  
Scope: RDM-023 preview, consent, fake execution and optional separately authorized real validation.

## Safety boundary

The default smoke must not install, upgrade or remove any real package. Use automated fake-runner coverage and the preview/cancel UI path. A real installation is a separate destructive-to-environment action and requires explicit consent naming the provider and target runtime immediately before running it.

Tinto ships no elevated recipe. Missing Node/npm, an unverifiable Windows `node.exe`/`npm-cli.js` pair, an unwritable npm global prefix or an unsupported provider/runtime must stop with manual guidance. Do not substitute `sudo`, `runas`, `cmd.exe`, PowerShell expressions, shell pipelines or a remote install script.

## Automated evidence

1. Run focused backend tests:

   ```text
   cargo test --manifest-path src-tauri/Cargo.toml agent_console::install::tests --no-fail-fast
   ```

2. Run the focused launcher and contract tests:

   ```text
   npm test -- src/panels/RepoCard.test.tsx src/bus/contract.test.ts --run
   ```

3. Confirm the fake cases cover:

   - all four compiled package recipes and exact argv;
   - Windows `node.exe` plus associated `npm-cli.js`, never `.cmd`/`.ps1` execution;
   - local versus named WSL targeting;
   - one claim per attempt, replay rejection, expiry and cancellation;
   - timeout, output caps, redaction and descendant cleanup;
   - verification before one backend-owned session start;
   - confirmation, decline, accessible dialog/status and cache invalidation.

## Preview-and-cancel smoke

1. Start Tinto with a registered repository whose selected provider is intentionally unavailable in its exact runtime.
2. Select the provider. Confirm the launcher names the missing provider and exact host or WSL distro.
3. Select **Instalar**. Confirm the dialog shows provider, runtime, official npm source, exact semantic command, global effect, recipe revision and **Sin privilegios elevados**.
4. Select **Cancelar** or press Escape. Confirm the dialog closes, no session appears and a subsequent preview requires a new attempt.
5. Change provider or repository runtime while a preview is open. Confirm the preview is invalidated and cannot be confirmed for the new selection.

## Optional real validation — separate consent required

Do not perform this section under the roadmap authorization alone. Before it begins, obtain explicit user approval for one named provider and one named local/WSL runtime. Prefer a disposable OS user or disposable WSL distribution with no provider installed.

After approval:

1. Record the provider, runtime and existing `node --version` / `npm --version` without capturing environment values.
2. Review and confirm the displayed recipe.
3. Observe bounded progress. Do not copy raw installer output into project artifacts.
4. Confirm Tinto reports a verified version and creates exactly one session.
5. Stop the session. Removal or rollback is out of scope and must follow a separately approved manual procedure.

## Expected limitations

- The feature does not install Node/npm, Git, credentials or provider login state.
- The feature does not elevate privileges or repair npm prefix permissions.
- Provider installation guidance can change. Before encoding a recipe revision, re-check the official Anthropic, OpenAI, Kimi and OpenCode sources cited in the implementation plan.
