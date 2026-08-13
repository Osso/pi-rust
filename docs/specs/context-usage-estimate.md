# Context usage estimate

This specification covers the module-local `ContextUsageEstimate` interface from `packages/agent/src/harness/compaction/compaction.ts:200-207` and its Rust value boundary. Context-token estimation and compaction orchestration remain deferred.

## What it must do

- [x] Expose a public mutable `ContextUsageEstimate` through `pi_agent_core::harness::compaction`.
- [x] Preserve required fields in source order: `tokens: f64`, `usage_tokens: f64`, `trailing_tokens: f64`, and `last_usage_index: Option<f64>`.
- [x] Derive `Debug` only without constructors, serialization, validation, normalization, or integer narrowing.
- [x] Preserve ordinary values, `None`/`Some` index states, public mutation, signed zero, subnormals, finite extremes, infinities, and distinct NaN payloads in every numeric field.

## How it works

- [Context usage estimate traceability](../porting/traceability/context-usage-estimate.md)
- [Compaction compatibility matrix](compatibility.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/compaction/mod.rs` — module-local value record.
- `crates/pi-agent-core/tests/context_usage_estimate.rs` — focused field, mutation, Debug, optional-state, and binary64 proof.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/context_usage_estimate.rs`

## Known gaps (current cycle)

- [ ] Add pinned TypeScript differential proof and any required direct-module type proof in a later phase.
- [ ] Complete final workspace, static, readability, documentation, and clean-pin gates after later phases.

## Out of scope

`estimateContextTokens`, `calculateContextTokens`, `getLastAssistantUsageInfo`, message and usage models, token estimation, compaction preparation/orchestration, constructors, serialization, validation, normalization, integer narrowing, package or harness-root exports, `DEFAULT_COMPACTION_SETTINGS`, dependencies, and unrelated compaction APIs remain deferred. Invalid JavaScript runtime values and lone UTF-16 surrogate strings remain outside the typed Rust boundary.
