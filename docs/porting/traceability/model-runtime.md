# model-runtime

Source: `packages/coding-agent/src/core/model-runtime.ts` (795 lines).
Rust: `crates/pi-coding-agent/src/core/model_runtime.rs`.
Tests: `crates/pi-coding-agent/tests/model_runtime.rs` (ports
`test/model-runtime-{auth-options,cloudflare-compat,credential-sync,modify-models-compat}.test.ts`
and runtime cases from `test/model-registry.test.ts`) and
`tests/differential_model_runtime.rs` (oracle `oracle/model-runtime-oracle.ts`:
provider lists, model lists, composition errors, auth statuses, availability,
resolved auth, and compatibility config from temp `models.json` + auth fixtures).

## Exports and branches

| TS export / branch | Rust item | Test |
|---|---|---|
| `CreateModelRuntimeOptions` | `CreateModelRuntimeOptions` | every runtime test |
| `ModelRuntimeAuthOverrides` | reuses `AuthResolutionOverrides` | env/auth-header tests |
| `CredentialSynchronizationOperation` | enum + `as_str` | `credential_sync_failure_*` |
| `CredentialSynchronizationError` | same struct + exact message | `credential_sync_failure_*` |
| `mergeHeaders` | `pi_ai::models::merge_headers` | `explicit_authorization_header_*` |
| `create`: default credentials/models path/store, builtin + remote catalog, radius providers, network flag, timeout/abort composition, initial refresh | `ModelRuntime::create` | all tests; `refresh_reloads_models_json_from_disk` |
| `configureRadiusProviders` | same | radius OAuth test (provider registration) |
| `providerIds` insertion order | `provider_ids` | `projects_provider_owned_names_and_auth_methods` |
| `recomposeProvider` no-overlay / compose / error fallback | `recompose_provider` | `reports_every_provider_composition_error`, override tests |
| `rebuildProviders`, `updateModelSnapshot` | same | every composition test |
| `runAvailabilityRefresh` (available + checks + credential list, sequence guards) | same | `get_available_filters_by_configured_auth` |
| `queueAvailabilityRefresh` (sequence bump, error recording) | same | `get_available_*` |
| `refreshProviderAvailability` (provider sequence, auth/stored/available update) | same | `runtime_api_keys_mask_*` |
| `getProviders/getProvider/getModels/getModel/checkAuth/getAvailable/getAvailableSnapshot` | same | runtime tests |
| `getError` (config + composition + availability, `\n\n`) | same | `reports_every_provider_composition_error` |
| registered provider config/ids/native accessors | same | `native_provider_registration_*`, `unregister_provider_*` |
| `getCompatibilityRequestConfig` | same | `compatibility_request_config_*` |
| `isUsingOAuth`, `isUsingSubscription`, `hasConfiguredAuth` | same | `distinguishes_subscription_oauth_*` |
| `getAuth` provider/model + configured header merge | `get_auth`, `get_auth_for_model` | `get_auth_merges_configured_headers_*`, env test |
| credential operation queue (per-provider, abort before start) | `enqueue_credential_operation` | `credential_sync_failure_*`, runtime-key tests |
| `synchronizeCredentialState` | same | runtime-key/login/logout |
| `setRuntimeApiKey` / `removeRuntimeApiKey` / `listCredentials` | same | `runtime_api_keys_*`, `list_credentials_*` |
| `getProviderAuthStatus` (runtime/stored/configured/environment) | same | `provider_auth_status_reports_models_json_sources` |
| `prepareRequest` (auth, header transform, env, baseUrl) | same | `explicit_authorization_header_*`, `transforms_*` |
| `stream/complete/streamSimple/completeSimple` | same | header capture tests |
| `fetchDeferred/cancelDeferred` | same | compile-level; deferred support is provider-dependent |
| `login/logout` | same | not exercised end-to-end (needs interactive auth) |
| `reloadConfig` | same (auth reload via the configured path) | `refresh_reloads_models_json_from_disk` uses `refresh` |
| `refresh` (providers/all, availability, error map, aborted) | same | refresh/override tests |
| `registerNativeProvider/registerProvider/unregisterProvider` | same | extension/provider tests |
| `ModelResolverRuntime` impl | same | model_resolver tests |

## Deviations / gaps

- Interactive `login`/`logout` are implemented but not behaviorally tested
  (would require an `AuthInteraction` fixture).
- `reloadConfig` reloads credentials only when the runtime created the
  `AuthStorage` itself (external `CredentialStore`s have no reload hook in the
  pi-ai trait). Recorded gap.
- `fetchDeferred`/`cancelDeferred` paths compile and delegate; no deferred
  provider fixture exists in this slice.
- The credential-sync failure test forces a failing `refreshModels` extension
  after an explicit provider refresh; the exact TS fixture (mock store failure)
  is not reproduced.
- Availability checks run concurrently via `JoinSet`. The snapshot stores auth
  and provider sets in `BTreeMap`/`BTreeSet` (sorted); the TypeScript `Map`
  insertion order is not observable through the public accessors.

## Discrepancies

None found.
