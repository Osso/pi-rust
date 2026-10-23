# model-catalog-cache.ts

Frozen source: `packages/coding-agent/src/core/model-catalog-cache.ts`, common pin `4f9e6f8a4`. Contract: [spec](../../specs/model-catalog-cache.md).

| Export / branch | Native mapping | Current proof |
| --- | --- | --- |
| `loadOpenRouterCatalogAtStartup` | `ModelCatalogCache::load_at_startup` | Actual module differential: fresh, exact seven-day expiry, future, stale; invalid dates, incomplete model, non-array models |
| `getOpenRouterCatalogModels` | `ModelCatalogCache::get_models` | Actual merged memoized models compared after startup |
| `ensureModelCatalogFresh` network | `ModelCatalogCache::ensure_fresh` | Full catalog object comparison plus exact persisted fetched bytes, tools filter, image/reasoning flags, parseFloat costs, fractional limits, repeated IDs and bundled precedence |
| Cached model minimal validation | `is_cached_model`, `has_valid_cached_identity` | Unknown nested properties and unpaired human-name units retained through actual frozen/native cache reads |
| Terminal request/schema errors | `fetch_catalog`, refresh wrapper | HTTP400, HTTP429/500 whose Retry-After exceeds deadline; missing data, empty data, no tool models and malformed JSON. NativeNodeError reused for nested request/refresh causes; raw human surrogate + scalar Fetch cause matches frozen nested Error snapshots |
| `fetchOpenRouterCatalog` | `fetch_openrouter_catalog` | Actual direct fetch preserves number type for overflow cost Infinity and negative zero until serialization; caller-absent signal shape GREEN. Abort reason proof awaits shared carrier |
| Retry loop / deadline | `fetch_catalog_with_retries` | Actual HTTP503→500→200, HTTP429→200, exhausted503×3 and permanent400 request counts/results/causes GREEN. Own retryable/retryAfterMs properties preserved. Jitter/deadline/transport proof pending |
| Atomic write failure / cleanup cause | `write_cached_catalog` | Exact successful bytes; actual OS rename-to-directory and mkdir-file/cleanup-ENOTDIR failures match complete native error metadata and messages; failed publication leaves memoization unchanged and temporary file removed |
| Production module memoization and cache root | Shared `ModelCatalogCache` instance to be wired | Not integrated; old runtime remains until combined registry replacement |

RED evidence: `/tmp/pi-rust-repin-delta-ai/catalog-cache-red-clean.log` (new source module absent). Targeted development GREEN: `catalog-cache-green-3.log`, four differential tests. No recursive normalization or live HTTP. Cached models use `JsValue` because the frozen validator returns original objects, not reconstructed typed models; narrowing before the registry boundary would lose valid unknown properties/native text.

Retry/error-own-property correction: `catalog-cache-retry-green.log` is RED despite filename (oracle began exposing actual enumerable error properties); `catalog-cache-retry-green-2.log` is six-test GREEN after restoring frozen CatalogRequestError retryable/retryAfterMs properties. No error-field stripping.

Cause correction RED: `catalog-cache-causes-red-2.log` reproduced a duplicated description, a flattened causal object and U+FFFD through Display. GREEN: `catalog-cache-causes-green-1.log`, five tests. Public errors reuse `pi_agent_core::harness::types::NativeNodeError`; only its property-builder visibility changed (parent clearance). Fetch still exposes `CallbackError.cause: Option<String>`: arbitrary native Error/object causes cannot originate at that boundary, so their proof remains partial. No cause objects or stacks are fabricated; runtime stack omission is user-approved (2026-10-22).

Atomic failure correction RED: `catalog-cache-rename-red.log` reproduced missing errno/code/syscall/path/dest in rename cause. `catalog-cache-atomic-green-1.log`: two OS-boundary tests GREEN after reusing Node filesystem metadata. Normalization touches only the genuinely generated `.openrouter-<pid>-<uuid>.tmp` filename at root-message, cause-message and cause-path locations; no recursive error rewriting or user-data masking.

Native-number correction RED: `catalog-cache-number-red.log` observed serde `json!` prematurely replacing Infinity with null in memory. `catalog-cache-number-green.log` proves direct mapped number identity and JSON output after native-number construction. DISCREPANCY (preserved): frozen parseCost checks finiteness before multiplying by one million; finite1e308 therefore becomes Infinity, writing null cost via JSON.stringify. Frozen startup rejects that resulting cache on its next read. No extra finiteness guard added.

Cache writer provenance: frozen lines108–142 constructs exactly ten known fields with no remote spread/optional-field copies; lines253–254 write fetched models and line206 stringifies them. Wrongly typed optional known fields are foreign/edited/other-version file values, not produced by this frozen writer. Registry handling of such persisted values awaits classification; typed-API exclusions do not automatically apply to files.

This is a partial Q15 module checkpoint, not registry/whole-module parity or integration acceptance. No source-absent architecture has been deleted at this checkpoint.
