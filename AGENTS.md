## Simplicity and Engineering Discipline

Prefer the simplest implementation that fully satisfies the current requirements.

Follow these principles:

* **YAGNI — You Aren’t Gonna Need It:** Do not implement features, abstractions, configuration options, extension points, or infrastructure based only on possible future needs.
* **KISS — Keep It Simple:** Choose clear, direct, and easily maintainable solutions over clever or unnecessarily sophisticated ones.
* **DRY — Don’t Repeat Yourself:** Avoid meaningful duplication of business rules or complex logic, but do not create premature abstractions merely to eliminate a few similar lines.
* **Avoid overengineering:** Do not introduce extra layers, patterns, dependencies, services, factories, wrappers, or generic frameworks unless they solve a concrete problem in the current task.

Before adding complexity, ask:

1. Is this required by the current acceptance criteria?
2. Does it solve a real, demonstrated problem?
3. Is there a simpler implementation?
4. Will the abstraction make the code easier to understand and maintain today?

Prefer:

* explicit code over hidden magic;
* small, focused changes over broad refactors;
* existing project patterns over new architectural approaches;
* readable duplication over a poorly justified abstraction;
* standard library or existing dependencies over adding new packages.

Do not optimize prematurely. Add abstractions, optimizations, and extensibility only when supported by concrete requirements, repeated use cases, profiling data, or an established project convention.

## Tests: proportional to the change

The suite is slow (each Delivery test builds a git repo and runs real processes), so every test has to earn its place. Before adding or keeping a test, triage it:

* **Write** tests that fail without the change, one per acceptance criterion, plus a few that protect a specific risk the change introduces.
* **Use the lowest layer that proves it:** a pure function or module test (`#[cfg(test)]` in the module, `taskStatus.test.ts`) before an end-to-end one in `delivery/tests.rs` or a panel render.
* **Do not duplicate coverage:** if another test already proves the behaviour through the same path, cite it instead. A module test plus an end-to-end test of the same rule is one too many.
* **Parameterize variants** (fields, modes, callers sharing one implementation) instead of copying a test per variant: every mode on one caller, one representative case per other caller.
* **Do not mirror the implementation:** no assertions that only restate code (exact internal strings, call orders) without protecting a behaviour.
* **Do not fake what the test environment cannot see** (layout, real rendering): make it a QA scenario with a measurable result.
* **Wait on conditions, not time:** poll for the state you expect; no fixed sleeps or real-time `sleep=N` jobs unless the timing itself is the behaviour.

A test suite much larger than the production diff needs a stated reason. When a change makes an older test redundant, delete it in the same change.
