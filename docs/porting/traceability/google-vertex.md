# Google Vertex API traceability

Frozen: `packages/ai/src/api/google-vertex.ts` at [source lock](../source-lock.json),599 lines. Native: `crates/pi-ai/src/api/google_vertex.rs` and `google_client.rs`.

## Whole-module body audit (2026-10-25)

| Source symbols / branches | Native proof |
|---|---|
| `GoogleVertexOptions`, API_VERSION, credentials marker, THINKING_LEVEL_MAP | typed options and identical SDK enum strings; original21 real-SDK loopback scenarios |
| `resolveApiKey`, `isPlaceholderApiKey`, `resolveProject`, `resolveLocation` | trimmed API key; marker/empty/placeholder→ADC; truthy project→GOOGLE_CLOUD_PROJECT→GCLOUD_PROJECT; truthy location→GOOGLE_CLOUD_LOCATION; exact missing errors; new empty-option/env-precedence cases |
| `createClient`, `createClientWithApiKey`, `buildHttpOptions`, `resolveCustomBaseUrl`, `buildGoogleAuthOptions` | native ADC/key modes, custom collection-scope endpoint or location defaults, placeholder base suppression, model→caller headers, captured keyFilename; credentials load is deferred until HTTP auth, after onPayload; genuine callback-priority RED pins missing-file ordering |
| `baseUrlIncludesApiVersion`, SDK constructUrl/requestStream | parsed pathname version segments, regex-equivalent segment fallback only for invalid URL input; query-bearing base URL regression; URL serialization and alt=sse setter preserve source behavior; full projects/... model resource does not get a second caller project prefix |
| `buildParams`, thinking config and disabled/level/budget helpers | exact source body text;65535 cap, optional system/tools/choice, latest server-managed alias, unanchored Pro/Flash match, gemini-3.7-flash off LOW; Vertex budget map intentionally differs from Studio Flash-Lite/Gemma cases |
| `streamSimple` | base field selection, no synchronous auth requirement, reasoning clamp/off→high, explicit level or budget; API-specific project/location are not copied by source base helper |
| `stream`: fixed output API, resolve client, replaceable payload, SDK request/start, shared text/thinking/function state machine, first response ID, finish/usage/cost, final close/abort/error/catch | original21 plus new wire/config cases and shared native callback matrix; text/signature/argument values stay native; empty/duplicate IDs generate from the Vertex-only counter |
| module toolCallCounter, absent function name, lazy factory | exact source/native isolated counter sequence1,1,2,2 across alternating Studio/Vertex calls; absent name prefix `undefined_` retained; native lazy-wrapper behavior |

**DISCREPANCIES preserved:** Simple drops project/location passed as API-specific extras and requires env-derived values in ADC mode (original source cases111/112); absent function name yields `undefined_` in the generated ID while tool name is empty. Neither behavior is silently fixed.

## Explicit gap rows (not full ADC acceptance)

| Status | Source boundary / remaining capability |
|---|---|
| gap | Existing SDK stand-in does not implement GCE metadata credentials or external_account, external_account_authorized_user, impersonated_service_account ADC files; those valid SDK credential modes remain unported, not silently ignored. |
| gap | Universe-domain token URLs and broad gaxios proxy/retry/transport-fault behavior are outside the proven service_account/authorized_user token exchange. Well-known ADC path and process-env endpoint overrides are implemented but not separately isolated in the current differential. |

The approved [typed-native payload signal gap](google-client.md#typed-native-payload-signal-boundary) also applies. Pathological wrong-typed JSON, numeric extremes, OS/runtime fault shapes and unmatched units beyond existing opaque/schema carriers retain the project bar.

Supported service-account RS256/JWT and authorized-user refresh-token wire requests are proven with the actual frozen google-auth-library through a private CONNECT/TLS proxy; only JWT time/signature and host-version substrings are normalized. The new full-resource auth test first omitted the fixture's existing private TLS allowance; that no-request TLS setup failure is not counted as behavioral RED. After matching the established isolated proxy setup, the exact double-project URL mismatch was genuine RED. [Ledger](../ai-api-repin-proof.md); [SDK scope](google-client.md).
