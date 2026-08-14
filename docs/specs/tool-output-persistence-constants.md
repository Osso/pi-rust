# Tool-output persistence constants

This specification covers `packages/coding-agent/src/core/session-tool-output.ts:19-20` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The coding-agent package re-exports both constants from `packages/coding-agent/src/index.ts:262-269`; Rust exposes the module and crate-root paths while `core` does not aggregate-re-export them. Differential/typeproof and final verification remain pending.

## What it must do

- [x] Expose `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` from `pi_coding_agent::core::session_tool_output` and the crate root with value `1_048_576`.
- [x] Expose `TOOL_RESULT_TRUNCATION_MARKER` from `pi_coding_agent::core::session_tool_output` and the crate root with exact value `"\n[tool output truncated at 1 MiB]"`.
- [x] Preserve the package-exported constant boundary without adding a `core` aggregate re-export.
- [x] Use `usize` only as a narrow Rust exception for the exact TypeScript numeric literal: all pinned source uses are byte limits. This does not generalize JavaScript `number` mappings.
- [x] Cover both module and crate-root paths and exact values in the focused behavioral test.
- [ ] Compare the source values through exactly one differential request, `13178`.
- [ ] Prove the exact coding-agent public-index declaration types and runtime values through the strict public-index proof.

## How it works

- [Tool-output persistence constants traceability](../porting/traceability/tool-output-persistence-constants.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/session_tool_output.rs` — module constants.
- `crates/pi-coding-agent/src/core/mod.rs` — public module exposure without aggregate re-exports.
- `crates/pi-coding-agent/src/lib.rs` — crate-root re-exports.
- `crates/pi-coding-agent/tests/session_tool_output_constants.rs` — focused value and export proof.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/session_tool_output_constants.rs`

## Known gaps (current cycle)

- [ ] Add differential request `13178` against the pinned direct source module and package export.
- [ ] Add strict coding-agent public-index exact type/value proof.
- [ ] Run final workspace/static/readability/documentation/dependency/pin gates and inherited verification.

## Out of scope

Migration reports and functions, truncation behavior, UTF-8 and serialization rules, filesystem/session/message/content behavior, malformed-file recovery, backups, constructors, mutable identity, dependencies, protocol/agent-core behavior, and unrelated APIs remain excluded.
