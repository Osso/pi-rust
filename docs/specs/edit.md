# Edit

This specification covers the direct-module edit records in `packages/agent/src/harness/tools/edit-diff.ts:187-195` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The source package and `harness/tools` indexes do not re-export either record; Rust preserves only the direct module path. `Edit` production/proof are `7085ae5`/`4b5fa7b`; `AppliedEditsResult` production/proof are `39bb794`/`27ac88c`. See [AppliedEditsResult](applied-edits-result.md) for its separate contract.

## What it must do

- [x] Expose `Edit` from `pi_agent_core::harness::tools::edit_diff`.
- [x] Preserve exactly two public mutable required fields, in source order: `old_text: String` and `new_text: String`.
- [x] Derive `Debug` only; add no constructors, defaults, cloning, equality, serialization, validation, or conversion behavior.
- [x] Compare four independent source-shaped ordinary records through exactly one differential request, `13181`, operation `edit_record`.
- [x] Prove the real direct-module declaration, exact required mutable string fields, invalid missing/wrong/excess cases, and omission from the real `harness/tools` and package indexes.

## Implementation inventory

- `crates/pi-agent-core/src/harness/tools/edit_diff.rs` — direct-module record.
- `crates/pi-agent-core/tests/edit_record.rs` — focused field, string, mutation, and `Debug` proof.
- `crates/pi-agent-core/tests/differential_truncate/edit_record.rs` — request `13181` source-shaped comparison.
- `oracle/truncate-oracle.ts` — source-shaped record oracle operation.
- `oracle/edit-record-typecheck.mjs` — direct-module compiler/export proof.

## Bounded proof status

- [x] Focused RED/GREEN and production commit `7085ae5` for `Edit`.
- [x] Differential RED at `7085ae5` and GREEN/typeproof commit `4b5fa7b` for request `13181`.
- [x] Four independent raw source-shaped `Edit` cases compare through request `13181`; the erased interface has no runtime identity claim.
- [x] `AppliedEditsResult` production/proof commit `39bb794`/`27ac88c` adds request `13183`; four independent raw records and the existing direct-module typeproof pass.
- [ ] Final workspace/check/all-feature Clippy, documentation/dependency/clean-pin, and inherited verification remain pending.

## Boundary and exclusions

`AppliedEditsResult` is a separate completed direct-module record slice; its algorithmic producer remains excluded. Fuzzy matching, Unicode normalization, matching/validation, offsets, overlap and replacement ordering, edit application, filesystem/tool behavior, and package/harness/crate-root exports remain excluded.

No dependency, deployment, protocol, or unrelated API change applies.
