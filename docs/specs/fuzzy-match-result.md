# FuzzyMatchResult

This specification covers the direct-module result interface in `packages/agent/src/harness/tools/edit-diff.ts:171-184` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The `harness/tools` and package indexes omit it; Rust preserves only `pi_agent_core::harness::tools::edit_diff::FuzzyMatchResult`.

## What it must do

- [x] Preserve exactly five public mutable fields in source order: `found: bool`, `index: f64`, `match_length: f64`, `used_fuzzy_match: bool`, and `content_for_replacement: String`.
- [x] Derive `Debug` only; add no constructors, defaults, cloning, equality, serialization, validation, or conversion behavior.
- [x] Preserve empty, Unicode, NUL, newline, and CRLF replacement content.
- [x] Preserve independent public mutation of all five fields.
- [x] Preserve unrestricted binary64 bit patterns in `index` and `match_length`, including signed zero, subnormal, finite, infinity, and NaN payload cases.
- [x] Compare eight independent binary64/source-shaped records through exactly one differential request, `13184`; no runtime interface identity or result-invariant claim.
- [x] Prove the real direct-module declaration, exact mutable field types, two valid assignments, nine exact structural invalid diagnostics, and omission from the real `harness/tools` and package indexes.

## How it works

- [Pinned source traceability](../porting/traceability/fuzzy-match-result.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/tools/edit_diff.rs` — direct-module result record.
- `crates/pi-agent-core/tests/fuzzy_match_result.rs` — focused shape, mutation, string-boundary, `Debug`, and binary64 coverage.
- `crates/pi-agent-core/tests/differential_truncate/fuzzy_match_result.rs` — request `13184` source-shaped comparison.
- `oracle/truncate-oracle.ts` — independent ordinary-record normalization.
- `oracle/edit-record-typecheck.mjs` — real direct-module declaration and export proof.
- `packages/agent/src/harness/tools/edit-diff.ts` — pinned source contract.

## Tests asserting this spec

- `crates/pi-agent-core/tests/fuzzy_match_result.rs`
- `crates/pi-agent-core/tests/differential_truncate/fuzzy_match_result.rs`
- `oracle/edit-record-typecheck.mjs`

## Proof status

- [x] Focused RED at `2cc5386` and GREEN with four tests; targeted Clippy, typecheck, fmt, diff, and readability pass.
- [x] Production commit `1a8ba0c`; differential RED at `1a8ba0c` and GREEN at `62e5770` for request `13184` with eight independent cases.
- [x] Real direct-module type/export proof completes two valid assignments and nine exact invalid diagnostics; targeted Clippy/typecheck/fmt/diff/readability pass.
- [x] Final executable integration at `f802bf1`: workspace `300 passed, 0 failed, 2 ignored` across 66 result lines; workspace check, all-target/all-feature Clippy with `-D warnings`, aggregate oracle/typeproof/Node, fmt/diff, Rust/TS readability, docs/scope/dependency/lock, and clean-pin audit pass.
- [ ] Inherited verification remains pending only for this documentation closure.

## Out of scope

`fuzzyFindText`, normalization, matching/index semantics and invariants, edit application, filesystem/tool behavior, extra traits, dependencies, deployment, protocol behavior, and package/harness/crate-root exports remain unported. This slice proves only the erased direct-module record shape; it does not implement fuzzy matching, validate result fields, or claim producer behavior.
