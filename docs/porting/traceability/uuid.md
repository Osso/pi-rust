# utils/uuid traceability

Pinned source: `packages/ai/src/utils/uuid.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `fillRandomBytes` | Web Crypto when available, otherwise `Math.random`. | `getrandom` OS bytes; failure panics instead of falling back to a weak source. |
| Module state `lastTimestamp` / `sequence` | Module-global monotonic state: a newer millisecond reseeds the sequence from random bytes 6-9; otherwise the sequence increments and a wrap bumps the timestamp. | `UuidV7Generator` behind a `Mutex`; exact TS vectors in `matches_the_typescript_rfc9562_layout_and_monotonic_order`, plus `restarts_the_sequence_when_the_clock_advances`. |
| `uuidv7()` | RFC 9562 layout: 48-bit big-endian timestamp, version `7`, variant `10`, sequence split across bytes 6-10, random tail. | `uuidv7`; integration tests `generates_rfc9562_version_7_uuids_with_a_current_timestamp`, `preserves_monotonic_order_across_calls`, `generates_distinct_uuids`. |
| Differential oracle | 200 TypeScript-generated UUIDs with before/after timestamps. | `tests/differential_uuid.rs` checks layout, version/variant, timestamp window, and strict monotonic order. |

## Exclusions and deviations

- `Date.now()` is integer milliseconds; Rust uses `i64` epoch milliseconds.
- Random bytes are not compared (non-deterministic); the exact sequence algorithm is proven by the ported deterministic vectors, and the oracle proves layout/monotonicity.
- The `Math.random` fallback is intentionally removed: Rust always uses the OS random source.
- No source bug found.
