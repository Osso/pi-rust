# Thinking-level validator

This specification covers `packages/coding-agent/src/cli/args.ts:67-70` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The pinned coding-agent package index at `index.ts:3` exports only `Args` and `parseArgs`, so Rust exposes the validator only at `pi_coding_agent::cli::args::is_valid_thinking_level`.

## What it must do

- [x] Keep the valid table private and ordered as the eight agent-harness `ThinkingLevel` literals: `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, `ultra`.
- [x] Return `true` for those exact string values and `false` for other Rust-representable strings.
- [x] Expose only `is_valid_thinking_level(&str) -> bool` from the direct `cli::args` module; do not expose the valid table or add `cli`/crate-root re-exports.
- [x] Reuse the existing exact local `pi-agent-core` dependency for the typed agent-harness values; add no dependency.
- [x] Focused RED/GREEN and targeted Clippy, formatting, diff, and readability gates pass.
- [ ] Compare the direct TypeScript runtime through request `13177` and a standalone exact type-predicate proof.

## Language-boundary discrepancy

TypeScript declares a caller-side narrowing predicate, `level is ThinkingLevel`. Rust exposes the approved typed-core equivalent as `bool`; it does not substitute a parser, `Option<ThinkingLevel>`, enum conversion, or validation API.

## Implementation inventory

- `crates/pi-coding-agent/src/cli/args.rs` — private eight-value table and direct-module predicate.
- `crates/pi-coding-agent/tests/thinking_level_validator.rs` — focused behavioral proof.
- `run-tests.sh` — focused registration.
- `docs/porting/traceability/thinking-level-validator.md` — source mapping and pending differential/typeproof proof.

## Pending proof

- Direct-module runtime oracle request `13177`.
- Standalone TypeScript proof of the exact `level is ThinkingLevel` predicate signature and eight-literal behavior.
- Broad workspace, final static, documentation closure, clean-pin, and inherited verification.

## Out of scope

`Mode`, `Args`, `parseArgs`, diagnostics, help, dispatch, public valid-table exports, parser/enum conversion, `FromStr`, `Display`, serde, provider/model/default behavior, non-string runtime inputs, package/root exports, protocol or agent-core changes, dependencies, and unrelated APIs remain excluded.
