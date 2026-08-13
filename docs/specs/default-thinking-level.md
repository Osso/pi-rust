# DEFAULT_THINKING_LEVEL

This specification covers `packages/coding-agent/src/core/defaults.ts:1-3` at the pinned TypeScript commit. The pinned coding-agent package index omits `defaults.ts`, so Rust exposes the value only at `pi_coding_agent::core::defaults`.

## What it must do

- [x] Expose `DEFAULT_THINKING_LEVEL` as a typed `pi_agent_core::harness::ThinkingLevel` constant.
- [x] Set the constant to `ThinkingLevel::Medium`.
- [x] Return the exact `"medium"` literal through the typed value.
- [x] Permit Copy use of the constant without changing its value.
- [x] Register the focused behavioral test beside coding-agent core value tests.

## How it works

- [DEFAULT_THINKING_LEVEL traceability](../porting/traceability/default-thinking-level.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/defaults.rs` — typed default constant.
- `crates/pi-coding-agent/src/core/mod.rs` — direct public module exposure.
- `crates/pi-coding-agent/Cargo.toml` — exact local `pi-agent-core` dependency.
- `crates/pi-coding-agent/tests/defaults.rs` — focused behavioral proof.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/defaults.rs`

## Known gaps (current cycle)

- [ ] Main-thread exact-path commit and inherited verification remain pending.

## Out of scope

Package-index or crate-root re-exports, resolver/session/SDK fallback, CLI parsing, provider behavior, protocol mapping, parser/serde behavior, consumer default selection, differential/oracle support, broad suites, and adjacent defaults remain deferred.
