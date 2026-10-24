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

Deleted old-only tests in tests/models.rs (not current parity evidence):

| Old-only concern | Retired test names |
|---|---|
| Refresh context/result/options | refresh_updates_dynamic_providers_and_reports_failures; refresh_reports_errors_in_completion_order; restricts_refresh_work_to_selected_providers |
| Store restoration / auth-first catalog phases | restores_cached_models_before_waiting_for_network_auth; persists_dynamic_catalogs_and_restores_them_without_network; passes_effective_api_key_and_refresh_options_and_skips_unconfigured_providers; refreshes_expired_oauth_before_refreshing_models |
| Refresh abort/supersession | returns_aborted_state_without_reporting_cancellation_as_a_provider_error; stops_waiting_on_abort_when_a_provider_ignores_its_signal; rejects_late_publication_from_a_superseded_provider; newer_dynamic_refresh_supersedes_older_network_work |
| Status/login/logout | checks_auth_without_refreshing_oauth_and_filters_available_models; runs_provider_login_and_logout_through_the_credential_store |
| Non-source wait cancellation / deferred | stops_waiting_for_non_cooperative_auth_callbacks; applies_resolved_options_to_deferred_fetch_and_cancellation |

Also deleted github_copilot_oauth::stored_oauth_credential_limits_the_provider_catalog: it exclusively asserted removed get_available/filterModels catalog restriction, contrary to frozen's full catalog. provider_auth's stored-credential/full-catalog assertion remains, migrated to actual Models.getModels.

Deleted the source-absent models_store.rs module and exclusive test file: in_memory_models_store_round_trips_clones_and_deletes, in_memory_models_store_honors_cancellation, models_store_entry_serializes_camel_case_timestamps; removed public-index compile assertions for those retired exports. In mixed tests, retired only min_oauth_validity_ms override and old merge_headers helper assertions; model equality remains. The tier test preserves base-rate assertions and now asserts frozen ignores tier metadata. Actual key/env override assertions use the shared request-auth entrypoint, not a fabricated Models status overload.

Preserved19 current test bodies: provider registration/model lookup, actual-model auth and stored ownership, error codes/exact cause values, request key/env/header merging, model-header separation, lazy streams and unknown/missing API errors, cost/thinking/equality and StreamFailure. Native21 focused cases GREEN in q15b-models-current-native-green.log (includes replacement differential and five-case cause differential). This is not whole-module acceptance: short-lived OAuth behavior in one retained test still reflects the queued auth-resolve re-port.

Approved2026-10-22 owned-value APIs omit JS mutation-through-returned-reference; native cost consumers assign the returned cost explicitly. Opaque provider IDs/names stay scalar under the existing approval; Model.name stays JsString. No getModels error is swallowed outside the exact frozen catch site.

Initial behavioral proof: `q15b-models-contract-red.log` native [old,new] vs frozen [new]; `q15b-models-contract-green-1.log` passes after source rewrite. This is not whole-module acceptance.
