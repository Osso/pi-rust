# ToolExecutionMode

This specification covers the committed `ToolExecutionMode` value from `packages/agent/src/types.ts:34-42` and its package export through `packages/agent/src/index.ts:145`. Rust production is `db9f5e2`; differential and typeproof support is `30d0165`. Tool preparation, scheduling, execution, and event ordering remain separate behavior.

## What it must do

- [x] Expose a closed public enum with exactly `Sequential` and `Parallel` variants.
- [x] Derive exactly `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
- [x] Return the exact source literals `"sequential"` and `"parallel"` from `as_str`.
- [x] Re-export `ToolExecutionMode` from `pi_agent_core::harness` without a crate-root direct re-export.
- [x] Cover both variants, literals, equality, copying, Debug output, and the harness-root import in focused behavioral tests.

## How it works

- [Tool execution mode traceability](../porting/traceability/tool-execution-mode.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — closed enum and literal mapping.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root re-export.
- `crates/pi-agent-core/tests/tool_execution_mode.rs` — focused behavioral/API and generic `Clone` proof.
- `crates/pi-agent-core/tests/differential_truncate/tool_execution_mode.rs` — request `13165` plain-value differential proof.
- `oracle/truncate-oracle.ts` — final-literal runtime oracle.
- `oracle/public-index-typecheck.mjs` — strict package-index union proof.
- `run-tests.sh` — focused, differential, and oracle registrations.

## Tests asserting this spec

- `crates/pi-agent-core/tests/tool_execution_mode.rs`
- `crates/pi-agent-core/tests/differential_truncate/tool_execution_mode.rs`
- `oracle/public-index-typecheck.mjs`

## Final integration proof

- [x] Run `cargo test --workspace --all-targets`: 260 passed, 0 failed, and 2 ignored at code/proof HEAD `30d0165`.
- [x] Run workspace all-target check and all-target/all-feature Clippy with `-D warnings`: both exit 0.
- [x] Run oracle/typeproof, formatting, Node syntax, and diff checks: all exit 0.
- [x] Complete Rust/TypeScript/JavaScript readability, documentation, dependency, and clean-pin audits.

## Verification closure

- [x] Inherited verification at clean HEAD `e1aa5c0`: reproduced pre-production RED exits 101 with only the two missing imports; focused GREEN 3 passed/0 failed, differential 1 passed/0 failed, fmt/diff exit 0; saved workspace 260 passed/0 failed/2 ignored plus workspace check, all-feature Clippy with `-D warnings`, oracle, and Node syntax pass. `OVERALL: PASS`.

## Proof status

- Focused RED evidence: the original `/tmp/pi-rust-tool-execution-mode-red.out` capture is zero bytes and is not used as proof. `/tmp/pi-rust-tool-execution-mode-red-reproduced.out` reconstructs the RED at exact pre-production revision `bca24d2` with the committed `db9f5e2` focused test source; it exits 101 with only the missing module and harness-root imports. The original production agent report independently recorded the same exit and failure boundary.
- Focused GREEN: `/tmp/pi-rust-tool-execution-mode-green-corrected.out` records 3 passed and 0 failed.
- Targeted Clippy RED/GREEN: `/tmp/pi-rust-tool-execution-mode-clippy-red.out` records the unsuppressed `clone_on_copy` failure; `/tmp/pi-rust-tool-execution-mode-clippy-green.out` records denied-warning Clippy exit 0 after the generic `Clone` proof replaced the invalid direct clone.
- Request `13165` is the single persistent source-shaped plain-value group after QueueMode request `13164`; it contains both `sequential` and `parallel`.
- `/tmp/pi-rust-tool-execution-mode-differential-red.out` records the valid unsupported-operation RED before oracle support; `/tmp/pi-rust-tool-execution-mode-differential-green.out` records the differential GREEN.
- `/tmp/pi-rust-tool-execution-mode-oracle.out` records the pinned oracle and strict public-index typeproof GREEN.
- Runtime plain-value comparison and erased public-index typeproof remain separate; no constructor, parser, validation, scheduling, execution, event-order, or invalid-runtime-input behavior is claimed.

## Out of scope

- Parsing, `FromStr`, `Display`, serde, validation, invalid runtime inputs, dependencies, tool scheduling or execution, event ordering, `QueueMode`, constructors, and adjacent APIs.
