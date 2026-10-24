# models.ts common-pin rewrite

Frozen `packages/ai/src/models.ts:1–451` at4f9e6f8a4; native `pi-ai/src/models.rs`. Collection implementation and consumer migration are complete; source differential matrix below covers the module's own branches. Underlying [auth-resolve](auth-resolve.md) now has direct locked-state/error/env/dispatch proof within its explicit typed exclusions; shared auth/types remains a separate partial module.

| Frozen surface | Native | Proof/status |
|---|---|---|
| Provider metadata/auth/streams | Provider trait; fallible get_models, optional refresh callback | current provider/catalog differentials plus stream capture matrix; workspace all-target0/0 |
| Models set/delete/clear, provider/model reads (:150–195) | insertion-ordered provider map; getModels-only catches (:176–180,185–189) | actual-source registration/upsert/deletion/clear/lookup and failing-provider isolation matrix |
| refresh(provider), allSettled all-provider refresh (:198–214) | Result<(),ModelsError>; explicit provider wraps/preserves native causes, all-provider best-effort | four raw cause payloads; existing ModelsError preserved; an outer Error caused by ModelsError remains an outer error and is wrapped with its full nested cause; missing/static no-op; both provider hooks start before either settles, failure swallowed only all-provider path |
| getAuth(actual model), request auth and literal object spreads (:216–256) | actual-model resolver, case-sensitive header/env merge; unconfigured auth leaves options unchanged | six actual-source cases: unconfigured, literal merges/raw UTF-16, empty key/base/maps, missing provider/API, raw false rejection; model headers/metadata isolation and serialized model key order exact |
| stream/complete/simple variants (:259–291) | lazy dispatch, native ModelsError carriers | all four variants for each of six source cases (24 full message results), exact provider capture/auth result; only runtime timestamps normalized after native clock-bound assertions |
| createProvider (:299–373) | replace whole list on successful refresh, shared inflight, retry after failure, per-api dispatch | actual-source inflight call count/last-known model snapshots/success/failure/retry; replacement RED→GREEN; stream matrix + retained mixed-API native test |
| cost/thinking/equality (:385–451) | base rates only, owned cost return, exact level filters/clamp and id/provider equality | differential_ai_runtime: five cost cases (missing/zero/positive long-cache writes, fractional/negative short component, ignored tier metadata), six thinking maps × eight requested levels, seven equality pairs (nulls, API-irrelevance, provider/ID/canonical-Unicode differences) |

## Removed source-absent architecture

Publication/persistence/generations/supersession, RefreshModelsContext/Options/Result, check_auth/get_available/login/logout/provider-ID get_auth, deferred collection/provider hooks, filter_models and dynamic-overlay merging are not in frozen models.ts. They are deleted from the implementation, not retained as compatibility paths. ModelsStore's standalone module/tests/exports are retired with consumer migration; no frozen ai models-store module exists.

## Old-only header-feature removal

Frozen packages have zero before_provider_headers/emitBeforeProviderHeaders/transformHeaders occurrences (main verified). Deleted HeaderTransform/request field, simple-options copy, SDK subscription and extension event/type/registration/dispatch/export. No current header/auth spread behavior is removed.

Retired only exclusive hook portions: models::merges_auth_options_and_keeps_model_headers_for_the_api_adapter (transform counter/callback/x-transformed/field-stripping assertions), models::applies_resolved_options_to_deferred_fetch_and_cancellation (transform callbacks/key assertions; entire deferred test separately retired below), differential_ai_runtime::compare_apply_auth (hasTransform observation and callback), sdk::create_agent_session_wires_extension_provider_hooks (header handler/assertion). All ordinary header/env merge, original model-header, context and response assertions remain.

## Test retirement inventory

Deleted old-only tests in tests/models.rs (not current parity evidence):

