# Retry delay

Baseline: frozen TS `4f9e6f8a4`, `coding-agent/src/core/retry-delay.ts`.

- Zero base delay returns zero without consulting jitter or attempt.
- Otherwise multiply base by 2^(attempt−1), cap the exponential term, apply symmetric 20% jitter, round as JavaScript Math.round, then apply the cap again.
- Session retries use the configured base/cap and a fresh random value; retry-start events expose the actual scheduled delay.
- Invalid retry/compaction settings propagate through the invoking operation; detached background-install failures are logged at their source-equivalent catch. Invalid compaction percentage is rejected at the prepare-next-turn boundary after an assistant response, before the next provider request. This is not an unconditional fresh initial-request preflight.

Existing full agent-session parity, percentage-trigger scheduling and idle-compaction behavior remain separate pending work; helper or validation proof does not establish those consumers.
