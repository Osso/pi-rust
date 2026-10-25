# compat traceability

Source: `packages/ai/src/compat.ts` at `4f9e6f8a4` (Result/dispatch slice; remaining rows still require common-pin audit).
Rust: `crates/pi-ai/src/compat/mod.rs` (the module also owns
`compat/extension-oauth-types.ts` in `compat/extension_oauth_types.rs`).
Tests: `crates/pi-ai/tests/compat.rs` (10 behavioral),
`crates/pi-ai/tests/differential_compat.rs` (5 differential tests against
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
| `wrapStream`/`wrapStreamSimple` mismatch throw | `ApiProvider::new` closes over api and returns CallbackError before stream creation with the frozen message | prerequisite implemented; mismatch test migration/current GREEN pending |
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
| old-only Cloudflare special dispatch | removed; frozen dispatches every known builtin directly through compatModels | frozen compat.ts:242–247 / :266–269 |
| `shouldUseBuiltinModels` (exact provider/id, matching API and builtin registered instance) | `get_builtin_provider_for_model` (`Arc::ptr_eq`, exact id/API match) | NSS custom Vertex id takes registry path; default-builtin probe exposed the former extra withEnvApiKey call |
| `stream`/`complete`/`streamSimple`/`completeSimple` | same names | behavioral + differential `dispatch` |
| no registered api | `resolve_api_provider` returns NativeNodeError; stream acquisition returns Err before yielding a stream | `unknown_api_throws_the_registration_error_before_returning_a_stream`, differential `missing` |
| Vertex homedir synchronous throw through withEnvApiKey | stream/stream_simple return Result; complete/complete_simple retain native rejection in CallbackError | `differential_compat::nss`: source/native custom Vertex id forced getpwuid_r EIO, exact native class/message; no event conversion |

## Deviations

- Unregistered API, Vertex environment lookup, API mismatch and registered callback failures use a shared CallbackError acquisition result. Parent approved the prerequisite2026-10-24; consumer migration and full current proof remain pending. No error-stream substitution where frozen propagates.
- `ProviderStreamOptions`/`ApiStreamOptions` are untyped in TypeScript and
  arrive in `StreamOptions::extra`; the compatibility dispatch only reads the
  shared `StreamOptions` fields, so no option mapping is needed here.
- `registerFauxProvider`'s `sourceId` is random in both implementations; only
  its shape (`faux-provider-<8 base36 chars>`) is observable and untested.

## Discrepancies

- None.

## Registered callback prerequisite (2026-10-24)

[Acquisition contract](../../specs/provider-stream-errors.md). Actual frozen registry/public compat dispatch proves4synchronous TypeError/raw UTF-16 callback cases (`model_registry_stream_throw_probe`); native compiler RED exposes the missing callback and dispatch carriers. ProviderStreams raw/simple methods now carry Result, retaining ApiProvider as a structurally usable provider module; optional deferred methods and provider behavior stay unchanged. Nine success-only module adapters wrap existing streams in Ok. Test-provider implementations and successful fixture calls now use the same Result contract without removing request/event assertions; mismatch fixtures assert the frozen synchronous rejection (name and message), not an invented event. All-target compile migration remains in progress. Codex direct Simple errors propagate to the source-cited lazy catch. Lazy and agent-loop consumers must retain their exact frozen catch boundaries; proof and mechanical migrations are not yet complete.

## Q18 provider request integration (2026-10-22)

Compatibility stream wrappers forward the existing native API-key authority directly; no checked or lossy adapter is introduced.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.

2026-10-25: removed old-only ApiProvider fetch_deferred/cancel_deferred methods and carrier imports; common-pin ProviderStreams has only stream/streamSimple. Other compat behavior unchanged.