| Old-only concern | Retired test names |
|---|---|
| Refresh context/result/options | refresh_updates_dynamic_providers_and_reports_failures; refresh_reports_errors_in_completion_order; restricts_refresh_work_to_selected_providers |
| Store restoration / auth-first catalog phases | restores_cached_models_before_waiting_for_network_auth; persists_dynamic_catalogs_and_restores_them_without_network; passes_effective_api_key_and_refresh_options_and_skips_unconfigured_providers; refreshes_expired_oauth_before_refreshing_models |
| Refresh abort/supersession | returns_aborted_state_without_reporting_cancellation_as_a_provider_error; stops_waiting_on_abort_when_a_provider_ignores_its_signal; rejects_late_publication_from_a_superseded_provider; newer_dynamic_refresh_supersedes_older_network_work |
| Status/login/logout | checks_auth_without_refreshing_oauth_and_filters_available_models; runs_provider_login_and_logout_through_the_credential_store |
| Non-source wait cancellation / deferred | stops_waiting_for_non_cooperative_auth_callbacks; applies_resolved_options_to_deferred_fetch_and_cancellation |

Replaced github_copilot_oauth::stored_oauth_credential_limits_the_provider_catalog: modern get_available/filterModels is removed, but legacy OAuthProviderInterface.modifyModels still filters credentials. Restored that requirement through legacy_modify_models_filters_present_ids_and_preserves_absent_id_catalog against actual frozen utils/oauth/github-copilot.ts:455–472, with omitted/present/empty IDs and full record equality. provider_auth's modern stored-credential/full-catalog assertion remains through actual Models.getModels. [Legacy filter proof](auth-oauth-github-copilot.md#common-pin-legacy-modifymodels-filter).

Deleted the source-absent models_store.rs module and exclusive test file: in_memory_models_store_round_trips_clones_and_deletes, in_memory_models_store_honors_cancellation, models_store_entry_serializes_camel_case_timestamps; removed public-index compile assertions for those retired exports. In mixed tests, retired only min_oauth_validity_ms override and old merge_headers helper assertions; model equality remains. The tier test preserves base-rate assertions and now asserts frozen ignores tier metadata. Actual key/env override assertions use the shared request-auth entrypoint, not a fabricated Models status overload.

Preserved19 current test bodies: provider registration/model lookup, actual-model auth and stored ownership, error codes/exact cause values, request key/env/header merging, model-header separation, lazy streams and unknown/missing API errors, cost/thinking/equality and StreamFailure. Native21 focused cases GREEN in q15b-models-current-native-green.log (includes replacement differential and five-case cause differential). This is not whole-module acceptance: short-lived OAuth behavior in one retained test still reflects the queued auth-resolve re-port.

Approved2026-10-22 owned-value APIs omit JS mutation-through-returned-reference; native cost consumers assign the returned cost explicitly. Opaque provider IDs/names stay scalar under the existing approval; Model.name stays JsString. No getModels error is swallowed outside the exact frozen catch site.

Initial RED: q15b-models-contract-red.log native [old,new] vs frozen [new]. Source matrix GREEN: q15b-models-contract-matrix-1.log (five catalog/refresh tests, including four raw-cause variants), q15b-models-stream-matrix-1.log (six cases × four stream variants). Runtime helpers/wire headers/public exports GREEN4 in q15b-models-runtime-current-1.log; expanded pure helper matrix GREEN q15b-models-pure-matrix.log. Six-test Models source matrix GREEN q15b-models-contract-final.log. Additional nested-error RED q15b-models-nested-error-red.log exposed an incorrect native cause-chain class search; frozen:205 checks the thrown error itself, so only direct typed ModelsError provenance is preserved; latest seven-case contract target plus pure/runtime differential GREEN8 in q15b-models-final-green.log after the nested-error correction; crate all-target check and targeted clippy -D warnings GREEN q15b-models-final-{check,clippy}.log. Earlier matrix logs remain bounded historical evidence. These do not certify the queued auth-resolve expiry/timeout or package-manager modules.

## Approved gaps / exclusions

- ProviderApi::Map uses own-key lookup. Frozen createProvider:329–340 ordinary-object inherited API properties may produce native-function/TypeError oddities; excluded under the2026-10-23 policy, not emulated.
- Independent async producer/microtask scheduling is native; shared inflight/retention/retry and all-provider concurrency are proven behaviorally, without first-poll/yield emulation.
- Typed Rust enums exclude invalid JS runtime level/API argument types. JS reference/mutation-through-returned-reference is excluded under the owned-value approval; getProvider retains Arc identity while model lists are owned.
