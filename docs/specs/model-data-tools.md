# Model data maintenance tools

Offline maintenance parity for pinned `packages/ai/scripts/model-data.ts` and `check-model-data.ts`. [API/branch mapping](../porting/traceability/model-data-tools.md).

## What it must do

- [x] Discover exact generated imports, match shard names, read API-grouped provider structure, reject duplicates/empty catalogs.
- [x] Deduplicate exact allowlists; compare identifiers in JavaScript UTF-16 order.
- [x] Produce schema-3 manifests and SHA-256 hashes over exact UTF-8 content and JavaScript canonical structure JSON (including integer-key enumeration).
- [x] Validate directory/file/hash sets, schema, timestamp, generation stamp, model metadata and API grouping; accumulate diagnostics in source order and show first 30 plus remaining count.
- [x] Native checker runs without arguments from a Rust repository root, validates `crates/pi-ai/data/providers`, and retains source stdout/stderr/exit behavior.
- [x] Preserve raw JSON strings, keys, API values and diagnostics as UTF-16, including lone surrogates and their distinctness from U+FFFD/pairs.

Authorized adaptation, 2026-10-20: user answered “Check Rust catalog (Recommended)” to “The original catalog checker expects TypeScript import files; Rust stores its catalog as JSON in data/providers. Should the port check actual Rust catalog by default, or original TS layout?” The binary uses only the native path above, with expected identity/API groups from the existing compiled catalog. No flags or source-layout fallback. Original source-layout exported APIs remain separate.

## How it works

- [Maintenance traceability](../porting/traceability/model-data-tools.md)
- [Checker traceability](../porting/traceability/check-model-data.md)

## Implementation inventory

- `crates/pi-ai/src/model_data_tools.rs`: exported maintenance constants, records, operations, validation.
- `crates/pi-ai/src/bin/check-model-data.rs`: no-argument native checker; working directory is the Rust repository root.
- `crates/pi-js-compat/src/date.rs`: authorized shared prerequisite fixes for numeric/month/slash/pre-epoch HTTP timestamps; [date proof](../porting/traceability/js-compat.md#dateparse-srcdaters).

## Tests asserting this spec

- `crates/pi-ai/tests/model_data_tools.rs`: temp filesystem fixtures and exact pinned-module differentials.
- `oracle/model-data-oracle.ts`: real pinned API and unmodified checker source evaluated with fixture import-meta URL; stdout, stderr and exitCode captured without filtering.

## Proof / existing snapshot discrepancy

Targeted module development proof: 15/15 tests pass after native adaptation; parent owns final integration/verification. Shared lossless prerequisite is integrated. See traceability ledger for revisions and exact logs.

Existing checked-in snapshot has 40 provider files while `.manifest.json` lists 39 file hashes (missing `kimi-coding.json`). Native validation must report stale data; this task does not refresh or rewrite the real catalog. Positive tests construct a fresh manifest only inside isolated temporary fixtures.

## Out of scope

- Runtime model loading, generation, image generation, reasoning options, hydration/downloads, network, credentials, telemetry, real catalog writes.
- Standalone `pi-ai` login CLI remains its separate approved gap.
- Invalid JavaScript argument types to typed Rust `ModelDataStructure`, manifest constructor, paths and string allowlists are excluded. Untrusted JSON files remain dynamically validated; malformed metadata is not excluded.
