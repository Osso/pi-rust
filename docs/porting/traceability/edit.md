# Edit traceability

Pinned source: `packages/agent/src/harness/tools/edit-diff.ts:187-195` at `532f7d12cd19235081bd4d6e425bde185652d86f`. [AppliedEditsResult traceability](applied-edits-result.md) records the separate result-record slice.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `edit-diff.ts:187-190` | Direct-module exported mutable interface with required `oldText: string` and `newText: string` | `harness::tools::edit_diff::Edit` uses public mutable `old_text: String` and `new_text: String` in source order; focused tests cover empty, Unicode, NUL, newline, and mutation states. |
| `harness/tools` index and package index | `Edit` is not re-exported | Rust adds no `harness::tools`, harness-root, crate-root, or package-equivalent re-export. |
| Differential request `13181` | Interfaces erase at runtime; ordinary records preserve representable field values | Four independent source-shaped cases compare through `edit_record`; no runtime interface identity or edit-algorithm behavior is claimed. |
| Direct-module type proof | Required mutable fields are exactly strings; missing, wrong, and excess fresh-object cases fail | `oracle/edit-record-typecheck.mjs` resolves the real direct module, proves exact fields and direct-only boundary, and checks four unsuppressed diagnostics. |
| `AppliedEditsResult` (`edit-diff.ts:192-195`) | Separate direct-module export with required mutable `baseContent` and `newContent` strings | Completed separately as `harness::tools::edit_diff::AppliedEditsResult`; application behavior remains excluded. |

## Rust boundary

`Edit` derives `Debug` only. No constructor, `Default`, `Clone`, equality, serde, validation, normalization, or conversion traits are added. Rust strings cover representable Unicode scalars, empty values, NUL, and line breaks; invalid JavaScript runtime values and lone UTF-16 surrogates remain outside the typed boundary.

## Proof artifacts

- Focused RED/GREEN: `/tmp/pi-rust-edit-record-focused-red.out` and `/tmp/pi-rust-edit-record-focused-green.out`.
- Differential RED/GREEN: `/tmp/pi-rust-edit-record-differential-red.out` and `/tmp/pi-rust-edit-record-differential-green.out`.
- Direct-module typeproof: `/tmp/pi-rust-edit-record-typeproof.out`.
- `Edit` production/proof: `7085ae5`/`4b5fa7b`.
- `AppliedEditsResult` production/proof: `39bb794`/`27ac88c`.

## Final integration proof

- `/tmp/pi-rust-edit-record-final-workspace-tests.out` records `cargo test --workspace --all-targets` exiting 0 at `3200027`; all 62 result lines sum to `289 passed, 0 failed, 2 ignored`.
- `/tmp/pi-rust-edit-record-final-focused.out`, `...-differential.out`, `...-workspace-check.out`, and `...-workspace-clippy.out` pass.
- `/tmp/pi-rust-edit-record-final-oracle.out`, `...-typeproof.out`, `...-node-check.out`, `...-fmt.out`, `...-diff.out`, and `...-readability.out` pass at `3200027`.
- `/tmp/pi-rust-edit-record-final-audit.out` records the earlier `Edit`-only boundary at `3200027`; `AppliedEditsResult` is now tracked by its separate [traceability](applied-edits-result.md), while edit-application behavior remains excluded.
- Inherited independent verification at clean Rust HEAD `3200027` returns `OVERALL: PASS`.

## Exclusions

Fuzzy matching, Unicode normalization, matching/validation, offsets, overlap and replacement ordering, edit application, filesystem/tool behavior, extra traits/APIs, package/root exports, dependencies, deployment, protocol behavior, and unrelated APIs remain unported. `AppliedEditsResult` application/producer behavior remains excluded. Combined executable gates pass at `b27569b`; inherited verification for the AppliedEditsResult documentation closure passes at `76d759f` (`OVERALL: PASS`). No deployment applies.
