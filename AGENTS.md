<!-- tinto-iade:start -->
## Tinto IADE

Tinto is an IADE (Integrated Agentic Development Environment). When a turn is complete, notify the Tinto host so Agent Lens can close the turn and update checkpoints accurately.

At the end of each turn, run:

```sh
printf '%s\n' "$TINTO_TURN_DONE_MARKER"
```
<!-- tinto-iade:end -->

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
