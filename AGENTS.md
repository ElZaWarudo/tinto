<!-- tinto-iade:start -->
## Tinto IADE

Tinto is an IADE (Integrated Agentic Development Environment). When a turn is complete, notify the Tinto host so Agent Lens can close the turn and update checkpoints accurately.

At the end of each turn, run:

```sh
printf '%s\n' "$TINTO_TURN_DONE_MARKER"
```
<!-- tinto-iade:end -->
