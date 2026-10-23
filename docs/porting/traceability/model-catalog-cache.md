# model-catalog-cache.ts

Frozen source: `packages/coding-agent/src/core/model-catalog-cache.ts`, common pin `4f9e6f8a4`. Contract: [spec](../../specs/model-catalog-cache.md).

| Export / branch | Native mapping | Current proof |
| --- | --- | --- |
| `loadOpenRouterCatalogAtStartup` | `ModelCatalogCache::load_at_startup` | Actual module differential: fresh, exact seven-day expiry, future, stale; invalid dates, incomplete model, non-array models |
| `getOpenRouterCatalogModels` | `ModelCatalogCache::get_models` | Actual merged memoized models compared after startup |
| `ensureModelCatalogFresh` network | `ModelCatalogCache::ensure_fresh` | Full catalog object comparison plus exact persisted fetched bytes, tools filter, image/reasoning flags, parseFloat costs, fractional limits, repeated IDs and bundled precedence |
| Cached model minimal validation | `is_cached_model`, `has_valid_cached_identity` | Unknown nested properties and unpaired human-name units retained through actual frozen/native cache reads |
| Terminal request/schema errors | `fetch_catalog`, refresh wrapper | HTTP400, HTTP429/500 whose Retry-After exceeds deadline; missing data, empty data, no tool models and malformed JSON. NativeNodeError reused for nested request/refresh causes; raw human surrogate + scalar Fetch cause matches frozen nested Error snapshots |
| `fetchOpenRouterCatalog` | `fetch_openrouter_catalog` | Implemented; direct-helper/abort proof pending |
| Retry loop / deadline | `fetch_catalog_with_retries` | Implemented; eligible retry count/jitter/deadline/transport proof pending |
| Atomic write failure / cleanup cause | `write_cached_catalog` | Successful exact bytes proven; failure/cleanup cause matrix pending |
| Production module memoization and cache root | Shared `ModelCatalogCache` instance to be wired | Not integrated; old runtime remains until combined registry replacement |

RED evidence: `/tmp/pi-rust-repin-delta-ai/catalog-cache-red-clean.log` (new source module absent). Targeted development GREEN: `catalog-cache-green-3.log`, four differential tests. No recursive normalization or live HTTP. Cached models use `JsValue` because the frozen validator returns original objects, not reconstructed typed models; narrowing before the registry boundary would lose valid unknown properties/native text.

Cause correction RED: `catalog-cache-causes-red-2.log` reproduced a duplicated description, a flattened causal object and U+FFFD through Display. GREEN: `catalog-cache-causes-green-1.log`, five tests. Public errors reuse `pi_agent_core::harness::types::NativeNodeError`; only its property-builder visibility changed (parent clearance). Fetch still exposes `CallbackError.cause: Option<String>`: arbitrary native Error/object causes cannot originate at that boundary, so their proof remains partial. No cause objects or stacks are fabricated; JS runtime stack text remains unproven.

This is a partial Q15 module checkpoint, not registry/whole-module parity or integration acceptance. No source-absent architecture has been deleted at this checkpoint.
