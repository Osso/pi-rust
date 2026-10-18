# google-client traceability

Private stand-in for the parts of `@google/genai` and `google-auth-library` that the Google adapters observe. Not a TypeScript module in `packages/ai`; it exists because the Rust port cannot depend on the Node SDKs.

Source of truth (read-only): `node_modules/@google/genai` (`ApiClient`, `Models.generateContentStream`, generated `*ToMldev`/`*ToVertex` transformers, SSE `processStreamResponse`, `throwErrorIfNotOK`, `NodeAuth`) and `node_modules/google-auth-library` (`GoogleAuth`, `JWT`, `UserRefreshClient`, `gtoken`).
Rust: `crates/pi-ai/src/api/google_client.rs`.
Tests: `crates/pi-ai/tests/differential_google_api.rs` (66 scenarios), `crates/pi-ai/tests/google_api.rs`.

## Behavior → proof
| SDK behavior | Rust | Test |
|---|---|---|
| `getRequestUrlInternal` (trailing slash, optional version segment) | `GoogleClient::request_url` | differential URLs, all scenarios |
| `constructUrl` + `shouldPrependVertexProjectPath` | `request_url` project/location prefix | differential ids 100-108 |
| `tModel` (Gemini `models/`, Vertex `publishers/...`) | `transform_model_name` | differential ids 100-107; model ids are plain in every scenario |
| `generateContentParametersTo{Mldev,Vertex}` + config/part/tool transformers | `request_body`, `transform_*` | differential request bodies |
| `getDefaultHeaders` + `patchHttpOptions` + `Headers.append` semantics | `HeaderList`, `GoogleClient::request_headers` | differential request headers |
| `NodeAuth.addKeyHeader` / `addGoogleAuthHeaders` | `request_headers` auth branch | differential headers |
| `processStreamResponse` SSE split (`\n\n`, `\r\r`, `\r\n\r\n`), `data:` prefix, per-chunk error JSON, incomplete tail | `GoogleStream::next_chunk`, `take_sse_event`, `check_chunk_error` | differential ids 39-43 |
| `TextDecoder('utf-8', {stream:true})` | `Utf8Decoder` | differential id 43 (multibyte split) |
| `throwErrorIfNotOK` (JSON vs text body, `ApiError.status`) | `read_http_error` | differential ids 33, 34 |
| `JSON.stringify` wire formatting (integral numbers) | `js_json` | differential request bodies (e.g. `maxOutputTokens: 100`) |
| `GoogleToken` JWT bearer exchange | `token_request_body`, `sign_service_account_jwt` | differential ids 103, 105-108, 120; `service_account_tokens_are_signed_rs256_with_the_google_claims` |
| `OAuth2Client.refreshTokenNoCache` | `token_request_body` authorized-user branch | differential id 104; `authorized_user_tokens_use_the_refresh_token_grant` |
| token caching / expiry | `GoogleClient::access_token` | `vertex_retries_reuse_the_cached_access_token` |
| `GoogleAuth` ADC file resolution | `load_credentials`, `well_known_adc_path`, `parse_credentials` | differential ids 103-108 (env path); well-known path untested |

## Gaps
- GCE metadata credentials, external/impersonated credential types, and universe-domain token URLs.
- gaxios retry config, proxy support, timeouts, and multipart/upload paths (not used by the adapters).
- The exact Node `SyntaxError` text for malformed SSE JSON; the differential normalizes both sides to `<json-parse-error>`.
- `maxRetries` fractional values truncate to `usize` (typed-core exclusion).

## Discrepancies
- None: the stand-in intentionally implements only the observed Pi paths.
