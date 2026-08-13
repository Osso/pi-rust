# DEFAULT_THINKING_LEVEL

This specification covers `packages/coding-agent/src/core/defaults.ts:1-3` at the pinned TypeScript commit. The pinned coding-agent package index omits `defaults.ts`, so Rust exposes the value only at `pi_coding_agent::core::defaults`.

## What it must do

- [x] Expose `DEFAULT_THINKING_LEVEL` as a typed `pi_agent_core::harness::ThinkingLevel` constant.
- [x] Set the constant to `ThinkingLevel::Medium`.
- [x] Return the exact `"medium"` literal through the typed value.
- [x] Permit Copy use of the constant without changing its value.
- [x] Register the focused behavioral test beside coding-agent core value tests.
- [x] Compare the actual direct-module export through persistent differential request `13167`.
- [x] Prove the direct-module type identity against pinned agent-core `ThinkingLevel` and separately prove the runtime value is exactly `medium`.

## How it works

- [DEFAULT_THINKING_LEVEL traceability](../porting/traceability/default-thinking-level.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/defaults.rs` — typed default constant.
- `crates/pi-coding-agent/src/core/mod.rs` — direct public module exposure.
- `crates/pi-coding-agent/Cargo.toml` — exact local `pi-agent-core` dependency.
- `crates/pi-coding-agent/tests/defaults.rs` — focused behavioral proof.
- `crates/pi-coding-agent/tests/differential_default_thinking_level.rs` — direct-module differential proof.
- `oracle/default-thinking-level-oracle.ts` — direct-module runtime oracle.
- `oracle/default-thinking-level-typecheck.mjs` — direct-module type/value proof.
- `run-tests.sh` — focused, differential, and oracle registrations.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/defaults.rs`
- `crates/pi-coding-agent/tests/differential_default_thinking_level.rs`
- `oracle/default-thinking-level-typecheck.mjs`

## Bounded proof gates (current cycle)

- [x] Differential RED/GREEN, direct-module type/value proof, formatting, diff, Node syntax, and changed-file readability pass; no broad suite or final static/inherited verification is claimed.
- [ ] Main-thread review/commit and final workspace/static/inherited verification remain pending.

## Out of scope

Package-index or crate-root re-exports, resolver/session/SDK fallback, CLI parsing, provider behavior, protocol mapping, parser/serde behavior, consumer default selection, broad suites, and adjacent defaults remain deferred. This slice makes no coding-agent package-index claim.
