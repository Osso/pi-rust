# models.ts common-pin rewrite

Frozen `packages/ai/src/models.ts:1–451` at4f9e6f8a4; native `pi-ai/src/models.rs`. **Implementation WIP: consumer/test migration and full module proof remain open.**

| Frozen surface | Native | Proof/status |
|---|---|---|
| Provider metadata/auth/streams | Provider trait; fallible get_models, optional refresh callback | consumer migration pending |
| Models set/delete/clear, provider/model reads (:156–198) | insertion-ordered provider map; exact getModels-only best-effort catch at:175–194 | old valid registration/list assertions to migrate; new catch cases pending |
| refresh(provider), allSettled all-provider refresh (:200–214) | Result<(),ModelsError>; explicit provider wraps/preserves native causes, all-provider best-effort | current refresh replacement RED→GREEN in models_contract; concurrency/error matrix pending |
| getAuth(actual model), request auth and literal object spreads (:216–256) | actual-model resolver, exact case-sensitive header/env merge; unconfigured auth leaves options unchanged | current auth/stream regression migration pending; underlying auth/resolve whole-module re-port follows |
| stream/complete/simple variants (:259–291) | lazy dispatch, native ModelsError carriers | pending current differential |
| createProvider (:299–387) | replace whole list on successful refresh, shared inflight, retry after failure, per-api dispatch | models_contract real source proves [old]→[new], former baseline-overlay behavior RED |
| cost/thinking/equality (:400–451) | base rates only, pure owned cost return, exact level filters/clamp and id/provider equality | old tier test is source-absent; current matrix pending |

## Removed source-absent architecture

Publication/persistence/generations/supersession, RefreshModelsContext/Options/Result, check_auth/get_available/login/logout/provider-ID get_auth, deferred collection/provider hooks, filter_models and dynamic-overlay merging are not in frozen models.ts. They are deleted from the implementation, not retained as compatibility paths. ModelsStore's standalone module/tests/exports are retired with consumer migration; no frozen ai models-store module exists.

## Old-only header-feature removal

Frozen packages have zero before_provider_headers/emitBeforeProviderHeaders/transformHeaders occurrences (main verified). Deleted HeaderTransform/request field, simple-options copy, SDK subscription and extension event/type/registration/dispatch/export. No current header/auth spread behavior is removed.

Retired only exclusive hook portions: models::merges_auth_options_and_keeps_model_headers_for_the_api_adapter (transform counter/callback/x-transformed/field-stripping assertions), models::applies_resolved_options_to_deferred_fetch_and_cancellation (transform callbacks/key assertions; entire deferred test separately retired below), differential_ai_runtime::compare_apply_auth (hasTransform observation and callback), sdk::create_agent_session_wires_extension_provider_hooks (header handler/assertion). All ordinary header/env merge, original model-header, context and response assertions remain.

## Test retirement inventory

Old-only tests scheduled for deletion (not current parity evidence): refresh completion-order error map; refresh allowNetwork/force/provider-list/context; restore/persist cached models; auth pre-refresh during catalog fetch; signal cancellation/late publication/supersession; status-only check/getAvailable; collection login/logout; deferred fetch/cancel; request transformHeaders; tiered request-wide cost calculation. Preserve and migrate every assertion about current frozen provider registration, model lookup, actual-model auth, auth option merging, lazy stream dispatch, cost/thinking/equality and stream failures. Exact removed names and final coverage recorded when migration commits.

Approved2026-10-22 owned-value APIs omit JS mutation-through-returned-reference; native cost consumers assign the returned cost explicitly. Opaque provider IDs/names stay scalar under the existing approval; Model.name stays JsString. No getModels error is swallowed outside the exact frozen catch site.

Initial behavioral proof: `q15b-models-contract-red.log` native [old,new] vs frozen [new]; `q15b-models-contract-green-1.log` passes after source rewrite. This is not whole-module acceptance.
