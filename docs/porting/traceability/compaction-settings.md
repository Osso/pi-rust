# Compaction settings traceability

Pinned source: `packages/agent/src/harness/compaction/compaction.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `compaction.ts:152-159` | Public mutable interface with required `enabled: boolean`, `reserveTokens: number`, and `keepRecentTokens: number` fields | `harness::compaction::CompactionSettings` with public `bool`, `f64`, and `f64` fields; focused mutation and binary64-bit tests; eight direct differential request groups (`13113..=13120`) and strict public-index proof |
| `compaction.ts:251-254` | Disabled settings return `false`; enabled settings use strict `contextTokens > contextWindow - settings.reserveTokens` | `harness::compaction::should_compact`; focused boundary and IEEE-754 tests; eight direct differential request groups (`13113..=13120`) |
| `index.ts:58-75` | Package index exports `CompactionSettings` and `shouldCompact` | Rust exports module and harness-root paths; strict public-index exact-shape, mutability, numeric-boundary, and call proof |

## Rust boundary

- `CompactionSettings` derives `Debug` only and remains publicly mutable.
- TypeScript `number` fields map to unrestricted Rust `f64` values; no validation or normalization is added.
- `keep_recent_tokens` is required by the value contract but is intentionally unobserved by `should_compact`, matching the source predicate.
- Invalid JavaScript runtime shapes and lone-surrogate strings remain outside the typed Rust boundary.

## Proof status

- Focused RED: `/tmp/pi-rust-compaction-settings-red.out` records exit 101 with only unresolved imports for the missing module/root API.
- Focused GREEN: `/tmp/pi-rust-compaction-settings-green.out` records exit 0 with 4 passed and 0 failed after correcting one fixture assertion; production/specification commit is `64fba3d`.
- Differential RED: `/tmp/pi-rust-compaction-settings-differential-red.out` records exit 101 when the unchanged oracle emitted non-JSON for the unsupported operation after prior requests completed.
- Differential GREEN: `/tmp/pi-rust-compaction-settings-differential-green.out` records exit 0 with the persistent test passing exactly eight request groups (`13113..=13120`).
- Oracle/public-index typecheck: `/tmp/pi-rust-compaction-settings-oracle.out` records exit 0 with exact mutable-interface and predicate-call proof.
- Rust/TypeScript/JavaScript readability: targeted audit reports no new violations; remaining reported helper lengths predate this slice.
- Final workspace, documentation, clean-pin, and inherited verification: pending.

## Exclusions

`DEFAULT_COMPACTION_SETTINGS`, mutable default-object identity, all adjacent compaction functions and types, constructors, serialization, validation, normalization, model calls, token budgets, retries, abort execution, provider integration, session mutation, and unrelated messages remain deferred.
