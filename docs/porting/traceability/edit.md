# Edit traceability

Pinned source: `packages/agent/src/harness/tools/edit-diff.ts:187-190` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `edit-diff.ts:187-190` | Direct-module exported mutable interface with required `oldText: string` and `newText: string` | `harness::tools::edit_diff::Edit` uses public mutable `old_text: String` and `new_text: String` in source order; focused tests cover empty, Unicode, NUL, newline, and mutation states. |
| `harness/tools` index and package index | `Edit` is not re-exported | Rust adds no `harness::tools`, harness-root, crate-root, or package-equivalent re-export. |
| Differential request `13181` | Interfaces erase at runtime; ordinary records preserve representable field values | Four independent source-shaped cases compare through `edit_record`; no runtime interface identity or edit-algorithm behavior is claimed. |
| Direct-module type proof | Required mutable fields are exactly strings; missing, wrong, and excess fresh-object cases fail | `oracle/edit-record-typecheck.mjs` resolves the real direct module, proves exact fields and direct-only boundary, and checks four unsuppressed diagnostics. |
| Adjacent `AppliedEditsResult` (`edit-diff.ts:192-195`) | Separate direct-module export tied to edit-application output | Explicit deferred gap; no Rust type or application behavior is claimed. |

## Rust boundary

`Edit` derives `Debug` only. No constructor, `Default`, `Clone`, equality, serde, validation, normalization, or conversion traits are added. Rust strings cover representable Unicode scalars, empty values, NUL, and line breaks; invalid JavaScript runtime values and lone UTF-16 surrogates remain outside the typed boundary.

## Proof artifacts

- Focused RED/GREEN: `/tmp/pi-rust-edit-record-focused-red.out` and `/tmp/pi-rust-edit-record-focused-green.out`.
- Differential RED/GREEN: `/tmp/pi-rust-edit-record-differential-red.out` and `/tmp/pi-rust-edit-record-differential-green.out`.
- Direct-module typeproof: `/tmp/pi-rust-edit-record-typeproof.out`.
- Production commit: `7085ae5`; proof commit: `4b5fa7b`.

## Exclusions

Fuzzy matching, Unicode normalization, matching/validation, offsets, overlap and replacement ordering, edit application, filesystem/tool behavior, `AppliedEditsResult`, extra traits/APIs, package/root exports, dependencies, deployment, protocol behavior, and unrelated APIs remain unported. Final workspace/check/all-feature Clippy, clean-pin, and inherited verification remain pending.
