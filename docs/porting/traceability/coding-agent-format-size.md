# Coding-agent format-size traceability

Pinned source: `packages/coding-agent/src/core/tools/truncate.ts:61-69` at TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source/export | Observable contract | Rust mapping and proof |
|---|---|---|
| `truncate.ts:61-69` | `formatSize(bytes: number): string`; B for values below 1024, KB below 1024², otherwise MB; one decimal for KB/MB | `pi_coding_agent::core::tools::truncate::format_size(usize) -> String` wraps the identical proven `pi_agent_core::harness::utils::format_size` helper. The approved Rust boundary accepts non-negative integer byte counts only. |
| `core/tools/index.ts:119-127` | `formatSize` is exported through the coding-agent tools index | `pi_coding_agent::core::tools::format_size`; three export-path focused tests cover direct module, tools index, and crate root. |
| `packages/coding-agent/src/index.ts:333` | `formatSize` is exported from the coding-agent package root | `pi_coding_agent::format_size`; no new formatting algorithm is introduced. |
| Differential request `13185` | Runtime results must match the real pinned TypeScript function for the approved numeric domain | Differential GREEN is 1/0 across 16 independent hard-coded byte-count cases. |
| Type/export proof | Real source function and all three export paths must be proven; no copied TypeScript declaration | Real direct-module/tools-index/package-index proof passes exact `(number) => string`, three valid assignments, and three wrong-string diagnostics. |

## Boundary and discrepancy

Rust uses `usize` for the approved non-negative integer byte-count domain. This is a deliberate typed-boundary restriction, not a claim to accept arbitrary JavaScript `number` values. Negative, fractional, nonfinite, and above-safe-integer inputs remain outside the contract; runtime validation is not added. The Rust implementation delegates to the already-proven agent-core helper, preserving one implementation of the formatting behavior. This coding-agent boundary is separate from agent-core truncation: only `formatSize` is covered here, and the wrapper preserves the identical approved `usize` behavior.

## Proof inventory

- Focused cases: 16 hard-coded values covering zero, B/KB/MB thresholds, and one-decimal rounding boundaries.
- Export-path tests: direct module, tools-index equivalent, and crate root.
- Differential request: `13185`, GREEN 1/0 across 16 independent cases.
- Type/export proof: real pinned TypeScript direct module, tools index, and package index; exact function signature; 3 valid and 3 wrong-string diagnostics.
- Targeted Clippy, typecheck, formatting, diff, and readability: pass.
- Final executable/docs verification at `a8a376e`: workspace `304 passed, 0 failed, 2 ignored` across 68 result lines; workspace check, all-target/all-feature Clippy with `-D warnings`, aggregate oracle/typeproof/Node, fmt/diff, Rust/TS readability, exact wrapper/export/scope/dependency/docs/lock/clean-pin audit pass. Evidence is `/tmp/pi-rust-coding-format-size-final-*.out` plus the final audit. Inherited verification remains pending only for this documentation closure.

## Exclusions

Other coding-agent truncation APIs, truncation constants, truncation types, other agent-core truncation functions, arbitrary JavaScript numeric values outside the approved integer domain, runtime validation, tool/filesystem/shell/output behavior, extra APIs, dependencies, deployment, and protocol changes remain excluded.
