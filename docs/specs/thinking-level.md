# ThinkingLevel

This specification covers the agent-harness `ThinkingLevel` union from `packages/agent/src/types.ts:293-298` and its wildcard package export through `packages/agent/src/index.ts:145`. The distinct protocol schema at `packages/protocol/src/schemas.ts:26-35` intentionally stops at `"max"`; this slice ports the agent-harness value and does not alter or depend on `pi_protocol::ThinkingLevel`.

## What it must do

- [x] Expose a closed public enum with exactly `Off`, `Minimal`, `Low`, `Medium`, `High`, `XHigh`, `Max`, and `Ultra` variants.
- [x] Derive exactly `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
- [x] Return the exact source literals `"off"`, `"minimal"`, `"low"`, `"medium"`, `"high"`, `"xhigh"`, `"max"`, and `"ultra"` from `as_str`.
- [x] Re-export `ThinkingLevel` from `pi_agent_core::harness` without a crate-root direct export.
- [x] Cover all variants and literals, equality, copying, generic `Clone`, Debug output, harness-root import, and explicit `Max`/`Ultra` distinction in focused behavioral tests.
- [x] Register the focused test in `run-tests.sh` beside the other mode-enum tests.
- [x] Compare one source-shaped plain-value differential group (`13166`) containing all eight literals in source order; Rust normalizes `ThinkingLevel::as_str` and the oracle echoes final typed literals.
- [x] Prove the exact agent-package public-index union with all eight valid assignments and rejected empty/`extreme` literals.

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — closed enum and literal mapping.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root re-export.
- `crates/pi-agent-core/tests/thinking_level.rs` — focused behavioral and trait proof.
- `run-tests.sh` — focused test registration.
- `crates/pi-agent-core/tests/differential_truncate/thinking_level.rs` — request `13166` differential proof.
- `oracle/truncate-oracle.ts` — minimal typed-literal echo operation.
- `oracle/public-index-typecheck.mjs` — strict pinned agent-package type proof.

## Focused proof

- Focused RED: `/tmp/pi-rust-agent-thinking-level-red.out` exits 101 with only unresolved module and harness-root imports before production exists.
- Focused GREEN: `/tmp/pi-rust-agent-thinking-level-green.out` records 4 passed and 0 failed.
- Targeted denied-warning Clippy: `/tmp/pi-rust-agent-thinking-level-clippy.out` exits 0.
- Formatting, diff, and changed-Rust readability: `/tmp/pi-rust-agent-thinking-level-fmt.out`, `/tmp/pi-rust-agent-thinking-level-diff.out`, and `/tmp/pi-rust-agent-thinking-level-readability.out` all pass.

## Differential/oracle proof

- Request `13166` is one persistent group after ToolExecutionMode request `13165`; cases are `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, `ultra`.
- The request compares only final source-shaped literals. It does not import or claim provider/model support, defaults, parsing, validation, or protocol behavior.
- `oracle/public-index-typecheck.mjs` imports `ThinkingLevel` from the pinned agent package index and proves the exact eight-literal union. It does not modify or typecheck against the protocol `ThinkingLevel`.
- RED: `/tmp/pi-rust-agent-thinking-level-differential-red.out`; GREEN: `/tmp/pi-rust-agent-thinking-level-differential-green.out`; oracle/typeproof: `/tmp/pi-rust-agent-thinking-level-oracle.out`.
- The initial differential fixture builder triggered a 34-line readability RED. A named immutable `CASES` table now preserves the same source order and request content; differential, denied-warning Clippy, formatting, diff, and readability checks pass. Workspace/static/inherited gates remain pending.

## Out of scope

Parsing, `FromStr`, `Display`, serde, validation, provider support or clamping, model resolution, coding-agent defaults, protocol integration, dependencies, broad suites, and adjacent APIs remain deferred. `Ultra` is valid here because the agent-harness union is broader than the distinct protocol schema.

## Traceability

- [ThinkingLevel traceability](../porting/traceability/thinking-level.md)
