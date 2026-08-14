# FuzzyMatchResult traceability

Pinned source: `packages/agent/src/harness/tools/edit-diff.ts:171-184` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `edit-diff.ts:171-184` | Direct-module exported mutable interface with `found`, `index`, `matchLength`, `usedFuzzyMatch`, and `contentForReplacement` | `pi_agent_core::harness::tools::edit_diff::FuzzyMatchResult` uses public mutable `found: bool`, `index: f64`, `match_length: f64`, `used_fuzzy_match: bool`, and `content_for_replacement: String` in source order. |
| `harness/tools` index and package index | `FuzzyMatchResult` is not re-exported | Rust adds no tools-module, harness-root, crate-root, or package-equivalent re-export. |
| Focused Rust record coverage | Fields preserve empty/Unicode/NUL/newline/CRLF strings, public mutation, `Debug`, and unrestricted binary64 values | `crates/pi-agent-core/tests/fuzzy_match_result.rs` covers source-shaped fields, independent mutation, control/unicode content, and bit-exact numeric patterns including signed zero, subnormal, finite, infinity, and NaN payload values. |
| Differential request `13184` | Interfaces erase at runtime; ordinary records preserve representable field values | Eight independent binary64/source-shaped cases compare through one request; no runtime interface identity, fuzzy matching, or result-invariant claim. |
| Direct-module type proof | Required mutable field order and exact bool/number/string types; package-index omission | Real pinned module/index proof passes with two valid assignments and nine exact structural invalid diagnostics. |

## Rust boundary

`FuzzyMatchResult` derives `Debug` only. JavaScript `number` fields map to Rust `f64` so all binary64 bit patterns remain representable; this does not add numeric validation or matching semantics. Rust strings cover representable Unicode scalars, empty values, NUL, and line breaks; invalid JavaScript runtime values and lone UTF-16 surrogates remain outside the typed boundary.

## Proof status

- Focused Rust shape, mutation, `Debug`, string-boundary, and binary64 coverage: complete in `crates/pi-agent-core/tests/fuzzy_match_result.rs`; focused RED/GREEN, targeted Clippy/typecheck/fmt/diff/readability pass.
- Production commit: `1a8ba0c`; differential RED at `1a8ba0c` and GREEN/proof at `62e5770`.
- Differential request `13184`: complete with eight independent binary64/source-shaped cases.
- Real direct-module type/export proof: complete with two valid assignments and nine exact invalid diagnostics; direct-only index boundary passes.
- Final workspace/all-target check, all-feature Clippy, aggregate oracle, clean-pin, and inherited verification: pending.

## Exclusions

`fuzzyFindText`, normalization, matching/index semantics and invariants, edit application, filesystem/tool behavior, extra traits/APIs, dependencies, deployment, protocol behavior, and package/harness/crate-root exports remain unported. No claim is made about fuzzy-match algorithm output, offsets, validation, replacement behavior, producer behavior, or runtime interface identity.
