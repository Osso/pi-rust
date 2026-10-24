# Models collection

Match frozen `packages/ai/src/models.ts` at4f9e6f8a4. [Traceability](../porting/traceability/models.md).

- Provider IDs are unique; upsert preserves insertion position. Model reads are synchronous last-known lists. A throwing provider getModels yields no models at that exact catch boundary.
- refresh(provider) is Result<(),ModelsError>: missing/static provider is a no-op; failure preserves an existing ModelsError or wraps the exact thrown cause as model_source. All-provider refresh starts providers concurrently and settles every failure without rejecting.
- createProvider's successful refresh replaces its entire model list; concurrent refresh calls share one inflight fetch, failure retains the last list, and a later call retries.
- getAuth accepts the actual model. Lazy stream/complete variants resolve auth and delegate to its provider. Unknown providers fail with ModelsError; unconfigured auth leaves request/model unchanged. Explicit apiKey wins; auth/options header/env object spreads are case-sensitive; truthy auth.baseUrl replaces the endpoint.
- Provider dispatch accepts one implementation or an API-keyed table; missing API reports ModelsError stream. Cost uses base rates (including long-cache formula), thinking level support/clamping and equality follow frozen logic.

No models store/publication, catalog supersession/cancellation/options phases, auth status/login/logout, deferred operations, transformHeaders or filter-model hooks. Optional provider refresh is Option<callback>, not a default method. UTF-16 human text and native error causes remain lossless. Existing owned-value and opaque-ID/name approvals apply.
