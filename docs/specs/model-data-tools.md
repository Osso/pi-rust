# Model data maintenance tools

Offline maintenance parity for pinned `packages/ai/scripts/model-data.ts` and `check-model-data.ts`. [API/branch mapping](../porting/traceability/model-data-tools.md).

## What it must do

- [ ] Discover exact generated imports, match shard names, read API-grouped provider structure, reject duplicates/empty catalogs.
- [ ] Deduplicate exact allowlists; compare identifiers in JavaScript UTF-16 order.
- [ ] Produce schema-3 manifests and SHA-256 hashes over exact UTF-8 content and JavaScript canonical structure JSON (including integer-key enumeration).
- [ ] Validate directory/file/hash sets, schema, timestamp, generation stamp, model metadata and API grouping; accumulate diagnostics in source order and show first 30 plus remaining count.
- [ ] Check an explicit AI-package root read-only; return TS success/failure exit code and exact stdout/stderr.

## How it works

- [Maintenance traceability](../porting/traceability/model-data-tools.md)
- [Checker traceability](../porting/traceability/check-model-data.md)

## Implementation inventory

- `crates/pi-ai/src/model_data_tools.rs`: exported maintenance constants, records, operations, validation.
- `crates/pi-ai/src/bin/check-model-data.rs`: native checker; positional `<ai-package-root>` replaces TS script-relative package location. Missing/extra argument emits usage and exits 2.

## Tests asserting this spec

- `crates/pi-ai/tests/model_data_tools.rs`: temp filesystem fixtures and exact pinned-module differentials.
- `oracle/model-data-oracle.ts`: real pinned API and unmodified checker source evaluated with fixture import-meta URL; stdout, stderr and exitCode captured without filtering.

## Known gaps (current cycle)

- [ ] GREEN pending initial implementation commit; RED recorded for absent API/binary.

## Out of scope

- Runtime model loading, generation, image generation, reasoning options, hydration/downloads, network, credentials, telemetry, real catalog writes.
- Standalone `pi-ai` login CLI remains its separate approved gap.
- Invalid JavaScript argument types to typed Rust `ModelDataStructure`, manifest constructor, paths and string allowlists are excluded. Untrusted JSON files remain dynamically validated; malformed metadata is not excluded.
