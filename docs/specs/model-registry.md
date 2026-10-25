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

Model definitions have optional api/baseUrl, not full output-Model requirements. Missing baseUrl inherits the provider endpoint; explicit empty baseUrl remains empty. API selection retains frozen's logical default. Registration reconstructs only the declared model fields: unrelated root properties, samplingParams and autoCompactionThreshold are dropped. Nested cost/thinking/compat values and their order remain exact; copying a Model into a registration input must not serialize or project those values. Native ProviderModelInput and explicit From<Model> implement this boundary; no empty-string absence sentinels.

OAuth model callbacks run without the registry lock. During registration and reapplication, callbacks can read the newly installed model list and register other providers. The returned model list replaces the current list after the callback returns; registered metadata is upserted after application, matching frozen ordering.

## How it works

- [Traceability and current proof bounds](../porting/traceability/model-registry.md).

## Implementation inventory

`crates/pi-coding-agent-core/src/core/model_registry.rs` and internal `model_registry/schema.rs`: catalog, configuration, request auth and extension registration. Session/services/SDK expose the same registry and storage to consumers.

## Tests asserting this spec

`crates/pi-coding-agent/tests/model_registry_frozen.rs`, `model_registry_auth_drift.rs` and `model_registry_oauth_throw_probe.rs` use `oracle/model-registry-oracle.ts` against the actual frozen module. Other registry/catalog/UI matrices and all-target checks remain open.

## Known gaps (current cycle)

- [ ] Complete consumer test migration, configuration/registration/request-auth differential matrices, production cache wiring, nested compat unknown fields and real RPC serialization proof.

## Out of scope

Live providers/network, new semantic deviations, source-absent native Provider registration and old model-runtime/publication/cache-wrapper behavior.
