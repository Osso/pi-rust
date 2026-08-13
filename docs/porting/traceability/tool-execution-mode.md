# ToolExecutionMode traceability

Pinned source: `packages/agent/src/types.ts:34-42` and package export `packages/agent/src/index.ts:145` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and focused proof |
|---|---|---|
| `types.ts:34-42` | Closed string union with values `"sequential"` and `"parallel"` | `harness::types::ToolExecutionMode::{Sequential, Parallel}` with `as_str`; `variants_return_exact_source_literals` |
| `types.ts:34-42` | The value is copyable and equality-comparable; Debug is an approved Rust boundary trait | Exact Rust derives and `enum_values_support_equality_copy_and_debug` |
| `index.ts:145` | `export * from "./types.ts"` exposes the value at the package index | `pi_agent_core::harness::ToolExecutionMode`; `harness_root_reexports_the_same_enum` proves the requested Rust public path. No crate-root direct re-export is added. |

## Rust boundary

- `ToolExecutionMode` is a closed public enum with exactly `Sequential` and `Parallel`.
- `as_str` is a `pub const fn` returning the exact source literals.
- Focused tests prove both variants, literal mapping, equality, copying, Debug output, and harness-root import.
- This phase does not claim parser, formatter, serializer, validation, scheduling, execution, or event-ordering behavior.

## Proof artifacts

- Focused RED: `/tmp/pi-rust-tool-execution-mode-red.out` records exit 101 with only unresolved module and harness-root imports.
- Corrected focused GREEN: `/tmp/pi-rust-tool-execution-mode-green-corrected.out` records exit 0 with 3 passed and 0 failed.
- Targeted Clippy RED/GREEN: `/tmp/pi-rust-tool-execution-mode-clippy-red.out` records the unsuppressed `clone_on_copy` failure; the test now exercises Clone through a generic helper, and `/tmp/pi-rust-tool-execution-mode-clippy-green.out` records denied-warning Clippy exit 0.
- Formatting and diff: `/tmp/pi-rust-tool-execution-mode-fmt-corrected.out` and `/tmp/pi-rust-tool-execution-mode-diff-corrected.out` record exit 0.
- Changed Rust readability: `/tmp/pi-rust-tool-execution-mode-readability-corrected.out` reports 0 issues across the current production/export/test files.

## Exclusions

Differential/oracle support, public-index typeproof, broad workspace suites, tool preparation, scheduling, execution, event ordering, parsing, `FromStr`, `Display`, serde, validation, `QueueMode`, dependencies, constructors, and adjacent APIs remain excluded.
