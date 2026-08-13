# Summarization system prompt

This specification covers the module-local `SUMMARIZATION_SYSTEM_PROMPT` constant from `packages/agent/src/harness/compaction/compaction.ts:489-491`. The pinned package index omits this constant, so the exact Rust contract is module-only; prompt construction and model execution remain deferred.

## What it must do

- [x] Expose `SUMMARIZATION_SYSTEM_PROMPT` as a compile-time `pub const &str` through `pi_agent_core::harness::compaction`.
- [x] Preserve the exact 310-byte, 310-character, 310-UTF-16-unit ASCII value.
- [x] Preserve exactly three lines separated by two adjacent LF characters, including one blank middle line.
- [x] Preserve zero CR characters, zero trailing whitespace, and no trailing LF.
- [x] Compare the module constant directly with the pinned TypeScript source through request `13129` and a standalone direct-module literal type proof, without claiming package-index/public-index proof.

## How it works

- [Summarization system prompt notes](../wiki/systems/summarization-system-prompt.md)
- [Summarization system prompt traceability](../porting/traceability/summarization-system-prompt.md)
- [Compaction compatibility matrix](compatibility.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/compaction/mod.rs` — module-local exact constant.
- `crates/pi-agent-core/tests/summarization_system_prompt.rs` — focused value, metric, line-structure, and const-usage proof.
- `crates/pi-agent-core/tests/differential_truncate/summarization_system_prompt.rs` — direct request `13129` against the pinned TypeScript value.
- `oracle/truncate-oracle.ts` — direct compaction-module runtime adapter and metrics normalization.
- `oracle/summarization-system-prompt-typecheck.mjs` — direct-module exact literal type proof.
- `crates/pi-agent-core/tests/differential_truncate/mod.rs` — persistent oracle source-module and request registration.
- `run-tests.sh` — focused, differential, and direct typeproof test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/summarization_system_prompt.rs`
- `crates/pi-agent-core/tests/differential_truncate/summarization_system_prompt.rs`
- `oracle/summarization-system-prompt-typecheck.mjs`

## Known gaps (current cycle)

- [x] Add direct-module differential request `13129` against the pinned TypeScript source.
- [x] Add direct-module type proof for the exact static export while preserving package-index omission.
- [x] Audit proof-status wording, export-boundary wording, inventory, and cross-links at code HEAD `08e105a`.
- [x] Complete final workspace, static, readability, documentation, and clean-pin gates: 238 workspace tests passed with 0 failures and 2 ignored; all-target check and all-feature Clippy with denied warnings passed; fmt/diff passed; pinned source and worktree were exact and clean.
- [ ] Complete inherited independent verification.

## Out of scope

The package-index and `pi_agent_core::harness` root do not export this module-local constant. The duplicate coding-agent constant, `SUMMARIZATION_PROMPT`, prompt construction, compaction and branch orchestration, model calls, retries, serialization, token estimation, session behavior, dynamic formatting, dependencies, and adjacent compaction APIs remain deferred.
