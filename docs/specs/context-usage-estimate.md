# Context usage estimate

This specification covers the module-local `ContextUsageEstimate` interface from `packages/agent/src/harness/compaction/compaction.ts:200-208` and its Rust value boundary. Context-token estimation and compaction orchestration remain deferred. Current committed revisions: production `6d51639`; differential and direct-module proof `a4d33ae`.

## What it must do

- [x] Expose a public mutable `ContextUsageEstimate` through `pi_agent_core::harness::compaction`.
- [x] Preserve required fields in source order: `tokens: f64`, `usage_tokens: f64`, `trailing_tokens: f64`, and `last_usage_index: Option<f64>`.
- [x] Derive `Debug` only without constructors, serialization, validation, normalization, or integer narrowing.
- [x] Preserve ordinary values, `None`/`Some` index states, public mutation, signed zero, subnormals, finite extremes, infinities, and distinct NaN payloads in every numeric field.
- [x] Compare final source-shaped plain records through exactly eight pinned differential request groups `13148..=13155`, with binary64 bit normalization and null-preserving optional state.
- [x] Prove the pinned direct-module interface's exact required shape, `number | null` index, public mutation, and unrestricted number assignments without public-index proof or runtime-construction claims.

## How it works

- [Context usage estimate traceability](../porting/traceability/context-usage-estimate.md)
- [Compaction compatibility matrix](compatibility.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/compaction/mod.rs` — module-local value record.
- `crates/pi-agent-core/tests/context_usage_estimate.rs` — focused field, mutation, Debug, optional-state, and binary64 proof.
- `crates/pi-agent-core/tests/differential_truncate/context_usage_estimate.rs` — eight source-shaped differential request groups.
- `oracle/truncate-oracle.ts` — plain-record request adapter and binary64 normalization only.
- `oracle/context-usage-estimate-typecheck.mjs` — standalone pinned direct-module exact-shape and mutability proof.
- `run-tests.sh` — focused, differential, and direct-module oracle registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/context_usage_estimate.rs`
- `crates/pi-agent-core/tests/differential_truncate/context_usage_estimate.rs`
- `oracle/context-usage-estimate-typecheck.mjs`

## Final integration proof

- [x] Run `cargo test --workspace --all-targets`: 252 passed, 0 failed, and 2 ignored at code/proof HEAD `a4d33ae`.
- [x] Run workspace all-target check and all-target/all-feature Clippy with `-D warnings`: both exit 0.
- [x] Run oracle/typeproof, formatting, Node syntax, and diff checks: all exit 0.
- [x] Complete Rust/TypeScript/JavaScript readability, documentation, dependency, and clean-pin audits.

## Known gaps

- [ ] Inherited independent verification remains pending after documentation closure.

## Out of scope

`estimateContextTokens`, `calculateContextTokens`, `getLastAssistantUsageInfo`, message and usage models, token estimation, compaction preparation/orchestration, constructors, serialization, production validation/normalization, integer narrowing, package or harness-root exports, `DEFAULT_COMPACTION_SETTINGS`, dependencies, and unrelated compaction APIs remain deferred. The differential adapter uses final source-shaped records only; it does not call or reimplement the producer. Invalid JavaScript runtime values and lone UTF-16 surrogate strings remain outside the typed Rust boundary.
