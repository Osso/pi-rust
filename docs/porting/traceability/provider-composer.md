# provider-composer

Source: `packages/coding-agent/src/core/provider-composer.ts` (573 lines).
Rust: `crates/pi-coding-agent/src/core/provider_composer.rs`.
Tests: `crates/pi-coding-agent/tests/provider_composer.rs` (ports
provider-composition cases from `test/model-registry.test.ts`).

## Exports and branches

| TS export / branch | Rust item | Test |
|---|---|---|
| `ExtensionOAuthConfig`, `ProviderConfigInput` | same names (`Model` list instead of partial definitions) | `extension_*`, `oauth_only_extension_*` |
| `AuthStatus` / `clearApiKeyCache` | `AuthStatus`, `clear_api_key_cache` | `configured_auth_status_*` |
| `mergeCompat` shallow + 4 nested merges | `merge_compat` | `provider_level_compat_*`, `model_level_compat_*` |
| `applyModelOverride` name/reasoning/thinking/cost/window/tokens/sampling/compat | `apply_model_override` | `model_overrides_*` |
| `modelFromJson` api/baseUrl requirements, invalid window/tokens, defaults, compat | `model_from_json` | `custom_models_*`, `provider_level_*` |
| `applyModelsJson` oauth-without-baseUrl, empty-config error, radius baseUrl, upsert | `apply_models_json` | `provider_without_any_configuration_*`, `custom_model_with_same_id_replaces_*`, `base_url_override_*` |
| `applyExtension` baseUrl-only rewrite, model replacement, api/baseUrl errors | `apply_extension` | `extension_models_override_*` |
| `adaptOAuth` callback mapping | `ExtensionOAuth`, `InteractionOAuthCallbacks` | `oauth_only_extension_has_no_api_key_method`; callback mapping not asserted end-to-end |
| `withConfiguredAuth` merge + authHeader error | `with_configured_auth` | covered through extension/oauth tests |
| `configuredApiKey` / `configuredHeaders` / `configContextEnv` | same | `configured_headers_resolve_at_request_time`, `model_level_headers_merge_*` |
| `composeApiKeyAuth` inherited/raw key/command/env check/resolve | `ComposedApiKeyAuth` | `configured_auth_status_*`, `provider_without_configured_key_*` |
| `composeOAuthAuth` extension/base selection, header wrapping | `ComposedOAuthAuth` | `composed_provider_keeps_base_oauth_*`, `oauth_only_extension_*` |
| `rawModelHeaders` override → definition → extension | `raw_model_headers` | `model_level_headers_merge_*` |
| `validateExtensionProvider` streamSimple/api rule + eager validation | `validate_extension_provider` | `extension_stream_simple_requires_api`, `extension_models_override_*` |
| `composeModelProvider` layers, eager validation, auth requirement, name/baseUrl/auth, refresh/filter/deferred delegation, stream dispatch | `compose_model_provider`, `ComposedProvider` | composition tests, `refresh_models_publishes_extension_results` |
| `resolveConfiguredModelHeaders` | `resolve_configured_model_headers` | `configured_headers_*` |
| `resolveCompatibilityRequestConfig` | `resolve_compatibility_request_config` | `compatibility_request_config_includes_static_model_headers` |
| `configuredRequestAuthStatus` | `configured_request_auth_status` | `configured_auth_status_*` |

## Deviations / gaps

- Extension model definitions are full `Model` values (typed Rust API excludes
  the partial JS definition shape). Composition still overrides `provider`,
  `headers`, and uses the model's `api`/`baseUrl`.
- `getApiProvider` fallback uses `pi_ai::compat::get_api_provider`, as approved.
- The stream dispatch itself is not exercised by tests (would require a network
  or faux provider); composition output and refresh are.
- OAuth callback `on_prompt`/`on_manual_code_input` return `String` (not
  `Result`) per the pi-ai compatibility trait; prompt failures surface as empty
  input. This mirrors the trait contract, not the TS throw path.

## Discrepancies

None found.
