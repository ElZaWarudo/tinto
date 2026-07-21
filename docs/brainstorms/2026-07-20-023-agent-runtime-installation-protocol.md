---
title: Agent runtime installation protocol requirements
status: planning-input-review-passed
date: 2026-07-20
roadmap_item: RDM-023
artifact_kind: requirements-brainstorm
planning_input: true
source_docs:
  - docs/brainstorms/2026-06-23-002-wsl-agent-bootstrap-protocol.md
  - docs/plans/2026-07-18-022-feat-kimi-opencode-agent-support-plan.md
  - src-tauri/src/agent_console/commands.rs
  - src-tauri/src/agent_console/validation.rs
  - src-tauri/src/bus/contract.rs
  - src/panels/agentAvailability.ts
  - src/panels/RepoCard.tsx
---

# Agent runtime installation protocol requirements

## Problem and goal

Tinto can determine whether a supported coding-agent binary is available in the runtime that owns a repository. Today, an unavailable binary prevents the session from starting, but there is no governed path for resolving that condition from Tinto.

The goal is to define a safe, explicit installation protocol. When the user tries to start a supported agent that is absent from the target runtime, Tinto shall explain the condition, request authorization, run the provider's official global installation procedure inside that runtime, verify the result, and continue the original launch only after successful verification.

## Vocabulary

- **Target runtime:** the execution environment that owns the repository and in which the coding agent must run, initially the local host or the selected WSL distribution.
- **Provider:** a supported coding agent: Claude Code, Codex, Kimi Code, or OpenCode.
- **Install recipe:** Tinto-maintained metadata identifying the provider's official installer, prerequisites, argument vector, verification command, and safe display name for a supported runtime.
- **Install authorization:** an explicit user decision approving one displayed install recipe for one provider and target runtime.
- **Elevation authorization:** a separate native operating-system or runtime prompt shown only when the approved global installation requires administrator or root privileges.
- **Verified installation:** an installation whose process succeeded and whose expected executable can subsequently be resolved and minimally probed inside the target runtime.

## Stakeholders and users

- Primary user: a Tinto user starting a coding agent for a registered local or WSL repository.
- Maintainer: the person updating provider-specific install recipes when official distribution methods change.
- Security reviewer: the person validating process execution, elevation, command provenance, output redaction, and failure behavior.

## Confirmed product decisions

- Installation requires explicit user authorization; attempting to start an agent is not itself authorization to install software.
- Tinto installs through the provider's official global installation mechanism in the target runtime.
- The installed agent remains usable outside Tinto.
- If elevated privileges are required, Tinto requests them through the native mechanism, explains why, and permits cancellation.

## Scope in

- Local-host and WSL target runtimes already supported by Tinto.
- The supported provider allowlist: `claude`, `codex`, `kimi`, and `opencode`.
- Detection of an absent provider binary in the target runtime.
- Resolution of a trusted install recipe for the exact provider/runtime pair.
- A confirmation surface that identifies the provider, target runtime, installer, exact command/arguments, expected privilege level, and external effects.
- Execution only after explicit authorization.
- Native elevation when necessary, with a separate cancellable system prompt.
- Bounded progress and safe status reporting while installation runs.
- Post-install executable resolution and minimal version/health verification.
- Automatic continuation of the single originally requested launch after verification succeeds.
- Structured cancellation and failure results that allow a user-directed retry.
- Tests with fake installers and runtime probes; no automated test may perform a real global installation.

## Scope out

- Automatic installation during startup, background discovery, monitoring, or application update.
- Installation of unsupported or arbitrary agent names.
- Arbitrary commands supplied by the user, repository, agent, or remote content.
- Automatic provider upgrades, downgrade selection, uninstall, repair, or dependency-runtime installation in this first slice.
- Installing credentials, accepting provider terms on the user's behalf, or automating provider login.
- Falling back to installation on the host when the repository's target runtime is WSL, or vice versa.
- Starting the requested agent before installation verification succeeds.
- Silent retries, alternative package sources, or unofficial mirrors.

## Protocol

