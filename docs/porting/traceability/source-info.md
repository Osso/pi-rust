# SourceInfo traceability

Pinned source: `packages/coding-agent/src/core/source-info.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `source-info.ts:3-4` | Closed `SourceScope` and `SourceOrigin` unions | `SourceScope` and `SourceOrigin` enums in `pi_coding_agent::core::source_info`; exact `as_str()` literals tested and type-proved. |
| `source-info.ts:6-12` | Mutable `SourceInfo` record with optional `baseDir` | `SourceInfo` with public mutable `path`, `source`, `scope`, `origin`, `base_dir: Option<String>`; module and crate-root paths tested. |
| `source-info.ts:24-43` | Fresh synthetic record; `scope ?? "temporary"`, `origin ?? "top-level"`, optional baseDir copied | `create_synthetic_source_info`; ten independent differential cases call the real pinned package function and compare returned fields/options state. |
| `index.ts:138`, `index.ts:292` | Package-visible `SourceInfo` and runtime `createSyntheticSourceInfo` | Crate root re-exports only `SourceInfo` and `create_synthetic_source_info`; dedicated typeproof confirms real source/package boundaries. |
| Anonymous options parameter | Required source plus optional scope/origin/baseDir | Approved Rust `SyntheticSourceInfoOptions` public carrier. This is a deliberate, limited API-shape deviation required by Rust public signatures. |
| `package-manager.ts:57-61`, `source-info.ts:14-22` | `PathMetadata` and `createSourceInfo` are separate direct/package-manager contracts | Explicit gap; no package-manager stand-in or `create_source_info` function added. |

## Boundary semantics

TypeScript accepts explicit `undefined` because the pinned compiler configuration has `exactOptionalPropertyTypes=false`; Rust maps those absence states to `None`. Rust does not distinguish omitted properties from explicit JavaScript `undefined`. Null runtime inputs are excluded from the typed Rust boundary, although JavaScript nullish coalescing would select the same defaults.

The slice is metadata construction only. Package resolution, filesystem/resource loading, extension/session/prompt consumers, validation, serialization, and orchestration remain unported.

## Proof artifacts

- Focused RED/GREEN: `/tmp/pi-rust-source-info-focused-red.out` and `/tmp/pi-rust-source-info-focused-green.out`.
- Differential RED/GREEN: `/tmp/pi-rust-source-info-differential-red.out` and `/tmp/pi-rust-source-info-differential-green.out`.
- Aggregate oracle and dedicated typeproof: `/tmp/pi-rust-source-info-oracle.out` and `/tmp/pi-rust-source-info-typeproof.out`.
- Node syntax, targeted static, formatting, diff, and readability artifacts use `/tmp/pi-rust-source-info-*`.

Production commit: `37a8cc2`. Differential/typeproof commit: `d3a4b08`.

Final workspace, static, dependency/lock, clean-pin, and inherited verification remain pending.
