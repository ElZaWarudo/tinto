# Root-direct WSL diagnostic repair

The initial worker candidate is rejected, preserved outside consolidation.
Independent review found an introduced successful-response race and late stderr
draining. It also lacked meaningful pooled/reader coverage.

Root owns only src-tauri/src/wsl_agent/launcher.rs in fresh wsl-root at source
59f0cecafb41c08db6a4c18001fa24131da7003e, index
e2541e203c7cf504d4a5e8f888dc48da89db8f1a, no dependency patches.
Route root-direct: one now-decision-closed diagnostic implementation; independent
concurrency/security review remains required.

Preserve original success, exit ordering, cleanup, retries and unknown-send
behavior. Start continuously draining bounded stderr immediately after spawn,
before stdin writes. Keep at most 4096 tail bytes in memory and expose only
fixed diagnostic labels, truncation and observable exit code. Never emit raw
stderr, paths, request content or secrets. Collection is best-effort/nonblocking
on errors, never waits for EOF/descendants. Unknown status/cause remains explicit.
Do not repair unrelated baseline waits or introduce new termination policy.

Add tests for bounded full draining, static-label non-disclosure, pooled error
augmentation and existing one-shot success behavior. Run explicit installed MSVC
focused test command; root captures exits. Budget one implementation, one fix
round, one review. Native cause and recovered ICook execution remain blocked by
the supported integration prerequisite.
