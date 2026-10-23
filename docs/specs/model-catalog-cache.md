# OpenRouter model catalog cache

Frozen `packages/coding-agent/src/core/model-catalog-cache.ts` supplies startup catalogs and explicit refreshes. [Traceability](../porting/traceability/model-catalog-cache.md) records current proof boundaries.

## What it must do

- [x] Startup returns bundled models unless a valid cache is younger than seven days; future timestamps are fresh. Stale valid caches retain their cached count.
- [x] Cached identity, costs, modalities and positive finite limits are validated without dropping unknown properties or replacing native UTF-16.
- [x] Bundled models win ID collisions; new duplicate IDs keep the first fetched model.
- [x] Fetch includes only tool-capable models, maps reasoning/image support, parses per-million pricing, and preserves fractional limits.
- [x] Successful refresh atomically writes the unmerged fetched catalog with the completion-time timestamp and memoizes the merged catalog.
- [x] Terminal HTTP, missing/empty data and malformed JSON produce frozen error messages.
- [ ] Retry eligible transport failures, HTTP429/5xx at most twice with jitter/Retry-After under one five-second deadline.
- [ ] Preserve filesystem primary/cleanup errors and causal identity.

## How it works

- [Current module mapping and proof](../porting/traceability/model-catalog-cache.md).

## Implementation inventory

- `crates/pi-coding-agent/src/core/model_catalog_cache.rs`: native cache module; explicit path instance preserves the frozen module's memoized state.
- `oracle/model-catalog-cache-oracle.ts`: actual frozen-module HTTP seam; no field normalization.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_model_catalog_cache.rs`: startup boundary, validation, native objects, mapping, persisted bytes and terminal errors.

## Known gaps (current cycle)

- [ ] Registry/startup/CLI consumers still use old runtime architecture; Q15 replacement pending.
- [ ] Retry/deadline/transport/atomic-failure matrices and direct public fetch helper proof pending.
- [ ] Production cache-root wiring pending registry integration.

## Out of scope

Live provider requests are excluded from lane verification.
