# compat traceability

Source: `packages/ai/src/compat.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/compat/mod.rs` (the module also owns
`compat/extension-oauth-types.ts` in `compat/extension_oauth_types.rs`).
Tests: `crates/pi-ai/tests/compat.rs` (9 behavioral),
`crates/pi-ai/tests/differential_compat.rs` (4 differential requests against
the pinned TS module).

## Consumer contract

- `pi_ai::compat::get_api_provider(api: &str) -> Option<Arc<ApiProvider>>`.
  `ApiProvider` implements `ProviderStreams`, so callers dispatch with
  `stream`/`stream_simple`/`fetch_deferred`/`cancel_deferred`. Registration
  order is preserved (JavaScript `Map` insertion order; re-registering an api
  keeps its position).

## Symbols → proof

| TypeScript | Rust | Proof |
|---|---|---|
| `getModel`/`getModels`/`getProviders` | `get_model`/`get_models`/`get_providers` (re-exports of `providers/all`) | registry differential; `providers-all` differential |
| `ApiStreamFunction`, `ApiStreamSimpleFunction` | `ApiStreamFunction`, `ApiStreamSimpleFunction` | used by `ApiProvider::new` |
| `ApiProvider` | `ApiProvider` (`new`, `from_streams`, `api`) | `registered_provider_receives_the_explicit_api_key` |
| `wrapStream`/`wrapStreamSimple` mismatch throw | `ApiProvider::new` closes over the api and returns an error stream with `Mismatched api: <model.api> expected <api>` | `api_mismatch_is_reported_as_the_typescript_error_message`; differential `mismatch` |
| `registerApiProvider(provider, sourceId?)` | `register_api_provider(ApiProvider, Option<String>)` | differential `registry` |
| `getApiProvider` | `get_api_provider` | behavioral + differential `registry` |
| `getApiProviders` | `get_api_providers` | differential `registry` (order preserved) |
| `unregisterApiProviders(sourceId)` | `unregister_api_providers` | `unregister_removes_only_the_named_source`, differential `registry` |
| `registerFauxProvider(options?)` | `register_faux_provider(RegisterFauxProviderOptions) -> FauxProviderRegistration` | `faux_registration_completes_and_estimates_usage`; differential `faux` |
| registration `unregister` | `FauxProviderRegistration::unregister` | differential `faux` (`afterUnregister`) |
| `BUILTIN_APIS` (10 entries, order) | `builtin_apis()` | `built_in_apis_are_registered_in_compat_order`, differential `registry.initial` |
| `registerBuiltInApiProviders` | `register_built_in_api_providers` | differential `registry.afterReset` |
| `resetApiProviders` | `reset_api_providers` | behavioral + differential `registry` |
| module-load registration | `ensure_initialized()` on first registry access (builtins are always registered before a caller's first `register_api_provider`) | differential `registry.initial` |
| `compatModels = builtinModels()` | `COMPAT_MODELS` (`LazyLock<ModelsCollection>`) | dispatch differential through builtin providers |
| `hasExplicitApiKey` (trim non-empty) | `has_explicit_api_key` | `explicit_key_beats_the_scoped_env_key_and_blank_keys_fall_back` |
| `withEnvApiKey` | `with_env_api_key` / `with_env_api_key_simple` | dispatch differential cases `explicit`, `env`, `explicit-wins`, `blank`, `no-key`, `simple-env`, `simple-explicit` |
| ambient `"<authenticated>"` marker is not injected | same | gap: the marker only comes from Vertex ADC discovery; covered by `env-api-keys` tests rather than a compat case |
| `hasResolvedCloudflareAuth` (`cf-aig-authorization` string) | `has_resolved_cloudflare_auth` | gap: exercising the cloudflare fallback requires real credential resolution/network; branch is a straight translation |
| `getBuiltinProviderForModel` (registered instance must be the builtin instance and the provider must list the api) | `get_builtin_provider_for_model` (`Arc::ptr_eq`) | dispatch differential: an override routes through the registry, the builtin routes through the provider |
| `stream`/`complete`/`streamSimple`/`completeSimple` | same names | behavioral + differential `dispatch` |
| no registered api | `resolve_api_provider` error stream `No API provider registered for api: <api>` | `unknown_api_reports_the_registration_error_in_the_stream`, differential `missing` |
| cloudflare model fallback to `compatModels.stream*` | same | gap as above |

## Deviations

- TypeScript `stream`/`streamSimple` throw synchronously for an unregistered
  api, and the wrapped stream functions throw on an api mismatch. Rust returns
  an `AssistantMessageEventStream` that immediately emits `error` with the same
  message and `stopReason: "error"`; callers observe the same message through
  `result()`. This follows the crate-wide "stream failures are encoded in the
  stream" convention.
- `ProviderStreamOptions`/`ApiStreamOptions` are untyped in TypeScript and
  arrive in `StreamOptions::extra`; the compatibility dispatch only reads the
  shared `StreamOptions` fields, so no option mapping is needed here.
- `registerFauxProvider`'s `sourceId` is random in both implementations; only
  its shape (`faux-provider-<8 base36 chars>`) is observable and untested.

## Discrepancies

- None.
