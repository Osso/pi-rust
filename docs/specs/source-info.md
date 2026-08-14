# SourceInfo

This specification covers `packages/coding-agent/src/core/source-info.ts` at pinned TypeScript commit `532f7d12cd19235081bd4d6e425bde185652d86f`. `SourceInfo` is package-visible through the extensions index; `createSyntheticSourceInfo` is exported from `packages/coding-agent/src/index.ts:292`. Production is `37a8cc2`; differential/typeproof proof is `d3a4b08`.

## What it must do

- [x] Preserve `SourceScope` literals `user`, `project`, and `temporary`.
- [x] Preserve `SourceOrigin` literals `package` and `top-level`.
- [x] Expose mutable `SourceInfo` fields `path`, `source`, `scope`, `origin`, and optional `baseDir`.
- [x] Expose `createSyntheticSourceInfo` with default scope `temporary` and origin `top-level`.
- [x] Preserve supplied scope, origin, baseDir, and boundary strings without mutating options or returned values.
- [x] Use approved public Rust carrier `SyntheticSourceInfoOptions` for the anonymous TypeScript options object.
- [x] Compare ten source-shaped cases through exactly request `13182`.
- [x] Prove real source/package exports, exact unions/records/function signature, explicit undefined acceptance, and four structural invalid diagnostics.

## Rust boundary

- `pi_coding_agent::core::source_info::{SourceScope, SourceOrigin, SourceInfo, SyntheticSourceInfoOptions, create_synthetic_source_info}`
- Crate root re-exports only `SourceInfo` and `create_synthetic_source_info`.
- `SyntheticSourceInfoOptions` is an explicit approved compatibility deviation: TypeScript uses an anonymous object parameter, while Rust requires a public named carrier for a public function signature. This approval is limited to this function and does not authorize general invented carriers.

## Explicit gaps

`PathMetadata` and `createSourceInfo` remain unported because `PathMetadata` belongs to `core/package-manager.ts` and its consumers are a separate package-manager boundary. Extensions, package resolution, resources, prompts, sessions, and other SourceInfo consumers remain excluded. Null runtime inputs, omission-versus-undefined identity, extra traits, serialization, dependencies, and deployment remain out of scope.

## Proof status

- [x] Focused RED/GREEN and production commit `37a8cc2`.
- [x] Differential RED/GREEN and real package runtime/typeproof commit `d3a4b08`.
- [ ] Final workspace/static/clean-pin/inherited verification.
