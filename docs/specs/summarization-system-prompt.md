# Summarization system prompt

This specification covers the module-local `SUMMARIZATION_SYSTEM_PROMPT` constant from `packages/agent/src/harness/compaction/compaction.ts:489-491`. It defines the exact Rust constant contract; prompt construction and model execution remain deferred.

## What it must do

- [x] Expose `SUMMARIZATION_SYSTEM_PROMPT` as a compile-time `pub const &str` through `pi_agent_core::harness::compaction`.
- [x] Preserve the exact 310-byte, 310-character, 310-UTF-16-unit ASCII value.
- [x] Preserve exactly three lines separated by two adjacent LF characters, including one blank middle line.
- [x] Preserve zero CR characters, zero trailing whitespace, and no trailing LF.
- [ ] Compare the module constant directly with the pinned TypeScript source through request `13129` and direct-module type proof.

## How it works

- [Summarization system prompt notes](../wiki/systems/summarization-system-prompt.md)
- [Compaction compatibility matrix](compatibility.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/compaction/mod.rs` — module-local exact constant.
- `crates/pi-agent-core/tests/summarization_system_prompt.rs` — focused value, metric, line-structure, and const-usage proof.
- `run-tests.sh` — focused test registration.

## Tests asserting this spec

- `crates/pi-agent-core/tests/summarization_system_prompt.rs`

## Known gaps (current cycle)

- [ ] Add direct-module differential request `13129` against the pinned TypeScript source.
- [ ] Add direct-module type proof for the exact static export while preserving package-index omission.
- [ ] Complete final workspace, static, readability, documentation, and clean-pin gates.
- [ ] Complete inherited independent verification.

## Out of scope

The package-index and `pi_agent_core::harness` root do not export this module-local constant. The duplicate coding-agent constant, `SUMMARIZATION_PROMPT`, prompt construction, compaction and branch orchestration, model calls, retries, serialization, token estimation, session behavior, dynamic formatting, dependencies, and adjacent compaction APIs remain deferred.
