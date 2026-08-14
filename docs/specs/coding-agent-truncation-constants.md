# Coding-agent truncation constants

This specification covers `packages/coding-agent/src/core/tools/truncate.ts:11-13` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

## What it must do

- [x] Expose `DEFAULT_MAX_LINES` from `pi_coding_agent::core::tools::truncate` with value `2_000` and type `usize`.
- [x] Expose `DEFAULT_MAX_BYTES` from `pi_coding_agent::core::tools::truncate` with value `10 * 1024` and type `usize`.
- [x] Expose `GREP_MAX_LINE_LENGTH` from `pi_coding_agent::core::tools::truncate` with value `500` and type `usize`.
- [x] Re-export only `DEFAULT_MAX_LINES` and `DEFAULT_MAX_BYTES` from `pi_coding_agent::core::tools` and the crate root.
- [x] Define the coding-agent constants as aliases of the identical `pi_agent_core::harness::utils` constants.
- [x] Focused behavioral compilation and value proof covers the direct module, tools index, crate root, exact values, and `usize` use.

The `usize` mapping is the approved typed non-negative truncation-limit boundary for these exact source constants. It does not generalize to arbitrary JavaScript `number` values.

## Out of scope

Truncation functions and algorithms, formatting behavior, interfaces, runtime validation, tool behavior, filesystem/session behavior, differential/oracle/typeproof work, dependencies, Cargo.lock, protocol behavior, and unrelated exports remain excluded.

## Implementation inventory

- `crates/pi-coding-agent/src/core/tools/truncate.rs` — direct constants and existing formatter wrapper.
- `crates/pi-coding-agent/src/core/tools/mod.rs` — mixed tools-index export boundary.
- `crates/pi-coding-agent/src/lib.rs` — mixed crate-root export boundary.
- `crates/pi-coding-agent/tests/truncation_constants.rs` — focused behavioral compile/value proof.

## Traceability

- [Coding-agent truncation constants traceability](../porting/traceability/coding-agent-truncation-constants.md)
