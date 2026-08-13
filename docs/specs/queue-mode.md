# Queue mode

This specification covers the typed `QueueMode` union from `packages/agent/src/types.ts:44-50` and its public package export through `packages/agent/src/index.ts:145`. Queue draining behavior remains deferred.

## What it must do

- [x] Expose a closed public enum with exactly `All` and `OneAtATime` variants.
- [x] Derive exactly `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
- [x] Return the exact source literals `"all"` and `"one-at-a-time"` from `as_str`.
- [x] Re-export `QueueMode` from `pi_agent_core::harness` without a crate-root direct re-export.
- [x] Cover both variants, literals, equality, copying, Debug output, and the harness-root import in focused behavioral tests.
- [x] Compare both source literals through one persistent source-shaped plain-value differential group (`13164`); the Rust side normalizes `QueueMode::as_str`, and the oracle returns final literals only.
- [x] Keep the runtime final-literal differential proof separate from the strict package-index exact-union typeproof.
- [x] Prove the package-index `QueueMode` union is exactly `"all" | "one-at-a-time"`, including both valid assignments and rejected empty/alternate literals.

## How it works

- [Queue mode traceability](../porting/traceability/queue-mode.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — closed enum and literal mapping.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root re-export.
- `crates/pi-agent-core/tests/queue_mode.rs` — focused behavioral/API proof.
- `crates/pi-agent-core/tests/differential_truncate/queue_mode.rs` — one persistent plain-value differential group.
- `oracle/truncate-oracle.ts` — plain-value oracle normalization only.
- `oracle/public-index-typecheck.mjs` — strict package-index type proof.
- `run-tests.sh` — focused and oracle registrations.

## Tests asserting this spec

- `crates/pi-agent-core/tests/queue_mode.rs`
- `crates/pi-agent-core/tests/differential_truncate/queue_mode.rs`
- `oracle/public-index-typecheck.mjs`

## Final integration proof

- [x] Run `cargo test --workspace --all-targets`: 257 passed, 0 failed, and 2 ignored at code/proof HEAD `7b0358e`.
- [x] Run workspace all-target check and all-target/all-feature Clippy with `-D warnings`: both exit 0.
- [x] Run oracle/typeproof, formatting, Node syntax, and diff checks: all exit 0.
- [x] Complete Rust/TypeScript/JavaScript readability, documentation, dependency, and clean-pin audits.

## Known gaps

- [ ] Inherited independent verification remains pending after documentation closure.

## Out of scope

Queue draining, parsing, `FromStr`, `Display`, serde, validation, `ToolExecutionMode`, dependencies, runtime constructors, and adjacent queue APIs remain deferred.
