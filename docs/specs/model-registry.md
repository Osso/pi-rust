# Frozen model registry

Port `packages/coding-agent/src/core/model-registry.ts` at the [common pin](../porting/source-lock.json). Source-absent runtime/composer/store layers are removed, not compatibility paths.

## What it must do

- [x] Registry availability, configured-auth and OAuth checks observe live AuthStorage mutations without an asynchronous snapshot refresh.
- [x] OAuth key callbacks preserve exact Error branding/properties and arbitrary thrown values. Request-auth catches use Error.message or shared JS String conversion; a throw from String conversion remains a rejected Result, distinct from the auth-error union.
- [ ] `create(AuthStorage, optional models path)` and `in_memory(AuthStorage)` synchronously load models; `refresh()` reloads auth/config and reapplies registered providers synchronously.
- [ ] Custom model validation, default fields, replacement order, provider/model overrides, headers and compat merges match frozen behavior; human strings and unknown model fields remain lossless.
- [ ] Request auth uses AuthStorage before provider config; provider/model/static header precedence, authHeader errors, status labels and display names match frozen behavior.
- [ ] SDK/session/RPC/UI/CLI consumers use the registry and AuthStorage directly. OAuth provider discovery and API-key login options match the frozen interactive helpers. Startup uses only a fresh OpenRouter cache, merged with bundled models winning collisions.

## Extension model registration inputs

Model definitions have optional api/baseUrl, not full output-Model requirements. Missing baseUrl inherits the provider endpoint; explicit empty baseUrl remains empty. API selection retains frozen's logical default. Registration reconstructs only the declared model fields: unrelated root properties, samplingParams and autoCompactionThreshold are dropped. Nested cost/thinking/compat values and their order remain exact; copying a Model into a registration input must not serialize or project those values. Native ProviderModelInput and explicit From<Model> implement this boundary; no empty-string absence sentinels. Custom Anthropic/GitLab example factories emit source-shaped definitions without injecting per-model API/provider/baseUrl fields; their missing fields inherit the provider configuration.

OAuth model callbacks run without the registry lock. During registration and reapplication, callbacks can read the newly installed model list and register other providers. The returned model list replaces the current list after the callback returns; registered metadata is upserted after application, matching frozen ordering.

Request auth captures provider configuration and environment before awaiting the storage key. Model request headers are read after that key resolution, so reentrant key callbacks can replace model headers for the current request while provider settings remain the pre-await snapshot.

Provider key lookup preserves any defined storage key, including empty strings. Only a truthy configured provider fallback invokes uncached resolution and reads provider environment; an explicitly empty configured key yields no fallback key even if a headers object caused its request record to be stored. Auth/status flags retain frozen behavior rather than inferring key validity.

Model-config paths are normalized once during construction. Empty paths disable file loading. Invalid file URLs fail construction with native error identity/properties; they are not converted into getError state. Refresh retains the normalized selected path even if HOME changes, and load-error messages use that path.

OAuth model modifiers return Result<Vec<Model>, CallbackError>. Registration, both constructors, refresh and unregister propagate that same rejection without converting it to text. Preserve Error branding/properties and arbitrary values, including lone UTF-16 units. Failed application leaves already-installed models/global registrations intact and does not upsert the incoming provider config. A failed load-time modifier does not publish the new combined model list. Ordinary config-file errors remain getError state.

The model-selector consumer distinguishes refresh rejection from its caught availability lookup failure. Selector construction stays infallible; an uncaught refresh rejection is published after construction through the existing shared uncaught authority with its original carrier. This is not a rendered getter error or a new panic channel.

Provider reapplication during refresh follows the live insertion order. A modifier's newly appended provider is visited in that pass; a deleted provider is skipped; deletion and reinsertion creates a new visit with its current config. Updating an existing provider retains its position. A stale snapshot must not restore deleted models or old config.

Registered raw/simple callbacks preserve synchronous throws according to the shared [stream acquisition contract](provider-stream-errors.md). The registry wrapper does not invent an error event where frozen returns no stream.

## How it works

- [Traceability and current proof bounds](../porting/traceability/model-registry.md).

## Implementation inventory

`crates/pi-coding-agent-models/src/core/model_registry.rs` and internal `model_registry/schema.rs`: catalog, configuration, request auth and extension registration. Session/services/SDK expose the same registry and storage to consumers.

## Tests asserting this spec

`crates/pi-coding-agent/tests/model_registry_frozen.rs`, `model_registry_auth_drift.rs` and `model_registry_oauth_throw_probe.rs` use `oracle/model-registry-oracle.ts` against the actual frozen module. Other registry/catalog/UI matrices and all-target checks remain open.

## Known gaps (current cycle)

- [ ] Complete consumer test migration, configuration/registration/request-auth differential matrices, production cache wiring, nested compat unknown fields and real RPC serialization proof.

## Out of scope

Live providers/network, new semantic deviations, source-absent native Provider registration and old model-runtime/publication/cache-wrapper behavior.
