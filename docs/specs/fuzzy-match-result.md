# FuzzyMatchResult

This specification covers the direct-module result interface in `packages/agent/src/harness/tools/edit-diff.ts:171-184` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The `harness/tools` and package indexes omit it; Rust preserves only `pi_agent_core::harness::tools::edit_diff::FuzzyMatchResult`.

## What it must do

- [x] Preserve exactly five public mutable fields in source order: `found: bool`, `index: f64`, `match_length: f64`, `used_fuzzy_match: bool`, and `content_for_replacement: String`.
- [x] Derive `Debug` only; add no constructors, defaults, cloning, equality, serialization, validation, or conversion behavior.
- [x] Preserve empty, Unicode, NUL, newline, and CRLF replacement content.
- [x] Preserve independent public mutation of both boolean fields and the replacement-content field.
- [x] Preserve unrestricted binary64 bit patterns in `index` and `match_length`, including signed zero, subnormal, finite, infinity, and NaN payload cases.
- [ ] Compare source-shaped records through exactly one differential request, `13184`.
- [ ] Prove the real direct-module declaration, exact mutable field types, invalid cases, and omission from the real `harness/tools` and package indexes.

## How it works

- [Pinned source traceability](../porting/traceability/fuzzy-match-result.md)

## Implementation inventory

- `crates/pi-agent-core/src/harness/tools/edit_diff.rs` — direct-module result record.
- `crates/pi-agent-core/tests/fuzzy_match_result.rs` — focused shape, mutation, string-boundary, `Debug`, and binary64 coverage.
- `packages/agent/src/harness/tools/edit-diff.ts` — pinned source contract.

## Tests asserting this spec

- `crates/pi-agent-core/tests/fuzzy_match_result.rs`

## Known gaps (current cycle)

- [ ] Add differential request `13184` against the pinned TypeScript source.
- [ ] Add strict direct-module type/export proof.
- [ ] Complete final workspace, static, documentation, clean-pin, and inherited verification.

## Out of scope

`fuzzyFindText`, normalization, matching/index semantics and invariants, edit application, filesystem/tool behavior, extra traits, dependencies, deployment, protocol behavior, and package/harness/crate-root exports remain unported. This slice proves only the erased direct-module record shape; it does not implement fuzzy matching or validate its result fields.
