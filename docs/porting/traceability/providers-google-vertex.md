# providers/google-vertex traceability

Pinned source: `packages/ai/src/providers/google-vertex.ts` at
`532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-ai/src/providers/google_vertex.rs`.
Tests: `crates/pi-ai/tests/provider_auth.rs`,
`crates/pi-ai/tests/differential_providers.rs` (`auth_resolution_matches_ts`,
`provider_login_flows_match_ts`).

| TypeScript | Rust | Proof |
|---|---|---|
| `vertexAuth.name` | `VertexAuth::name` (`"Google Cloud credentials"`) | `provider_metadata_matches_the_ts_factories` |
| `login` select (`api-key`, `adc`, `service-account`) | `login` | `vertex_login_covers_key_adc_and_service_account_flows`, differential login |
| `api-key` branch → secret prompt → `{type:"api_key", key}` | `ApiKeyCredential { key }` | same |
| ADC/service-account → `info` event + project/location (+ credentials path) prompts → env credential | `AuthEvent::Info` + `ApiKeyCredential { env }` | same |
| unknown method → `Unknown Google Vertex AI auth method: <method>` | same message | same |
| `resolve`: stored/env `GOOGLE_CLOUD_API_KEY` → `ctx.fileExists(adcPath ?? VERTEX_ADC_PATH)` with project (`GOOGLE_CLOUD_PROJECT`/`GCLOUD_PROJECT`) and location → `undefined` | `resolve` | `vertex_resolve_prefers_api_key_then_adc_with_project_and_location`, `vertex_stored_credential_env_drives_adc_resolution`, differential auth |
| `googleVertexProvider()` metadata/catalog/vertex api | `google_vertex_provider` | provider metadata test; request parity lives in `differential_google_api` (vendor SDK) |

## Branch notes

- `??` chains use presence, not truthiness: a stored empty key suppresses the env
  fallback, while empty env values are treated as unset.
- `VERTEX_ADC_PATH` is `~/.config/gcloud/application_default_credentials.json`;
  `AuthContext::file_exists` expands a leading `~`.
- ADC requires a non-empty project and location; the returned `env` is
  `credential.env` verbatim and the source is `stored credential` when a
  credential was supplied, otherwise `gcloud application default credentials`.

## Gaps

- Dispatch request parity is not part of `provider_dispatch_matches_ts_requests`
  (the `@google/genai` SDK bypasses `globalThis.fetch`).

## Discrepancies

- None.

## Q18 provider request integration (2026-10-22)

Login project/location/credential path values and stored/environment auth results retain JsString. AuthContext file_exists receives the original native path. The public-provider login case is mapped in [focused boundary proof](lossless-sdk-boundaries.md).

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.
