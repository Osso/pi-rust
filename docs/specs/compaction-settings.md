# Compaction settings

This specification covers the public compaction threshold value and predicate from `packages/agent/src/harness/compaction/compaction.ts:152-159,251-254`. It defines the Rust API and behavioral contract; implementation details belong in the linked [compaction settings notes](../wiki/systems/compaction-settings.md).

## What it must do

- [x] Expose mutable `CompactionSettings` fields `enabled: bool`, `reserve_tokens: f64`, and `keep_recent_tokens: f64` through `pi_agent_core::harness::compaction`.
- [x] Re-export `CompactionSettings` and `should_compact` from `pi_agent_core::harness`.
- [x] Return `false` immediately when `enabled` is `false`.
- [x] Otherwise return the strict comparison `context_tokens > context_window - reserve_tokens`.
- [x] Preserve public field mutation and unrestricted Rust-representable binary64 values without validation or normalization.
- [ ] Compare the Rust predicate directly with the pinned TypeScript implementation through differential requests `13113..=13120` and strict public-index type proof.

## How it works

- [Compaction settings notes](../wiki/systems/compaction-settings.md)
- [Compaction compatibility matrix](compatibility.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/compaction/mod.rs` — settings value and threshold predicate.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root re-exports.
- `crates/pi-agent-core/tests/compaction_settings.rs` — focused value and predicate proof.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/compaction_settings.rs`
- `crates/pi-agent-core/tests/differential_truncate/compaction_settings.rs` — pending.
- `oracle/public-index-typecheck.mjs` — pending type proof extension.

## Known gaps (current cycle)

- [ ] Add direct TypeScript differential requests `13113..=13120`.
- [ ] Extend strict public-index proof for the mutable interface and predicate.
- [ ] Complete final workspace, readability, documentation, clean-pin, and inherited verification gates.

## Out of scope

`DEFAULT_COMPACTION_SETTINGS` and its mutable object identity, every other compaction function or type, constructors, serialization, validation, normalization, model calls, token estimation, retries, session mutation, invalid JavaScript runtime values, and dependencies remain deferred.
