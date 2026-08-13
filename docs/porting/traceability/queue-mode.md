# Queue mode traceability

Pinned source: `packages/agent/src/types.ts:44-50` and package export `packages/agent/src/index.ts:145` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and focused proof |
|---|---|---|
| `types.ts:44-50` | Closed string union with values `"all"` and `"one-at-a-time"` | `harness::types::QueueMode::{All, OneAtATime}` with `as_str`; `variants_return_exact_source_literals` |
| `types.ts:44-50` | The value type is copyable and equality-comparable; runtime Debug is not a TypeScript contract | Exact Rust derives and focused `enum_values_support_equality_copy_and_debug` test |
| `index.ts:145` | `export * from "./types.ts"` exposes `QueueMode` at the package index | `pi_agent_core::harness::QueueMode`; `harness_root_reexports_the_same_enum` proves the root path |

## Rust boundary

- `QueueMode` is a closed public enum with exactly `All` and `OneAtATime`.
- `as_str` is a `pub const fn` returning the exact source literals.
- The enum is re-exported from `pi_agent_core::harness`; no crate-root direct re-export is added.
- No parser, formatter, serializer, validation, queue operation, or dependency is part of this slice.

## Proof status

- Focused RED: `/tmp/pi-rust-queue-mode-red.out` records exit 101 with only unresolved `QueueMode` imports from the module and harness-root paths.
- Focused GREEN: `/tmp/pi-rust-queue-mode-green.out` records exit 0 with 3 passed and 0 failed.
- `cargo fmt --all -- --check` exits 0; artifact: `/tmp/pi-rust-queue-mode-fmt.out`.
- `git diff --check` exits 0; artifact: `/tmp/pi-rust-queue-mode-diff-check.out`.
- Changed Rust readability audit reports 0 issues; artifact: `/tmp/pi-rust-queue-mode-readability-audit.json`.
- Rust metrics artifacts are under `/tmp/pi-rust-queue-mode-readability/`; no new complexity or length finding is claimed.

## Exclusions

Queue draining, parsing, `FromStr`, `Display`, serde, validation, `ToolExecutionMode`, differential/oracle support, dependencies, and adjacent queue APIs remain deferred.
