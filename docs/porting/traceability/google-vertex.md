# google-vertex traceability

Source: `packages/ai/src/api/google-vertex.ts` and `google-vertex.lazy.ts`.
Rust: `crates/pi-ai/src/api/google_vertex.rs` (plus the private SDK/auth stand-in `google_client.rs`).
Tests: `crates/pi-ai/tests/differential_google_api.rs` (21 recorded-fixture scenarios, ids 100-120, against the pinned TypeScript module with the real `google-auth-library` token exchange replayed through a local CONNECT/TLS proxy), `crates/pi-ai/tests/google_api.rs` (8 behavioral).

## Representation
- `GoogleVertexOptions extends StreamOptions` → `GoogleVertexOptions { stream, tool_choice, thinking, project, location }`, parsed from `StreamOptions::extra`.
- `streamSimple` reproduces the TypeScript `buildBaseOptions` shape: `project`/`location` are **not** copied, so only env-provided project/location reach Vertex.
- Vertex API-key detection: trimmed key; empty, `gcp-vertex-credentials`, and `/^<[^>]+>$/` placeholders fall back to ADC.
- ADC: `GOOGLE_APPLICATION_CREDENTIALS` (provider env, then process env) or `$HOME/.config/gcloud/application_default_credentials.json`. Supported credential types: `service_account` (JWT bearer grant, RS256) and `authorized_user` (refresh-token grant).
- `THINKING_LEVEL_MAP` maps every level to the identical Google enum string, so the level is serialized directly.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `GoogleVertexOptions` | `GoogleVertexOptions` | differential ids 100-120 |
| `stream` | `stream` | differential ids 100-120 |
| `streamSimple` | `stream_simple` | differential ids 111, 112 |
| `createClient` | `GoogleClient::vertex_adc` | differential ids 103-108, 120 |
| `createClientWithApiKey` | `GoogleClient::vertex_api_key` | differential ids 100-102, 109, 110 |
| `buildHttpOptions` | `resolve_custom_base_url`, `base_url_includes_api_version` | differential ids 101, 102, 108 |
| `resolveCustomBaseUrl` (`{location}` placeholder) | `resolve_custom_base_url` | differential ids 100, 105-107 |
| `baseUrlIncludesApiVersion` | `base_url_includes_api_version` | differential id 102 |
| `buildGoogleAuthOptions` | `load_credentials` (provider env) | differential ids 103, 104 |
| `resolveApiKey` / `isPlaceholderApiKey` | `resolve_api_key`, `is_placeholder_api_key` | differential ids 109, 110 |
| `resolveProject` | `resolve_project` | differential id 112, `vertex_stream_simple_without_project_or_env_reports_the_typescript_message` |
| `resolveLocation` | `resolve_location` | differential ids 103-108 |
| `buildParams` | `build_params` | differential request bodies |
| `getDisabledThinkingConfig` | `get_disabled_thinking_config` | differential id 113 |
| `getGemini3ThinkingLevel` | `get_gemini3_thinking_level` | differential ids 111, 114 |
| `getGoogleBudget` | `get_google_budget` | differential id 115 |
| `googleVertexApi` (lazy) | `google_vertex_api` | `lazy_api_wrappers_expose_the_provider_streams_contract` |
| token exchange (google-auth-library) | `GoogleClient::access_token`, `sign_service_account_jwt` | differential ids 103, 104, 120; `service_account_tokens_are_signed_rs256_with_the_google_claims` (RS256 verified with the fixture public key), `authorized_user_tokens_use_the_refresh_token_grant`, `vertex_retries_reuse_the_cached_access_token` |

## Branch notes
- Default endpoint selection: `global` → `https://aiplatform.googleapis.com/`; `us`/`eu` → `https://aiplatform.{location}.rep.googleapis.com/`; otherwise `https://{location}-aiplatform.googleapis.com/`.
- ADC requests prepend `projects/{project}/locations/{location}`; API-key mode and custom base URLs (`ResourceScope.COLLECTION`) do not.
- Custom base URL already containing a `vN[betaN]` path segment suppresses the `v1` path element.
- `GOOGLE_VERTEX_BASE_URL` process env replaces the location-derived base URL when the model has no custom base URL.
- Service-account JWT: header `{"alg":"RS256"}`, claims `iss, scope, aud=https://oauth2.googleapis.com/token, exp=iat+3600, iat`, `grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer` then `assertion` in the body, `accept: application/json`.
- Authorized-user token body order: `refresh_token, client_id, client_secret, grant_type=refresh_token`, `accept: */*`.
- The token exchange is never cancelled by the request signal (google-auth-library's transporter has no AbortSignal); the adapter's request still passes the signal to `fetch`.
- Retries reuse the cached access token within one client instance.

## Gaps
- Metadata-server (GCE) credentials, `external_account`, `external_account_authorized_user`, and `impersonated_service_account` ADC types are not implemented; they fail with "Unsupported application default credential type".
- The well-known ADC path reads `$HOME` at request time; it is implemented but not exercised by tests (process env isolation).
- `GOOGLE_VERTEX_BASE_URL` fallback is implemented but not differentially exercised.
- Token-endpoint transport failures without a JSON error body use gaxios's `Request failed with status code <status>` message; the exact gaxios text for exotic failures is not reproduced.
- Non-object `functionCall.args` and non-object `onPayload` replacements follow the typed-core exclusions noted in `google-generative-ai.md`.

## Discrepancies
- **Source bug preserved**: `streamSimple` calls `buildBaseOptions`, which does not copy `project`/`location`, so `streamSimple(model, context, {project, location})` cannot reach Vertex without env-provided project/location. The Rust port preserves the behavior; differential id 112 pins the resulting error and id 111 pins the env-provided path.
