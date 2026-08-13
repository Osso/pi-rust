# Compaction settings

This specification covers the public compaction threshold value and predicate from `packages/agent/src/harness/compaction/compaction.ts:152-159,251-254`. It defines the Rust API and behavioral contract; implementation details belong in the linked [compaction settings notes](../wiki/systems/compaction-settings.md).

## What it must do

- [x] Expose mutable `CompactionSettings` fields `enabled: bool`, `reserve_tokens: f64`, and `keep_recent_tokens: f64` through `pi_agent_core::harness::compaction`.
- [x] Re-export `CompactionSettings` and `should_compact` from `pi_agent_core::harness`.
- [x] Return `false` immediately when `enabled` is `false`.
- [x] Otherwise return the strict comparison `context_tokens > context_window - reserve_tokens`.
- [x] Preserve public field mutation and unrestricted Rust-representable binary64 values without validation or normalization.
- [x] Compare the Rust predicate directly with the pinned TypeScript implementation through exactly eight differential request groups `13113..=13120` and strict public-index type proof.

## How it works

- [Compaction settings notes](../wiki/systems/compaction-settings.md)
- [Compaction compatibility matrix](compatibility.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/compaction/mod.rs` — settings value and threshold predicate.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root re-exports.
- `crates/pi-agent-core/tests/compaction_settings.rs` — focused value and predicate proof.
- `crates/pi-agent-core/tests/differential_truncate/compaction_settings.rs` — eight persistent differential request groups.
- `oracle/truncate-oracle.ts` — pinned TypeScript predicate adapter.
- `oracle/public-index-typecheck.mjs` — exact mutable-interface and public-function proof.
- `run-tests.sh` — focused and oracle test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/compaction_settings.rs`
- `crates/pi-agent-core/tests/differential_truncate/compaction_settings.rs`
- `oracle/public-index-typecheck.mjs`

## Known gaps (current cycle)

- [x] Add direct TypeScript differential requests `13113..=13120`.
- [x] Extend strict public-index proof for the mutable interface and predicate.
- [x] Complete final workspace, readability, documentation, and clean-pin gates: 230 workspace tests passed with 0 failures and 2 ignored; check, denied-warning Clippy, fmt, oracle/typecheck, Node syntax, diff, and exact source pins passed.
- [ ] Complete inherited independent verification; the resumed runtime cannot spawn a supervisor verifier.

## Out of scope

`DEFAULT_COMPACTION_SETTINGS` and its mutable object identity, every other compaction function or type, constructors, serialization, validation, normalization, model calls, token estimation, retries, session mutation, invalid JavaScript runtime values, and dependencies remain deferred.
