# AgentToolExecutionContext

This specification covers `packages/agent/src/types.ts:387-393` and its package export through `packages/agent/src/index.ts:145` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

## What it must do

- [x] Expose `AgentToolExecutionContext` from `pi_agent_core::harness::types` and re-export it from `pi_agent_core::harness`.
- [x] Preserve public mutable fields in source order: `started_at: f64` and `agent_id: Option<String>`.
- [x] Derive `Debug` only.
- [x] Preserve unrestricted source `number` values through Rust `f64`, including the focused binary64 bit patterns.
- [x] Preserve absent and present optional `agentId` states and representable strings, including Unicode, NUL, and newline content.
- [x] Cover the module path, harness-root path, mutation, optional states, binary64 values, and strings in the focused test; GREEN is 5 passed and 0 failed.
- [ ] Compare source-shaped records through exactly eight persistent differential request groups `13168..=13175`.
- [ ] Prove the exact public-index interface shape, mutability, optionality, and `number` identity.

## Implementation inventory

- `crates/pi-agent-core/src/harness/types.rs` — value record.
- `crates/pi-agent-core/src/harness/mod.rs` — harness-root re-export.
- `crates/pi-agent-core/tests/agent_tool_execution_context.rs` — focused behavioral proof.
- `run-tests.sh` — focused registration; no differential/oracle registration exists yet.

## Focused proof

- RED: `/tmp/pi-rust-agent-tool-execution-context-red.out` records exit 101 in a clean temporary worktree before production changes, with exactly the missing module and harness-root imports.
- GREEN: `/tmp/pi-rust-agent-tool-execution-context-green.out` records 5 passed and 0 failed.
- Targeted denied-warning Clippy, formatting, and diff checks pass in `/tmp/pi-rust-agent-tool-execution-context-clippy.out`, `/tmp/pi-rust-agent-tool-execution-context-fmt.out`, and `/tmp/pi-rust-agent-tool-execution-context-diff.out`.
- Changed-Rust readability artifacts report no threshold violations. Differential, public-index, workspace, final static, documentation-closeout, and inherited verification remain pending.

## Boundary

`startedAt` is an unrestricted TypeScript `number` mapped to `f64`; no validation or integer narrowing is added. `agentId` is an optional Rust string. Invalid JavaScript runtime values and lone UTF-16 surrogates remain outside the typed Rust boundary.

The record preserves lifecycle metadata shape only. It does not implement timestamp capture, durable identity preparation, tool callbacks or results, producers, lifecycle validation, execution, scheduling, orchestration, streams, cancellation, sessions, providers, models, constructors, parsing, serde, defaults, fallbacks, protocol behavior, or coding-agent behavior.

## Pending proof

- Differential requests `13168..=13175` and their RED/GREEN oracle support.
- Strict public-index TypeScript typeproof.
- Workspace tests, workspace check, all-feature Clippy with `-D warnings`, final formatting/diff/oracle/Node gates, documentation/dependency/readability audits, clean pins, and inherited verification.

No dependency change is part of this slice.
