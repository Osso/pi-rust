# AppliedEditsResult

This specification covers the direct-module interface `packages/agent/src/harness/tools/edit-diff.ts:192-195` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. The `harness/tools` and package indexes omit it; Rust preserves only the direct module path. Production is `39bb794`; proof is `27ac88c`.

## What it must do

- [x] Expose `AppliedEditsResult` from `pi_agent_core::harness::tools::edit_diff`.
- [x] Preserve exactly two public mutable required fields, in source order: `base_content: String` and `new_content: String`.
- [x] Derive `Debug` only; add no constructors, defaults, cloning, equality, serialization, validation, or conversion behavior.
- [x] Compare four independent source-shaped ordinary records through exactly one differential request, `13183`, operation `applied_edits_result`.
- [x] Prove the real direct-module declaration, exact required mutable string fields, invalid missing/wrong/excess cases, and omission from the real `harness/tools` and package indexes.

## Implementation inventory

- `crates/pi-agent-core/src/harness/tools/edit_diff.rs` — direct-module record.
- `crates/pi-agent-core/tests/applied_edits_result.rs` — focused field, string, mutation, and `Debug` proof.
- `crates/pi-agent-core/tests/differential_truncate/applied_edits_result.rs` — request `13183` source-shaped comparison.
- `oracle/truncate-oracle.ts` — source-shaped record oracle operation.
- `oracle/edit-record-typecheck.mjs` — direct-module compiler/export proof for both edit records.

## Bounded proof status

- [x] Focused RED/GREEN and production commit `39bb794`.
- [x] Differential RED at `39bb794` and GREEN/typeproof at `27ac88c`.
- [x] Four independent raw source-shaped cases compare through request `13183`; the erased interface has no runtime identity claim.
- [x] Final executable integration at `b27569b`: workspace `296 passed, 0 failed, 2 ignored`; focused/differential, workspace check, all-target/all-feature Clippy with `-D warnings`, aggregate oracle, shared direct-module typeproof, Node syntax, formatting/diff, and changed-file readability pass.
- [ ] Inherited verification is pending only for this documentation closure.

## Boundary and exclusions

`applyEditsToNormalizedContent` remains unported. Fuzzy matching, Unicode normalization, matching/validation, offsets, overlap and replacement ordering, edit application, filesystem/tool behavior, extra traits/APIs, dependencies, deployment, protocol behavior, and unrelated APIs remain excluded. This slice proves only the direct-module record shape, not the algorithm that produces it.

No dependency or deployment applies.
