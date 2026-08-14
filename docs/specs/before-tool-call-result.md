# BeforeToolCallResult

This specification covers `packages/agent/src/types.ts:55-69` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The interface declaration is `types.ts:61-69`; the agent package re-exports it through `packages/agent/src/index.ts:145`. Production is committed as `77b0580`; differential/typeproof work is committed as `f2badfb`.

## What it must do

- [x] Expose `BeforeToolCallResult` from `pi_agent_core::harness::types` and re-export it from `pi_agent_core::harness`.
- [x] Preserve exactly three public mutable fields, in source order: `block: Option<bool>`, `reason: Option<String>`, and `terminate: Option<bool>`.
- [x] Derive `Debug` only; add no constructors, defaults, cloning, equality, serialization, validation, or conversion behavior.
- [x] Preserve omitted typed fields through `Option<T>` without claiming to distinguish omission from explicit JavaScript `undefined`.
- [x] Compare eleven source-shaped plain-record fixtures through exactly one differential request, `13180`.
- [x] Prove the real package-index wildcard export, exact field set/types, optionality, mutability, explicit `undefined` acceptance, and fresh excess-property diagnostics.

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — public record.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root re-export; no crate-root re-export.
- `crates/pi-agent-core/tests/before_tool_call_result.rs` — focused export, optional-state, string, combination, and mutation proof.
- `crates/pi-agent-core/tests/differential_truncate/before_tool_call_result.rs` — request `13180` source-shaped comparison.
- `oracle/truncate-oracle.ts` — source-shaped record oracle operation.
- `oracle/public-index-typecheck.mjs` — real-index interface and type-shape proof.
- `run-tests.sh` — focused test registration; shared differential/oracle infrastructure requires no new target.

## Bounded proof status

- [x] Focused RED at clean base `44737ce` records only missing module and harness-root exports; production commit `77b0580` adds the record and focused proof.
- [x] Differential RED at `77b0580` records the structured unsupported-operation response for request `13180`; GREEN at `f2badfb` compares eleven state-tag fixtures against the source-shaped oracle.
- [x] Real-index typeproof at `f2badfb` verifies the wildcard alias, exact fields/order, optional writable properties, `exactOptionalPropertyTypes=false`, explicit `undefined`, and excess-property diagnostic behavior.
- [x] Targeted Clippy, formatting, diff, readability, and aggregate oracle/typeproof artifacts pass at the bounded proof revision.
- [ ] Final workspace/check/all-feature Clippy, documentation/dependency/clean-pin, and inherited verification remain pending.

## Boundary

The source comments describe blocking, error-result/default-message selection, and batch-termination behavior. This slice preserves only the record shape. `beforeToolCall` execution, blocking, error generation, default messages, termination logic, callbacks, agent loops, tools, providers, messages, serialization, and orchestration remain excluded.

TypeScript optional properties accept explicit `undefined` because the pinned compiler configuration does not enable `exactOptionalPropertyTypes`. Rust `None` is the typed absence state and does not preserve omission-versus-`undefined` identity. JSON differential fixtures omit absent fields and never emit `null`; arbitrary runtime values, excess-property identity, and JavaScript object identity remain outside the typed Rust boundary.

No dependency, deployment, protocol, or unrelated API change applies to this slice.