1. **Resolve target:** Tinto resolves the registered repository and its target runtime before probing provider availability.
2. **Probe availability:** Tinto checks for the requested provider executable inside that exact runtime.
3. **Launch if ready:** If the executable is available and usable, Tinto follows the existing launch path and performs no install action.
4. **Resolve recipe:** If the executable is absent, Tinto looks up an allowlisted official install recipe for the provider/runtime pair.
5. **Explain or stop:** If no supported recipe exists or a required prerequisite is missing, Tinto does not execute an inferred command. It reports the blocking condition and safe manual guidance.
6. **Request authorization:** Tinto shows the provider, runtime, official installer/source, exact command and arguments, global-install effect, expected elevation, and cancellation option. The install action remains disabled until this information is available.
7. **Install:** After explicit authorization, Tinto executes the immutable recipe directly inside the target runtime without shell interpolation. Repository content cannot modify the executable, package identifier, arguments, environment allowlist, or elevation decision.
8. **Elevate if required:** Elevation is selected only by the authorized recipe metadata or by an application-controlled preflight performed before installer execution, never by parsing untrusted installer output. Tinto explains the need and invokes the native privilege prompt before executing the installer. If preflight changes the command, arguments, target runtime, privilege level, or recipe revision, Tinto invalidates the prior authorization and presents the revised recipe again. Denial or cancellation ends the protocol without starting the installer or agent.
9. **Report progress:** Tinto exposes bounded, sanitized progress and permits cancellation where the installer/runtime can terminate safely. Raw environment values, credentials, tokens, and unrestricted installer output are not persisted.
10. **Verify:** After a successful installer exit, Tinto invalidates availability caches, resolves the executable again inside the same runtime, and runs a bounded minimal version/health probe.
11. **Continue once:** If verification succeeds, Tinto records success and resumes exactly the original pending launch once. It must not replay stale or duplicate launch requests.
12. **Fail safely:** If installation, elevation, cancellation, timeout, process cleanup, executable resolution, or verification fails, Tinto does not launch the agent. It returns a structured result with a safe explanation and an explicit retry action.

## Functional requirements

- **FR1:** When a user requests an agent launch, the system shall probe that provider in the repository's resolved target runtime.
- **FR2:** When the provider executable is unavailable, the system shall not create an agent session before the installation protocol completes successfully.
- **FR3:** The system shall select installation commands only from an application-controlled allowlist of official provider/runtime recipes.
- **FR4:** Before execution, the system shall display the provider, runtime, installer/source, exact command and arguments, global-install effect, and expected privilege requirement.
- **FR5:** The system shall require explicit authorization tied to the displayed provider, runtime, and recipe revision.
- **FR6:** Changed provider, runtime, command, arguments, source, or recipe revision shall invalidate prior authorization and require confirmation again.
- **FR7:** The system shall execute the approved recipe inside the target runtime without shell interpolation.
- **FR8:** When an authorized recipe or application-controlled preflight determines before execution that global installation requires elevated privileges, the system shall explain the reason and request elevation through the runtime's native mechanism; untrusted installer output shall never authorize or select elevation.
- **FR9:** Denying or cancelling either authorization shall leave the agent unstarted and return control to the launcher without treating cancellation as an application fault.
- **FR10:** The system shall bound installer runtime, captured output, progress events, and shutdown cleanup.
- **FR11:** After installer success, the system shall invalidate cached readiness and verify executable resolution plus a minimal bounded provider probe in the same runtime.
- **FR12:** The system shall resume the original launch exactly once only after verification succeeds.
- **FR13:** A successful install followed by failed verification shall be reported as `verification_failed`, not as a successful installation or running session.
- **FR14:** If no approved recipe exists, a prerequisite is absent, or the platform is unsupported, the system shall provide safe manual guidance without executing guessed remediation.
- **FR15:** The system shall distinguish at least: unsupported recipe, missing prerequisite, authorization declined, elevation declined, installer spawn failure, installer failure, timeout, cancellation, cleanup failure, and verification failure.
- **FR16:** Tests shall exercise the protocol through injected recipes, fake installers, fake elevation, and fake probes without mutating the developer or CI environment.

## Non-functional requirements

- **NFR1 — Security:** Only immutable allowlisted recipes shipped or cryptographically trusted by Tinto may execute; repository and provider output are untrusted input.
- **NFR2 — Consent:** Authorization must be informed, specific, current, and revocable until execution starts. It must not be stored as blanket consent for future installations.
- **NFR3 — Runtime isolation:** All detection, installation, verification, and launch steps must address the same resolved target runtime.
- **NFR4 — Reliability:** Every external process and protocol wait must have a bounded timeout and owned cleanup.
- **NFR5 — Privacy:** Tinto must not persist credentials, environment values, elevation secrets, or unrestricted installer output.
- **NFR6 — Auditability:** Tinto may retain a sanitized local record of provider, runtime identity, recipe revision, timestamps, outcome category, and resolved version; it must not retain secret-bearing command output.
- **NFR7 — Compatibility:** Existing launch behavior for already-installed agents must remain unchanged except for the availability probe already performed by the launcher.
- **NFR8 — Accessibility:** Confirmation, progress, elevation explanation, cancellation, failure, retry, and success states must be keyboard-operable and announced to assistive technology.

