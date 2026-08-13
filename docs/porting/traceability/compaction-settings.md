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
- Focused GREEN: `/tmp/pi-rust-compaction-settings-green.out` records exit 0 with 4 passed and 0 failed; production commit is `64fba3d`, with finite-cancellation coverage in `2c7cf52`.
- Differential RED: `/tmp/pi-rust-compaction-settings-differential-red.out` records exit 101 when the unchanged oracle emitted non-JSON for the unsupported operation after prior requests completed.
- Differential GREEN: `/tmp/pi-rust-compaction-settings-differential-green.out` records exit 0 with the persistent test passing exactly eight request groups (`13113..=13120`).
- Oracle/public-index typecheck: `/tmp/pi-rust-compaction-settings-oracle.out` records exit 0 with exact mutable-interface and predicate-call proof.
- Initial Rust/TypeScript/JavaScript readability audit reported no new slice violations; inherited artifact verification then found `differential_truncate::run` at 32 body lines after registration, above the 30-line threshold.
- Readability correction commit `54b68e2` (`test(agent): extract differential oracle setup`) changed only `crates/pi-agent-core/tests/differential_truncate/mod.rs`; `spawn_oracle` is 13 lines and `run` is 22 lines, with source paths and comparison order preserved.
- Post-fix gates: 230 workspace tests passed with 0 failures and 2 ignored; all-target check exit 0; all-feature Clippy with `-D warnings` exit 0; oracle/typecheck, fmt, and diff exit 0; readability clean.
- Final oracle/typecheck: `/tmp/pi-rust-compaction-settings-final-oracle.out` records exit 0; exact Rust/source heads and clean worktrees are verified.
- Differential/proof commit: `a26bd35` (`test(agent): compare compaction settings predicate`); readability correction commit: `54b68e2`.
- Inherited final verification at `54b68e2`: `OVERALL: PASS`.

## Exclusions

`DEFAULT_COMPACTION_SETTINGS`, mutable default-object identity, all adjacent compaction functions and types, constructors, serialization, validation, normalization, model calls, token budgets, retries, abort execution, provider integration, session mutation, and unrelated messages remain deferred.
