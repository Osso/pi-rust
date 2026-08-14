# AppliedEditsResult traceability

Pinned source: `packages/agent/src/harness/tools/edit-diff.ts:192-195` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `edit-diff.ts:192-195` | Direct-module exported mutable interface with required `baseContent: string` and `newContent: string` | `pi_agent_core::harness::tools::edit_diff::AppliedEditsResult` uses public mutable `base_content: String` and `new_content: String` in source order; no tools/harness/crate-root re-export. |
| `harness/tools` index and package index | `AppliedEditsResult` is not re-exported | Rust preserves the direct-module-only boundary. |
| Differential request `13183` | Interfaces erase at runtime; ordinary records preserve representable field values | Four independent source-shaped cases compare through `applied_edits_result`; no runtime interface identity or edit-application behavior is claimed. |
| Direct-module type proof | Required mutable fields are exactly strings; missing, wrong, and excess fresh-object cases fail | `oracle/edit-record-typecheck.mjs` proves the real declaration, valid mutable assignments, and two-record invalid diagnostics without suppression. |
| `edit-diff.ts:302-305` | `applyEditsToNormalizedContent` returns the record | Producer behavior remains excluded; this slice does not port matching, normalization, validation, offsets, ordering, or application. |

## Rust boundary

`AppliedEditsResult` derives `Debug` only. No constructor, `Default`, `Clone`, equality, serde, validation, normalization, or conversion traits are added. Rust strings cover representable Unicode scalars, empty values, NUL, and line breaks; invalid JavaScript runtime values and lone UTF-16 surrogates remain outside the typed boundary.

## Proof artifacts

- Focused RED/GREEN: `/tmp/pi-rust-applied-edits-result-focused-red.out` and `/tmp/pi-rust-applied-edits-result-focused-green.out`.
- Differential RED/GREEN: `/tmp/pi-rust-applied-edits-result-differential-red.out` and `/tmp/pi-rust-applied-edits-result-differential-green.out`.
- Dedicated direct-module typeproof: `/tmp/pi-rust-edit-record-typeproof.out`; it proves both `Edit` and `AppliedEditsResult`, with two valid assignments and four exact invalid diagnostics for each record.
- Production commit: `39bb794`; proof commit: `27ac88c`.

## Final integration proof

- `/tmp/pi-rust-applied-edits-result-final-workspace-tests.out` records `cargo test --workspace --all-targets` exiting 0 at `b27569b`; all 65 result lines sum to `296 passed, 0 failed, 2 ignored`.
- `/tmp/pi-rust-applied-edits-result-final-focused.out`, `...-differential.out`, `...-workspace-check.out`, and `...-workspace-clippy.out` pass.
- `/tmp/pi-rust-applied-edits-result-final-oracle.out`, `...-typeproof.out`, `...-node-check.out`, `...-fmt.out`, `...-diff.out`, and `...-readability.out` pass.
- `/tmp/pi-rust-applied-edits-result-final-audit.out` confirms exact direct-module boundaries, request `13183`, four fixtures, truthful RED artifacts, intact `Edit` proof, no Cargo/dependency/protocol/deployment changes, and clean pins.

Final executable gates pass. Inherited independent verification at clean Rust HEAD `76d759f` returns `OVERALL: PASS`; broad evidence remains valid from `b27569b` because the intervening delta is documentation-only.
