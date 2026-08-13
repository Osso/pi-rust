# Queue mode traceability

Pinned source: `packages/agent/src/types.ts:44-50` and package export `packages/agent/src/index.ts:145` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `types.ts:44-50` | Closed string union with values `"all"` and `"one-at-a-time"` | `harness::types::QueueMode::{All, OneAtATime}` with `as_str`; `variants_return_exact_source_literals` |
| `types.ts:44-50` | The value type is copyable and equality-comparable; runtime Debug is not a TypeScript contract | Exact Rust derives and focused `enum_values_support_equality_copy_and_debug` test |
| `index.ts:145` | `export * from "./types.ts"` exposes `QueueMode` at the package index | `pi_agent_core::harness::QueueMode`; `harness_root_reexports_the_same_enum` proves the root path, and the strict public-index typeproof proves exactly `"all" | "one-at-a-time"` |

## Rust boundary

- `QueueMode` is a closed public enum with exactly `All` and `OneAtATime`.
- `as_str` is a `pub const fn` returning the exact source literals.
- The enum is re-exported from `pi_agent_core::harness`; no crate-root direct re-export is added.
- No parser, formatter, serializer, validation, queue operation, or dependency is part of this slice.
- Differential and typeproof roles are separate: the runtime proof compares final plain literals only; the typeproof checks the erased package-index union and invalid assignments.

## Proof status

- Focused RED: `/tmp/pi-rust-queue-mode-red.out` records exit 101 with only unresolved `QueueMode` imports from the module and harness-root paths.
- Focused GREEN: `/tmp/pi-rust-queue-mode-green.out` records exit 0 with 3 passed and 0 failed.
- Differential RED: `/tmp/pi-rust-queue-mode-differential-red.out` records exit 101 after prior requests pass; request `13164` reaches the unchanged oracle's unsupported `queue_mode` operation and returns non-JSON.
- Differential GREEN: `/tmp/pi-rust-queue-mode-differential-green.out` records exit 0 with the persistent differential test passing; request `13164` contains exactly the `all` and `one-at-a-time` source literals.
- Oracle/typeproof GREEN: `/tmp/pi-rust-queue-mode-oracle.out` records `./run-tests.sh oracle` exit 0, including the strict public-index proof for exact union shape, both valid assignments, and rejected empty/alternate literals.
- Differential and typeproof roles remain separate: request `13164` compares final literals only; the public-index proof checks the exact union and rejected assignments.
- Changed Rust/TypeScript/JavaScript readability audits report no new threshold violation.
- Final workspace: `/tmp/pi-rust-queue-mode-final-workspace-tests.out` captures `cargo test --workspace --all-targets` at code/proof HEAD `7b0358e` with 257 passed, 0 failed, and 2 ignored; exit 0.
- Final static gates: `/tmp/pi-rust-queue-mode-final-check.out` captures `cargo check --workspace --all-targets`; `/tmp/pi-rust-queue-mode-final-clippy.out` captures all-target/all-feature Clippy with `-D warnings`; both exit 0.
- Final oracle, formatting, syntax, and diff gates: `/tmp/pi-rust-queue-mode-final-oracle.out`, `/tmp/pi-rust-queue-mode-final-fmt.out`, both final Node artifacts, and `/tmp/pi-rust-queue-mode-final-diff.out` record exit 0. Later documentation-only changes do not invalidate executable proof.
- Documentation and pins: README, dependency inventory, compatibility, dedicated spec, and traceability are consistent; the pinned source worktree is clean at `532f7d12cd19235081bd4d6e425bde185652d86f`.
- Inherited verification: at clean HEAD `740520d`, focused results were 3 passed and 0 failed, differential results were 1 passed and 0 failed, and oracle, Node syntax, formatting, and diff checks exited 0. Saved workspace results of 257 passed, 0 failed, and 2 ignored plus workspace check and all-feature Clippy with `-D warnings` pass. `OVERALL: PASS`.

## Exclusions

Queue draining, parsing, `FromStr`, `Display`, serde, validation, `ToolExecutionMode`, runtime constructors, dependencies, and adjacent queue APIs remain deferred.
