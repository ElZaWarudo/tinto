# Rejected WSL worker candidate

Reviewer /root/wsl_review returned a failed correctness and Security Sentinel
assessment for contract
sha256:89f1044e61fea4f661630f67e2a489a87fe9d56ba8fd53e7543603df11002dd4,
diff sha256:1a40a0474bb47a532c3d3681864a271747871ffe5a2b1c6e87a6c52df56a279d.
Its response used an implementation-terminal envelope, not the requested review
certificate. It is not accepted certification.

- P1: one-shot try_wait could observe successful exit before the stdout reader
  delivered its response, causing valid response loss (candidate lines 1082–1093).
- P1: stderr draining started after synchronous stdin write, creating a new
  pipe-blocking path (candidate lines 997–1024).
- P2: pooled timeout did not observe process status.
- Missing tests: real capture bounds/draining, pooled EOF and timeout.

Root independently confirmed the first two regressions. Candidate patch,
manifest and worktrees are preserved but excluded from integration.
The source-baseline root-direct replacement preserves original control flow;
its separate contract, patch, root tests and new review govern acceptance.
