# Model data maintenance tools

Offline maintenance parity for pinned `packages/ai/scripts/model-data.ts` and `check-model-data.ts`. [API/branch mapping](../porting/traceability/model-data-tools.md).

## What it must do

- [ ] Discover exact generated imports, match shard names, read API-grouped provider structure, reject duplicates/empty catalogs.
- [x] Deduplicate exact allowlists; compare identifiers in JavaScript UTF-16 order.
- [x] Produce schema-3 manifests and SHA-256 hashes over exact UTF-8 content and JavaScript canonical structure JSON (including integer-key enumeration).
- [ ] Validate directory/file/hash sets, schema, timestamp, generation stamp, model metadata and API grouping; accumulate diagnostics in source order and show first 30 plus remaining count.
- [x] For an explicit TS-shaped fixture root, check read-only and return TS success/failure exit code and exact stdout/stderr.
- [ ] Preserve a useful native checker default/catalog layout; current provisional argument-only fixture checker does not satisfy the original no-argument workflow.

## How it works

- [Maintenance traceability](../porting/traceability/model-data-tools.md)
- [Checker traceability](../porting/traceability/check-model-data.md)

## Implementation inventory

- `crates/pi-ai/src/model_data_tools.rs`: exported maintenance constants, records, operations, validation.
- `crates/pi-ai/src/bin/check-model-data.rs`: provisional explicit-root checker. Missing/extra argument emits usage and exits 2; final native layout/default remains a user-owned decision.
- `crates/pi-js-compat/src/date.rs`: authorized shared prerequisite fixes for numeric/month/slash/pre-epoch HTTP timestamps; [date proof](../porting/traceability/js-compat.md#dateparse-srcdaters).

## Tests asserting this spec

- `crates/pi-ai/tests/model_data_tools.rs`: temp filesystem fixtures and exact pinned-module differentials.
- `oracle/model-data-oracle.ts`: real pinned API and unmodified checker source evaluated with fixture import-meta URL; stdout, stderr and exitCode captured without filtering.

## Known gaps (current cycle)

- [ ] Raw JSON lone-surrogate strings lose their UTF-16 identity in shared `js_json::parse`; exact diagnostic regression remains RED. Not an invalid-input-type exclusion or approved gap. Shared representation redesign is not authorized.
- [ ] Checker default/layout decision: actual Rust snapshot (`data/providers`, no TS import/shard files) versus source-layout tool (`src/models.generated.ts`, `src/providers/*.models.ts`, `src/providers/data`). Current command only validates the latter with an explicit root.

## Out of scope

- Runtime model loading, generation, image generation, reasoning options, hydration/downloads, network, credentials, telemetry, real catalog writes.
- Standalone `pi-ai` login CLI remains its separate approved gap.
- Invalid JavaScript argument types to typed Rust `ModelDataStructure`, manifest constructor, paths and string allowlists are excluded. Untrusted JSON files remain dynamically validated; malformed metadata is not excluded.
