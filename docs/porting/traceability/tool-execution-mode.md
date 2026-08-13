# ToolExecutionMode traceability

Pinned source: `packages/agent/src/types.ts:34-42` and package export `packages/agent/src/index.ts:145` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and focused proof |
|---|---|---|
| `types.ts:34-42` | Closed string union with values `"sequential"` and `"parallel"` | `harness::types::ToolExecutionMode::{Sequential, Parallel}` with `as_str`; `variants_return_exact_source_literals` |
| `types.ts:34-42` | The value is copyable and equality-comparable; Debug is an approved Rust boundary trait | Exact Rust derives and `enum_values_support_equality_copy_and_debug` |
| `index.ts:145` | `export * from "./types.ts"` exposes the value at the package index | `pi_agent_core::harness::ToolExecutionMode`; `harness_root_reexports_the_same_enum` proves the requested Rust public path. No crate-root direct re-export is added. |
| `types.ts:34-42`, `index.ts:145` | Both valid literals remain the final source-shaped values, and the public union excludes `""` and `"concurrent"` | Differential request `13165` compares `sequential` and `parallel`; `oracle/public-index-typecheck.mjs` proves the exact erased union and valid/invalid assignments. |

## Rust boundary

- `ToolExecutionMode` is a closed public enum with exactly `Sequential` and `Parallel`.
- `as_str` is a `pub const fn` returning the exact source literals.
- Focused tests prove both variants, literal mapping, equality, copying, Debug output, and harness-root import.
- This phase does not claim parser, formatter, serializer, validation, scheduling, execution, or event-ordering behavior.

## Differential/oracle proof

- The persistent differential runner keeps `tool_execution_mode::compare` immediately after QueueMode request `13164`.
- Request `13165` contains one source-shaped plain-value group with both final literals: `sequential` and `parallel`.
- Rust normalizes `ToolExecutionMode::as_str`; the TypeScript oracle echoes only the typed final literals. Neither side implements constructors, parsing, validation, scheduling, execution, or event ordering.
- Runtime plain-value comparison and erased public-index type proof remain separate.

## Proof artifacts

- Focused RED: `/tmp/pi-rust-tool-execution-mode-red.out` records exit 101 with only unresolved module and harness-root imports.
- Corrected focused GREEN: `/tmp/pi-rust-tool-execution-mode-green-corrected.out` records exit 0 with 3 passed and 0 failed.
- Differential RED: `/tmp/pi-rust-tool-execution-mode-differential-red.out` records exit 101 after prior requests pass; request `13165` reaches the unchanged oracle and receives non-JSON unsupported-operation output.
- Differential GREEN: `/tmp/pi-rust-tool-execution-mode-differential-green.out` records exit 0 with 1 passed and 0 failed.
- Oracle/typeproof GREEN: `/tmp/pi-rust-tool-execution-mode-oracle.out` records `./run-tests.sh oracle` exit 0, including exact public-index union, valid assignments, and `@ts-expect-error` rejection checks.
- Bounded formatting, diff, and Node syntax: `/tmp/pi-rust-tool-execution-mode-fmt.out`, `/tmp/pi-rust-tool-execution-mode-diff.out`, and `/tmp/pi-rust-tool-execution-mode-node-check.out` record exit 0.
- Changed Rust/TypeScript/JavaScript readability artifacts: `/tmp/pi-rust-tool-execution-mode-readability-1.out`, `/tmp/pi-rust-tool-execution-mode-readability-2.out`, `/tmp/pi-rust-tool-execution-mode-readability-ts-1.out`, and `/tmp/pi-rust-tool-execution-mode-readability-ts-2.out`; no new threshold violations or suppressions.
- Final workspace/static/inherited verification remains pending and is intentionally not claimed by this phase.

## Exclusions

Broad workspace suites, tool preparation, scheduling, execution, event ordering, parsing, `FromStr`, `Display`, serde, validation, `QueueMode`, dependencies, constructors, and adjacent APIs remain excluded.
