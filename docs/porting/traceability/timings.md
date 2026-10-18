# coding-agent timings traceability

Pinned source: `packages/coding-agent/src/core/timings.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: focused child-process tests in `crates/pi-coding-agent/tests/timings.rs` capture the real stderr of a probe process with `PI_TIMING` set and unset. Differential testing is not applicable: output embeds wall-clock milliseconds.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `ENABLED` | `PI_TIMING === "1"`, read once at module load | `enabled()` reads the variable once via `OnceLock` |
| `TimingLabel` | `main \| extensions` | `enum TimingLabel` with `as_str` |
| `resetTimings` | No-op unless enabled; replaces the namespace and sets `lastTime` | `reset_timings`; `reset_namespace` |
| `time` | No-op unless enabled; lazily resets a missing namespace, records `now - lastTime`, updates `lastTime` | `time`; probe asserts the first label is `0ms` and the second measures the sleep |
| `printTimingGroup` | Skips non-positive deltas, prints header, per-label lines, total, and a `-` rule of `title.length + 8` | `format_timing_group`; regex asserts the full block including both rules (35 and 29 dashes) |
| `printTimings` | No-op unless enabled; iterates namespaces in insertion order | `print_timings` over an insertion-ordered `Vec`; probe asserts extensions precede main |

## Exclusions

- Namespaces live in one `Mutex<Vec<...>>` instead of a `Map`; iteration order matches JavaScript `Map` insertion order.
- Timestamps use `SystemTime` milliseconds since the epoch instead of `Date.now()`.

## Discrepancies

None.
