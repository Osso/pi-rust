# Coding-agent format size

This specification covers `formatSize` from the pinned coding-agent source `packages/coding-agent/src/core/tools/truncate.ts:61-69` at TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The function is exported through the coding-agent tools index (`packages/coding-agent/src/core/tools/index.ts:119-127`) and package root export block (`packages/coding-agent/src/index.ts:333`).

## What it must do

- [x] Expose the direct Rust path `pi_coding_agent::core::tools::truncate::format_size`.
- [x] Expose the tools-index-equivalent Rust path `pi_coding_agent::core::tools::format_size`.
- [x] Expose the crate-root Rust path `pi_coding_agent::format_size`.
- [x] Preserve source B, KB, and MB thresholds and one-decimal formatting for the approved non-negative integer byte-count boundary.
- [x] Implement the coding-agent function as a wrapper over the identical proven `pi_agent_core::harness::utils::format_size` helper.
- [x] Cover 16 hard-coded focused byte-count cases across branch thresholds and rounding boundaries.
- [x] Cover all three Rust export paths with focused tests.
- [x] Compare request `13185` against the real pinned TypeScript function with 16 independent hard-coded cases; differential GREEN is 1/0.
- [x] Complete real direct-module/tools-index/package-index proof: exact `(number) => string` signature, three valid assignments, and three wrong-string diagnostics; targeted Clippy, typecheck, formatting, diff, and readability pass.
- [x] Complete executable/docs verification at `a8a376e`: workspace `304 passed, 0 failed, 2 ignored` across 68 result lines; workspace check, all-target/all-feature Clippy with `-D warnings`, aggregate oracle/typeproof/Node, fmt/diff, Rust/TS readability, exact wrapper/export/scope/dependency/docs/lock/clean-pin audit pass. Evidence: `/tmp/pi-rust-coding-format-size-final-*.out` and final audit.

## How it works

- [Coding-agent format-size traceability](../porting/traceability/coding-agent-format-size.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/tools/truncate.rs` — direct-module wrapper over agent-core `format_size`.
- `crates/pi-coding-agent/src/core/tools/mod.rs` — tools-index-equivalent export.
- `crates/pi-coding-agent/src/core/mod.rs` — coding-agent tools module boundary.
- `crates/pi-coding-agent/src/lib.rs` — crate-root export.
- `crates/pi-coding-agent/tests/format_size.rs` — 16 focused cases and three export-path tests.
- `crates/pi-coding-agent/tests/differential_format_size/` — request `13185` source comparison.
- `oracle/coding-agent-format-size-oracle.ts` — real direct-module runtime adapter.
- `oracle/coding-agent-format-size-typecheck.mjs` — defining/tools/package export proof.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/format_size.rs`
- `crates/pi-coding-agent/tests/differential_format_size.rs`
- `oracle/coding-agent-format-size-typecheck.mjs`
- `crates/pi-agent-core/tests/truncate.rs`
- `crates/pi-agent-core/tests/differential_truncate/format_size.rs`

## Known gaps (current cycle)

- [x] Inherited independent verification at docs-only closure HEAD `52ff0b0` returns `OVERALL: PASS`; broad evidence remains valid from `a8a376e`.

## Boundary and implementation relationship

Coding-agent `formatSize` is a separate package boundary from agent-core truncation. Its Rust wrapper delegates to the identical approved `pi_agent_core::harness::utils::format_size` `usize` behavior; this does not claim broader coding-agent truncation coverage.

## Out of scope

- Coding-agent truncation APIs other than the separately completed constants and `formatSize`.
- Truncation types (`TruncationOptions` and `TruncationResult`) and other coding-agent truncation functions.
- Arbitrary JavaScript negative, fractional, nonfinite, or above-safe-integer values.
- Runtime validation of unsupported numeric inputs.
- Tool, filesystem, shell, and output-rendering behavior.
- Extra APIs, dependencies, deployment, and protocol changes.
