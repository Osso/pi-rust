# Tool-output persistence constants

This specification covers `packages/coding-agent/src/core/session-tool-output.ts:19-20` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The coding-agent package re-exports both constants from `packages/coding-agent/src/index.ts:262-269`; Rust exposes the module and crate-root paths while `core` does not aggregate-re-export them. Focused production is committed as `1a8077a`; differential/public-index proof is committed as `66e30fa`.

## What it must do

- [x] Expose `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` from `pi_coding_agent::core::session_tool_output` and the crate root with value `1_048_576`.
- [x] Expose `TOOL_RESULT_TRUNCATION_MARKER` from `pi_coding_agent::core::session_tool_output` and the crate root with exact value `"\n[tool output truncated at 1 MiB]"`.
- [x] Preserve the package-exported constant boundary without adding a `core` aggregate re-export.
- [x] Use `usize` only as a narrow Rust exception for the exact TypeScript numeric literal: all pinned source uses are byte limits. This does not generalize JavaScript `number` mappings.
- [x] Cover both module and crate-root paths and exact values in the focused behavioral test.
- [x] Compare the source values through exactly one differential request, `13178`.
- [x] Prove exact coding-agent public-index declaration types through the pinned TypeScript compiler API, scoped to the real index and defining module; require real-index aliases and exact number/string literal types.
- [x] Prove the exact runtime values through a package-index import; the runtime probe returns `1048576` and `"\n[tool output truncated at 1 MiB]`.

## How it works

- [Tool-output persistence constants traceability](../porting/traceability/tool-output-persistence-constants.md)

## Implementation inventory

- `crates/pi-coding-agent/src/core/session_tool_output.rs` — module constants.
- `crates/pi-coding-agent/src/core/mod.rs` — public module exposure without aggregate re-exports.
- `crates/pi-coding-agent/src/lib.rs` — crate-root re-exports.
- `crates/pi-coding-agent/tests/session_tool_output_constants.rs` — focused value and export proof.
- `crates/pi-coding-agent/tests/differential_tool_output_persistence_constants.rs` and `.../mod.rs` — one-request differential proof.
- `oracle/tool-output-persistence-constants-oracle.ts` — direct-module JSONL oracle.
- `oracle/tool-output-persistence-constants-typecheck.mjs` — symbol-scoped real-index compiler and pinned `tsx` runtime proof.
- `run-tests.sh` — differential and oracle registration.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/session_tool_output_constants.rs`
- `crates/pi-coding-agent/tests/differential_tool_output_persistence_constants.rs`
- `oracle/tool-output-persistence-constants-typecheck.mjs`

## Bounded proof status

- [x] Differential RED/GREEN: structured unsupported-operation RED and `1 passed, 0 failed` GREEN for request `13178`.
- [x] Direct oracle source typecheck, Node syntax, targeted denied-warning Clippy, formatting, diff, and changed Rust/TypeScript/JavaScript readability pass.
- [x] Runtime package-index probe returns both exact values.
- [x] Symbol-scoped declaration proof passes through the real pinned index and defining module: compiler version `5.9.3`, zero scoped diagnostics, real-index aliases, exact number literal `1048576`, and exact string literal. Unrelated transitive files are not required to be globally clean; no source suppression or copied declaration was used.
- [x] Final integration at code/proof HEAD `66e30fa`: workspace `278 passed, 0 failed, 2 ignored`; workspace check, all-target/all-feature Clippy with `-D warnings`, aggregate oracle/typeproof, dedicated typeproof, Node syntax, formatting, slice-range diff, changed-file readability, documentation, dependency/lock, and clean-pin gates pass.
- [x] The Rust worktree is clean at `66e30fa`; the source worktree is clean at `532f7d12cd19235081bd4d6e425bde185652d86f`.
- [ ] Inherited independent verification remains pending.

No dependency change or deployment applies to this slice.

## Out of scope

Migration reports and functions, truncation behavior, UTF-8 and serialization rules, filesystem/session/message/content behavior, malformed-file recovery, backups, constructors, mutable identity, dependencies, protocol/agent-core behavior, and unrelated APIs remain excluded.
