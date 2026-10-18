# SourceInfo traceability

Pinned source: `packages/coding-agent/src/core/source-info.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `source-info.ts:3-4` | Closed `SourceScope` and `SourceOrigin` unions | `SourceScope` and `SourceOrigin` enums in `pi_coding_agent::core::source_info`; exact `as_str()` literals tested and type-proved. |
| `source-info.ts:6-12` | Mutable `SourceInfo` record with optional `baseDir` | `SourceInfo` with public mutable `path`, `source`, `scope`, `origin`, `base_dir: Option<String>`; module and crate-root paths tested. |
| `source-info.ts:24-43` | Fresh synthetic record; `scope ?? "temporary"`, `origin ?? "top-level"`, optional baseDir copied | `create_synthetic_source_info`; ten independent differential cases call the real pinned package function and compare returned fields/options state. |
| `index.ts:138`, `index.ts:292` | Package-visible `SourceInfo` and runtime `createSyntheticSourceInfo` | Crate root re-exports only `SourceInfo` and `create_synthetic_source_info`; dedicated typeproof confirms real source/package boundaries. |
| Anonymous options parameter | Required source plus optional scope/origin/baseDir | Approved Rust `SyntheticSourceInfoOptions` public carrier. This is a deliberate, limited API-shape deviation required by Rust public signatures. |
| `package-manager.ts:57-61`, `source-info.ts:14-22` | `PathMetadata` fields copied into a fresh `SourceInfo` | `create_source_info`; five differential cases call the pinned `createSourceInfo` and compare result + metadata state. Package-manager `SourceScope`/`MetadataOrigin` convert through `From` impls. |

## Boundary semantics

TypeScript accepts explicit `undefined` because the pinned compiler configuration has `exactOptionalPropertyTypes=false`; Rust maps those absence states to `None`. Rust does not distinguish omitted properties from explicit JavaScript `undefined`. Null runtime inputs are excluded from the typed Rust boundary, although JavaScript nullish coalescing would select the same defaults.

The module is metadata construction only; resource loading and consumers live in
`core::resource_loader`.

## Proof artifacts

- Focused RED/GREEN: `/tmp/pi-rust-source-info-focused-red.out` and `/tmp/pi-rust-source-info-focused-green.out`.
- Differential RED/GREEN: `/tmp/pi-rust-source-info-differential-red.out` and `/tmp/pi-rust-source-info-differential-green.out`.
- Aggregate oracle and dedicated typeproof: `/tmp/pi-rust-source-info-oracle.out` and `/tmp/pi-rust-source-info-typeproof.out`.
- Node syntax, targeted static, formatting, diff, and readability artifacts use `/tmp/pi-rust-source-info-*`.

Production commit: `37a8cc2`. Differential/typeproof commit: `d3a4b08`.

## Final integration proof

- `/tmp/pi-rust-source-info-final-workspace-tests.out` records `cargo test --workspace --all-targets` exiting 0 at `e966d3d`; all 64 result lines sum to `293 passed, 0 failed, 2 ignored`.
- `/tmp/pi-rust-source-info-final-focused.out`, `...-differential.out`, `...-workspace-check.out`, and `...-workspace-clippy.out` pass.
- `/tmp/pi-rust-source-info-final-oracle.out`, `...-typeproof.out`, `...-node-check.out`, `...-fmt.out`, `...-diff.out`, and `...-readability.out` pass at `e966d3d`.
- `/tmp/pi-rust-source-info-final-audit.out` confirms the approved carrier deviation, exact request `13182`, ten cases, explicit gaps, no Cargo/dependency/deployment changes, and clean Rust/source pins.
- Inherited independent verification at docs-only HEAD `e48a6c0` revalidates current focused/differential/oracle/typeproof/Node/fmt/diff/readability evidence, confirms the `e966d3d` broad artifacts remain valid, and returns `OVERALL: PASS`.

Final workspace/static/dependency/clean-pin and inherited verification pass. No deployment applies.
