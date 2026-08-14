# Coding-agent format-size traceability

Pinned source: `packages/coding-agent/src/core/tools/truncate.ts:61-69` at TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source/export | Observable contract | Rust mapping and proof |
|---|---|---|
| `truncate.ts:61-69` | `formatSize(bytes: number): string`; B for values below 1024, KB below 1024², otherwise MB; one decimal for KB/MB | `pi_coding_agent::core::tools::truncate::format_size(usize) -> String` wraps the identical proven `pi_agent_core::harness::utils::format_size` helper. The approved Rust boundary accepts non-negative integer byte counts only. |
| `core/tools/index.ts:119-127` | `formatSize` is exported through the coding-agent tools index | `pi_coding_agent::core::tools::format_size`; three export-path focused tests cover direct module, tools index, and crate root. |
| `packages/coding-agent/src/index.ts:333` | `formatSize` is exported from the coding-agent package root | `pi_coding_agent::format_size`; no new formatting algorithm is introduced. |
| Differential request `13185` | Runtime results must match the real pinned TypeScript function for the approved numeric domain | Pending one differential request with the 16 hard-coded byte-count cases. |
| Type/export proof | Real source function and all three export paths must be proven; no copied TypeScript declaration | Pending real TypeScript export proof. |

## Boundary and discrepancy

Rust uses `usize` for the approved non-negative integer byte-count domain. This is a deliberate typed-boundary restriction, not a claim to accept arbitrary JavaScript `number` values. Negative, fractional, nonfinite, and above-safe-integer inputs remain outside the contract; runtime validation is not added. The Rust implementation delegates to the already-proven agent-core helper, preserving one implementation of the formatting behavior.

## Proof inventory

- Focused cases: 16 hard-coded values covering zero, B/KB/MB thresholds, and one-decimal rounding boundaries.
- Export-path tests: direct module, tools-index equivalent, and crate root.
- Differential request: `13185`, pending.
- Type/export proof: real pinned TypeScript source and package exports, pending.
- Final workspace, static, and inherited verification: pending.

## Exclusions

Truncation constants, truncation types, other truncation functions, arbitrary JavaScript numeric values outside the approved integer domain, runtime validation, tool/filesystem/shell/output behavior, extra APIs, dependencies, deployment, and protocol changes remain excluded.
