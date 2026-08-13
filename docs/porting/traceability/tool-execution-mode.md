# ToolExecutionMode traceability

Pinned source: `packages/agent/src/types.ts:34-42` and package export `packages/agent/src/index.ts:145` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`. Rust production is `db9f5e2`; differential and typeproof support is `30d0165`.

| TypeScript source | Observable contract | Rust mapping and focused proof |
|---|---|---|
| `types.ts:34-42` | Closed string union with values `"sequential"` and `"parallel"` | `harness::types::ToolExecutionMode::{Sequential, Parallel}` with `as_str`; `variants_return_exact_source_literals` |
| `types.ts:34-42` | The value is copyable and equality-comparable; Debug is an approved Rust boundary trait | Exact `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq` derives plus `enum_values_support_equality_copy_and_debug` |
| `index.ts:145` | `export * from "./types.ts"` exposes the value at the package index | `pi_agent_core::harness::ToolExecutionMode`; `harness_root_reexports_the_same_enum` proves the requested Rust public path. No crate-root direct re-export is added. |
| `types.ts:34-42`, `index.ts:145` | Both valid literals remain the final source-shaped values, and the public union excludes `""` and `"concurrent"` | Differential request `13165` compares `sequential` and `parallel`; `oracle/public-index-typecheck.mjs` proves the exact erased union and valid/invalid assignments. |

## Rust boundary

- `ToolExecutionMode` is a closed public enum with exactly `Sequential` and `Parallel`.
- It derives exactly `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
- `as_str` is the only conversion API and returns the exact source literals.
- Focused tests prove both variants, literal mapping, equality, copying, Debug output, and harness-root import; the generic `Clone` proof avoids a `clone_on_copy` warning suppression.
- This phase does not claim parser, formatter, serializer, validation, scheduling, execution, or event-ordering behavior.

## Differential/oracle proof

- The persistent differential runner keeps `tool_execution_mode::compare` immediately after QueueMode request `13164`.
- Request `13165` contains one source-shaped plain-value group with both final literals: `sequential` and `parallel`.
- Rust normalizes `ToolExecutionMode::as_str`; the TypeScript oracle echoes only the typed final literals. Neither side implements constructors, parsing, validation, scheduling, execution, or event ordering.
- Runtime plain-value comparison and erased public-index type proof remain separate.

## Proof artifacts

- Focused RED: `/tmp/pi-rust-tool-execution-mode-red.out` records exit 101 with only unresolved module and harness-root imports.
- Corrected focused GREEN: `/tmp/pi-rust-tool-execution-mode-green-corrected.out` records exit 0 with 3 passed and 0 failed.
- Targeted Clippy RED/GREEN: `/tmp/pi-rust-tool-execution-mode-clippy-red.out` records the unsuppressed `clone_on_copy` failure; `/tmp/pi-rust-tool-execution-mode-clippy-green.out` records denied-warning Clippy exit 0 after the generic `Clone` proof correction.
- Differential RED: `/tmp/pi-rust-tool-execution-mode-differential-red.out` records exit 101 after prior requests pass; request `13165` reaches the unchanged oracle and receives non-JSON unsupported-operation output.
- Differential GREEN: `/tmp/pi-rust-tool-execution-mode-differential-green.out` records exit 0 with 1 passed and 0 failed.
- Oracle/typeproof GREEN: `/tmp/pi-rust-tool-execution-mode-oracle.out` records `./run-tests.sh oracle` exit 0, including exact public-index union, valid assignments, and `@ts-expect-error` rejection checks.
- Bounded formatting, diff, and Node syntax: `/tmp/pi-rust-tool-execution-mode-fmt.out`, `/tmp/pi-rust-tool-execution-mode-diff.out`, and `/tmp/pi-rust-tool-execution-mode-node-check.out` record exit 0.
- Changed Rust/TypeScript/JavaScript readability artifacts: `/tmp/pi-rust-tool-execution-mode-readability-1.out`, `/tmp/pi-rust-tool-execution-mode-readability-2.out`, `/tmp/pi-rust-tool-execution-mode-readability-ts-1.out`, and `/tmp/pi-rust-tool-execution-mode-readability-ts-2.out`; no new threshold violations or suppressions.
- Final workspace: `/tmp/pi-rust-tool-execution-mode-final-workspace-tests.out` captures `cargo test --workspace --all-targets` at code/proof HEAD `30d0165` with 260 passed, 0 failed, and 2 ignored; exit 0.
- Final static gates: `/tmp/pi-rust-tool-execution-mode-final-check.out` captures `cargo check --workspace --all-targets`; `/tmp/pi-rust-tool-execution-mode-final-clippy.out` captures all-target/all-feature Clippy with `-D warnings`; both exit 0.
- Final oracle, formatting, syntax, and diff gates: `/tmp/pi-rust-tool-execution-mode-final-oracle.out`, `/tmp/pi-rust-tool-execution-mode-final-fmt.out`, both final Node artifacts, and `/tmp/pi-rust-tool-execution-mode-final-diff.out` record exit 0. Later documentation-only changes do not invalidate executable proof.
- Documentation and pins: README, dependency inventory, compatibility, dedicated spec, and traceability are consistent; the pinned source worktree is clean at `532f7d12cd19235081bd4d6e425bde185652d86f`.

## Exclusions

Broad workspace suites, tool preparation, scheduling, execution, event ordering, parsing, `FromStr`, `Display`, serde, validation, invalid runtime inputs, `QueueMode`, dependencies, constructors, and adjacent APIs remain excluded.
