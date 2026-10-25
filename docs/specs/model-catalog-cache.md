# OpenRouter model catalog cache

Frozen `packages/coding-agent/src/core/model-catalog-cache.ts` supplies startup catalogs and explicit refreshes. [Traceability](../porting/traceability/model-catalog-cache.md) records current proof boundaries.

## What it must do

- [x] Startup returns bundled models unless a valid cache is younger than seven days; future timestamps are fresh. Stale valid caches retain their cached count.
- [x] Cached identity, costs, modalities and positive finite limits are validated without dropping unknown properties or replacing native UTF-16.
- [x] Bundled models win ID collisions; new duplicate IDs keep the first fetched model.
- [x] Fetch includes only tool-capable models, maps reasoning/image support, parses per-million pricing, and preserves fractional limits. Overflow pricing remains Infinity in memory, negative zero remains signed, and JSON serialization alone projects nonfinite numbers to null.
- [x] Successful refresh atomically writes the unmerged fetched catalog with the completion-time timestamp and memoizes the merged catalog.
- Freshness and completion use separate clock evaluations even without a cache. Invalid completion dates reject with RangeError before temporary-file creation or publication.
- [x] Terminal HTTP, missing/empty data and malformed JSON produce frozen error messages.
- [x] Request/refresh wrapping preserves branded nested Error causes, plain values, native human text and original error properties. Plain objects with message fields are not Errors; String conversion failures propagate their actual TypeError.
- [x] Public unretried fetch preserves raw response-body rejections; its error is the shared thrown-value carrier, not a synthesized Error.
- [x] Retry HTTP429/5xx at most twice; stop on success or permanent HTTP400. Preserve enumerable retry metadata in returned nested errors.
- [x] Retry eligible transport failures with constructor-based TypeError checks, exponential jitter and Retry-After under one five-second abort timer. Headers.get trims HTTP whitespace and joins repeated values; date parsing and retry admission use the same wall clock. Retry waits follow Node timer truncation and reject on abort with the original reason.
- The timer aborts rather than racing the operation. An injected fetch ignoring its signal may still succeed after five seconds; timeout errors retain the actual request/delay cause. Clear the timer on every settled path.
- [x] Preserve rename/mkdir primary errors and cleanup failure context, including actual Node filesystem cause metadata. Failed writes do not publish models.

## Startup and CLI consumers

- Normal startup loads the disk/bundled catalog before constructing registries. Architect and Supervisor service startup load it before entering their services.
- Registries use the same memoized OpenRouter authority and re-read it on refresh; existing registry snapshots do not update implicitly.
- `--refresh-models` forces refresh even with a fresh cache, prints fetched/cached/bundled/merged counts and cache path, then exits. Offline mode rejects the attempted network refresh instead of serving a cached substitute.
- The flag is recognized as a boolean, has its frozen help entry and is plain metadata only without print/mode overrides. Other Args/help rows retain separate proof scope.

## How it works

- [Current module mapping and proof](../porting/traceability/model-catalog-cache.md).

## Implementation inventory

- `crates/pi-coding-agent-core/src/core/model_catalog_cache.rs`: one process-wide module state, shared by public startup/refresh/getter functions and ModelRegistry. Cache path is recomputed on each startup/ensure call; isolated state objects used by tests take a path per operation, not a stored path.
- `oracle/model-catalog-cache-oracle.ts`: actual frozen-module HTTP seam; no field normalization.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_model_catalog_cache.rs`: startup boundary, validation, native objects, mapping, persisted bytes and terminal errors.

## Known gaps (current cycle)

- [x] Normal startup/registry/list-models process proof, forced offline failure, forced positive summary, shared getter/registry lifetime, refresh-only snapshot updates and dynamic cache-root proof. Architect/Supervisor startup calls are wired/type-checked; their services are not launched by this lane.
- [x] Retry/jitter/Retry-After/deadline/transport/atomic-failure and direct public-helper matrices pass at4285f35f: real5s abort/5100ms late success, supplied/falsey signal reasons and10 deterministic timer-boundary cases. Broader lane acceptance remains separate.
- [x] Per-call production cache-root and same-process module/registry lifetime proof, including raw names, unknown properties and exact model JSON key order.
- [x] Constructor/cause/body-rejection matrices pass at976af57e; callbacks use the shared thrown-value carrier, including branded nested causes. Runtime JS stack omission is user-approved (2026-10-22).

## Out of scope

Live provider requests are excluded from lane verification.
