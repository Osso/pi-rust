# ThinkingLevel

This specification covers the committed agent-harness `ThinkingLevel` union from `packages/agent/src/types.ts:293-298` and its public export through `packages/agent/src/index.ts:145`. Rust production is `624040f`; differential and public-index proof is `716f1dd`. The distinct protocol schema at `packages/protocol/src/schemas.ts:26-35` intentionally stops at `"max"`; this slice ports the agent-harness value and does not alter or depend on `pi_protocol::ThinkingLevel`.

## What it must do

- [x] Expose a closed public enum with exactly `Off`, `Minimal`, `Low`, `Medium`, `High`, `XHigh`, `Max`, and `Ultra` variants.
- [x] Derive exactly `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
- [x] Return the exact source literals `"off"`, `"minimal"`, `"low"`, `"medium"`, `"high"`, `"xhigh"`, `"max"`, and `"ultra"` from `as_str`.
- [x] Re-export `ThinkingLevel` from `pi_agent_core::harness` without a crate-root direct export.
- [x] Cover all variants and literals, equality, copying, generic `Clone`, Debug output, harness-root import, and explicit `Max`/`Ultra` distinction in focused behavioral tests.
- [x] Register the focused test in `run-tests.sh` beside the other mode-enum tests.
- [x] Compare one source-shaped plain-value differential group (`13166`) containing all eight literals in source order; Rust normalizes `ThinkingLevel::as_str` and the oracle echoes final typed literals.
- [x] Prove the exact agent-package public-index union with all eight valid assignments and rejected empty/`extreme` literals.

## How it works

- [ThinkingLevel traceability](../porting/traceability/thinking-level.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — closed enum and literal mapping.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root re-export.
- `crates/pi-agent-core/tests/thinking_level.rs` — focused behavioral and trait proof.
- `run-tests.sh` — focused, differential, and oracle registrations.
- `crates/pi-agent-core/tests/differential_truncate/thinking_level.rs` — request `13166` differential proof.
- `oracle/truncate-oracle.ts` — minimal typed-literal echo operation.
- `oracle/public-index-typecheck.mjs` — strict pinned agent-package type proof.

## Tests asserting this spec

- `crates/pi-agent-core/tests/thinking_level.rs`
- `crates/pi-agent-core/tests/differential_truncate/thinking_level.rs`
- `oracle/public-index-typecheck.mjs`

## Proof status

- Focused RED: `/tmp/pi-rust-agent-thinking-level-red.out` exits 101 with only unresolved module and harness-root imports before production exists.
- Focused GREEN: `/tmp/pi-rust-agent-thinking-level-green.out` records 4 passed and 0 failed.
- Targeted denied-warning Clippy, formatting, diff, and changed-Rust readability pass in `/tmp/pi-rust-agent-thinking-level-clippy.out`, `/tmp/pi-rust-agent-thinking-level-fmt-current.out`, `/tmp/pi-rust-agent-thinking-level-diff-current.out`, and `/tmp/pi-rust-agent-thinking-level-readability-current-green.out`.
- Request `13166` is one persistent differential group after ToolExecutionMode request `13165`; `/tmp/pi-rust-agent-thinking-level-differential-red.out` records the pre-oracle unsupported-operation RED, and `/tmp/pi-rust-agent-thinking-level-differential-green-readability.out` records the corrected fixture's 1 passed/0 failed GREEN.
- The strict public-index proof is implemented in `oracle/public-index-typecheck.mjs` and registered by `./run-tests.sh oracle`; the empty initial oracle/Node captures are not execution evidence. Self-describing final proof artifacts record the command, revision, and exit.
- The initial differential fixture builder caused a 34-line readability RED; the immutable `CASES` table correction preserves request content and source order.

## Final integration proof

- [x] Run `cargo test --workspace --all-targets`: 264 passed, 0 failed, and 2 ignored at code/proof HEAD `716f1dd`.
- [x] Run workspace all-target check and all-target/all-feature Clippy with `-D warnings`: both exit 0.
- [x] Run oracle/typeproof, formatting, both Node syntax checks, and diff checks with self-describing artifacts: all exit 0.
- [x] Complete Rust/TypeScript/JavaScript readability, documentation, dependency, and clean-pin audits.

## Known gaps

- [ ] Inherited independent verification remains pending after documentation closure.

## Out of scope

Parsing, `FromStr`, `Display`, serde, validation, provider support or clamping, model resolution, coding-agent defaults, protocol integration or conversion, dependencies, invalid runtime inputs, broad suites, and adjacent APIs remain deferred. `Ultra` is valid here because the agent-harness union is broader than the distinct protocol schema.
