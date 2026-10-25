# Coding-agent truncation constants

This specification covers `packages/coding-agent/src/core/tools/truncate.ts:11-13` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

## What it must do

- [x] Expose `DEFAULT_MAX_LINES` from `pi_coding_agent::core::tools::truncate` with value `2_000` and type `usize`.
- [x] Expose `DEFAULT_MAX_BYTES` from `pi_coding_agent::core::tools::truncate` with value `10 * 1024` and type `usize`.
- [x] Expose `GREP_MAX_LINE_LENGTH` from `pi_coding_agent::core::tools::truncate` with value `500` and type `usize`.
- [x] Re-export only `DEFAULT_MAX_LINES` and `DEFAULT_MAX_BYTES` from `pi_coding_agent::core::tools` and the crate root.
- [x] Define the coding-agent constants as aliases of the identical `pi_agent_core::harness::utils` constants.
- [x] Focused behavioral compilation and value proof covers the direct module, tools index, crate root, exact values, and `usize` use.
- [x] Differential request `13186` compares one independent source-shaped request against the real pinned direct module; Rust actual output contains all three direct constants.
- [x] Dedicated source proof verifies real `const` declarations, exact literal/broad-number types, the two-index alias boundary, valid assignments, five exact invalid diagnostics, direct/tools-index runtime values, and compiler-proven package-root aliases.

The `usize` mapping is the approved typed non-negative truncation-limit boundary for these exact source constants. It does not generalize to arbitrary JavaScript `number` values.

## Out of scope

`TruncationResult`/`TruncationOptions`, `truncateHead`, `truncateTail`, `truncateLine`, truncation algorithms, formatting behavior, runtime validation, tool behavior, filesystem/session behavior, dependencies, Cargo.lock, protocol behavior, and unrelated exports remain excluded. Differential/oracle/typeproof coverage for these constants is complete.

## Implementation inventory

- `crates/pi-coding-agent-tools/src/core/tools/truncate.rs` — direct constants and existing formatter wrapper.
- `crates/pi-coding-agent-tools/src/core/tools/mod.rs` — mixed tools-index export boundary.
- `crates/pi-coding-agent/src/lib.rs` — mixed crate-root export boundary.
- `crates/pi-coding-agent/tests/truncation_constants.rs` — focused behavioral compile/value proof.
- `crates/pi-coding-agent/tests/differential_truncation_constants/` — request `13186` source comparison.
- `oracle/coding-agent-truncation-constants-oracle.ts` — real direct-module runtime adapter.
- `oracle/coding-agent-truncation-constants-typecheck.mjs` — real declaration, export, diagnostic, and runtime proof.

## Traceability

- [Coding-agent truncation constants traceability](../porting/traceability/coding-agent-truncation-constants.md)