## Business rules

- **BR1:** The user's request to start an agent does not authorize software installation.
- **BR2:** Authorization applies to one provider, one target runtime, one displayed recipe revision, and one installation attempt.
- **BR3:** Installation must occur where the agent will run. A WSL repository installs into its selected distribution; a local repository installs on its local host.
- **BR4:** Tinto shall never substitute an unofficial mirror or alternative installer after authorization.
- **BR5:** Tinto shall not install or configure provider authentication credentials.
- **BR6:** Successful process exit is insufficient; post-install verification is mandatory.
- **BR7:** Cancellation is an expected outcome and must not create or mark an agent session as failed.
- **BR8:** A failed attempt requires a new user-directed retry; Tinto shall not retry installation silently.
- **BR9:** If elevation changes the command or target runtime, the original authorization is invalid and Tinto must present the revised recipe again.
- **BR10:** The provider allowlist remains the authority for which agents Tinto may offer to install.

## Acceptance criteria

- **AC1:** Given an already usable provider binary, when the user starts the agent, Tinto launches through the existing path and does not show or execute installation.
- **AC2:** Given an absent binary and an approved recipe, when the confirmation appears, it shows the provider, exact target runtime, official source/installer, exact command and arguments, global-install effect, and privilege expectation.
- **AC3:** Given an absent binary, when the user declines installation, no installer or agent process starts and the launcher returns to a usable state.
- **AC4:** Given authorization, when installation does not need elevation, Tinto executes exactly the displayed allowlisted recipe in the target runtime.
- **AC5:** Given an authorized recipe or application-controlled preflight with a genuine elevation requirement, Tinto explains the reason and opens a separate native privilege prompt before installer execution; declining it starts neither the installer nor the agent, and any changed recipe identity requires a new Tinto confirmation first.
- **AC6:** Given any change to the authorized recipe identity or target runtime, Tinto requires a new confirmation before executing.
- **AC7:** Given installer success and successful post-install verification, Tinto invalidates readiness caches and starts the original requested agent exactly once.
- **AC8:** Given installer success but a missing or unhealthy executable, Tinto reports `verification_failed` and does not start the agent.
- **AC9:** Given a timeout, cancellation, non-zero exit, spawn error, or cleanup failure, Tinto reports the corresponding safe category, does not launch the agent, and offers a user-directed retry where safe.
- **AC10:** Given a WSL repository, all probe, recipe, install, verification, and launch operations target the registered distribution; no step falls back to Windows.
- **AC11:** Given repository-controlled text that resembles commands, package names, paths, or environment assignments, it cannot alter the selected recipe or installation process.
- **AC12:** Automated tests complete without installing, upgrading, or removing any real global package.
- **AC13:** Logs and persisted records contain no credentials, secret environment values, privilege secrets, or unrestricted installer output.
- **AC14:** Existing local and WSL agent launch tests remain green for installed providers.

## Assumptions

- Provider-specific commands and prerequisites belong to implementation planning and must be validated against official provider documentation before a recipe is accepted.
- A provider may require authentication on first launch after installation; that interaction remains owned by the provider session and is not part of installation success.
- Package-manager rollback semantics differ. This first slice guarantees no agent launch after failure and reports any partial-install possibility; it does not promise automatic rollback.
- Automatic update, downgrade, uninstall, repair, and prerequisite installation can be considered separately after this protocol is validated.

## Open questions

- No material product questions remain for requirements validation.
- Planning must enumerate the official recipe and prerequisite matrix for each supported provider on each supported local-host and WSL runtime before implementation begins.

## Validation status

Requirements status: planning-input review passed after reconciling elevation with recipe-bound authorization. No open P0-P2 findings remain.

This is what will be built: a consent-gated, allowlisted global installation flow that operates inside the repository's target runtime, requests native elevation only when required, verifies the installed provider, and resumes the original launch exactly once. Automatic updates, uninstall, credential setup, arbitrary installers, and prerequisite installation remain out of scope.
