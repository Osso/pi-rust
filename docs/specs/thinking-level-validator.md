# Thinking-level validator

This specification covers `packages/coding-agent/src/cli/args.ts:67-70` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The pinned coding-agent package index at `index.ts:3` exports only `Args` and `parseArgs`, so Rust exposes the validator only at `pi_coding_agent::cli::args::is_valid_thinking_level`.

## What it must do

- [x] Keep the valid table private and ordered as the eight agent-harness `ThinkingLevel` literals: `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, `ultra`.
- [x] Return `true` for those exact string values and `false` for other Rust-representable strings.
- [x] Expose only `is_valid_thinking_level(&str) -> bool` from the direct `cli::args` module; do not expose the valid table or add `cli`/crate-root re-exports.
- [x] Reuse the existing exact local `pi-agent-core` dependency for the typed agent-harness values; add no dependency.
- [x] Focused RED/GREEN and targeted Clippy, formatting, diff, and readability gates pass.
- [x] Compare the direct TypeScript runtime through request `13177` and a standalone exact type-predicate proof.

## Language-boundary discrepancy

TypeScript declares a caller-side narrowing predicate, `level is ThinkingLevel`. Rust exposes the approved typed-core equivalent as `bool`; it does not substitute a parser, `Option<ThinkingLevel>`, enum conversion, or validation API.

## Implementation inventory

- `crates/pi-coding-agent/src/cli/args.rs` — private eight-value table and direct-module predicate.
- `crates/pi-coding-agent/tests/thinking_level_validator.rs` — focused behavioral proof.
- `crates/pi-coding-agent/tests/differential_thinking_level_validator.rs` and `crates/pi-coding-agent/tests/differential_thinking_level_validator/mod.rs` — request `13177` proof.
- `oracle/thinking-level-validator-oracle.ts` — direct-module runtime oracle.
- `oracle/thinking-level-validator-typecheck.mjs` — source-lock, type-predicate, narrowing, runtime, and private-export proof.
- `run-tests.sh` — focused, differential, aggregate oracle, and standalone-proof registration.
- `docs/porting/traceability/thinking-level-validator.md` — source mapping and bounded proof record.

## Bounded differential/typeproof proof

- [x] RED `/tmp/pi-rust-thinking-level-validator-differential-red.out` exits `101` because request `13177` receives structured `unsupported operation: thinking_level_validator` instead of a success response.
- [x] GREEN `/tmp/pi-rust-thinking-level-validator-differential-green.out` exits `0` with `1 passed, 0 failed`; the request compares valid literals in source order plus `""`, case variants, leading/trailing whitespace, `x-high`, `extreme`, `unsupported`, `é😀`, U+0000, and U+000A.
- [x] `/tmp/pi-rust-thinking-level-validator-oracle.out` and `/tmp/pi-rust-thinking-level-validator-aggregate-oracle.out` exit `0`; the standalone proof checks the exact type predicate, caller narrowing, valid/invalid runtime cases, private-table non-export, source lock, pinned HEAD, and the pinned highlight declaration root. Its `--skipLibCheck` matches the pinned `tsconfig.base.json`; strict source-file checking remains enabled.
- [x] Node syntax, targeted denied-warning Clippy, formatting, diff, and Rust/TypeScript/JavaScript readability artifacts pass: `/tmp/pi-rust-thinking-level-validator-node-oracle.out`, `/tmp/pi-rust-thinking-level-validator-node-typeproof.out`, `/tmp/pi-rust-thinking-level-validator-clippy.out`, `/tmp/pi-rust-thinking-level-validator-fmt.out`, `/tmp/pi-rust-thinking-level-validator-diff.out`, and `/tmp/pi-rust-thinking-level-validator-readability.out`.

## Remaining verification

Broad workspace/check/Clippy, final integration, documentation closure, clean-pin, and inherited verification remain pending.

## Out of scope

`Mode`, `Args`, `parseArgs`, diagnostics, help, dispatch, public valid-table exports, parser/enum conversion, `FromStr`, `Display`, serde, provider/model/default behavior, non-string runtime inputs, package/root exports, protocol or agent-core changes, dependencies, and unrelated APIs remain excluded.
