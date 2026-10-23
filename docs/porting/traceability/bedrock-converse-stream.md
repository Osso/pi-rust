# `ai/src/api/bedrock-converse-stream.ts` traceability

Source: `packages/ai/src/api/bedrock-converse-stream.ts` (pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-ai/src/api/bedrock_converse_stream.rs`
plus the Rust-only helpers `bedrock_sigv4.rs`, `bedrock_eventstream.rs`,
`bedrock_credentials.rs`, and `utils/base64.rs`.

## Real Smithy abort-reason boundaries

`differential_bedrock_converse_stream::bedrock_custom_abort_reasons_match_real_smithy_header_and_body_boundaries` drives actual frozen provider + Smithy NodeHttpHandler over loopback HTTP, and Rust default_fetch over an equivalent real socket. Ten reasons × pre-abort/pending headers/after-response body boundaries compare the complete final AssistantMessage, normalizing only timestamp. Reasons include default/null/false/zero/empty string, native UTF-16 text/arrays, plain Error lookalike, native Error and TimeoutError DOMException.

`bedrock-abort-reasons-red-2.log` → `bedrock-abort-reasons-green-2.log`30 GREEN: header handling reads the original signal reason. Error/DOMException and falsey raw values yield Request aborted; truthy raw values use native String conversion. Response-body cancellation yields Node's aborted failure, not a fabricated early Request was aborted; final post-stream cancellation check remains separate. No raw reason is inferred as an Error from name/message fields. HTTP non-2xx body cancellation, malformed custom coercion and later partial-content cancellation remain unproven.

The initial oracle blocked because Smithy's deserializer reads the first event frame before client.send/onResponse resolves. Fixture now writes the same messageStart frame on both real sockets, then holds the body. A new request-specific2s source watchdog plus native4s bound prevents recurrence. First unexpected10-minute job was cancelled; cargo/tsx process groups and descendants were verified gone. Raw failure/progress logs remain under `/tmp/pi-rust-repin-delta-ai`; no hung source run counts as behavioral RED.

## Q4 SDK identity boundary

Unmasked source request headers exposed native lang/rust and missing SDK language/compiler/feature labels. Native now retains frozen aws-sdk-js3.1048.0, ua2.1, lang/js, md/tsc5.9.3, Bedrock API version and retry/credential feature labels (standardE, directe, profilen). Host kernel/Node versions are unavailable in Rust and emitted as unknown; only those version substrings are masked at captured default User-Agent. Explicit caller User-Agent remains exact. A concrete helper test preserves changed SDK/compiler/language/feature values. The first signed source request now matches all headers/signature inputs; next RED exposes start/delta index, empty-start ordering and source-absent rawStopReason (`bedrock-sdk-identity-{red,green-1}.log`). No full-provider acceptance yet.

## Native scratch and push ordering

Frozen handlers433–561 retain index/partialJson on output blocks; no second content vector exists. Removed native BlockState/sync_content copies and narrowed i64 indices. Text/thinking starts now snapshot empty content before the delta; thinking signatures append only after the text delta. Finalization strips scratch before constructing end snapshots; owner catches strip open-block scratch. Removed source-absent rawStopReason/error derivation. New fractional0.25/0.5/0.75/0.875 interleaving reproduces the old collapsed blocks RED then passes exact events. Existing text/tool/thinking scenarios advance too; next RED is native rich-message removal of empty tool-argument keys (`bedrock-native-scratch-{red,green-1}.log`). This is a partial checkpoint, not complete Bedrock parity.

Frozen rich-message request retains empty tool-argument keys. Removed sanitize_bedrock_document's recursive empty-key deletion; no request/expected body projection was added. Exact matrix advances from rich-messages to source-absent options.apiKey bearer fallback (`bedrock-document-keys-green-1.log`).

Frozen bearer auth uses only explicit bearerToken/AWS_BEARER_TOKEN_BEDROCK, not common apiKey. Exact source scenario previously named bearer actually ignores apiKey and signs with configured credentials; removed native apiKey fallback and corrected the independent signature assertion. Kept that input as source-ignores-api-key and added explicit-bearer coverage; both source request paths now match. Next RED was missing custom endpoint featureN (`bedrock-native-key-green-2.log`). Frozen Smithy endpoint middleware adds ENDPOINT_OVERRIDEN only for a custom endpoint; native now emits it in frozen order before retry/credential labels. Matrix advances to error shape (`bedrock-endpoint-feature-green-1.log`): old native diagnostics are source-absent, and the oracle's Readable.from HTTP stand-in leaks synthetic stream internals into frozen error text. Removed source-absent diagnostics and their emitter; stale unit expectations for diagnostics/rawStopReason and common-apiKey bearer were replaced with actual frozen behavior. Local real-HTTP probe established actual frozen bug: HTTP400 ValidationException becomes `Validation error: 400: [object Object]` because normalizeProviderError stringifies consumed IncomingMessage, detects a cycle and uses String(responseBody). Replaced oracle Readable.from stand-in with original NodeHttpHandler redirected only to a scripted127.0.0.1 server; actual SDK/HTTP errors now reproduce that same result (`bedrock-loopback-oracle-red.log`, standalone `bedrock-real-http-probe-2.log`). Parent classified preserve literally (2026-10-22): native HTTP failure body now represents that Node boundary as `[object Object]`; no oracle error masking. DISCREPANCY: frozen `utils/error-body.ts:89–92` makes actual Node IncomingMessage failures lose raw body text; loopback proof `/tmp/pi-rust-repin-delta-ai/bedrock-real-http-probe-2.log` and `bedrock-loopback-oracle-red.log`. HTTP400 validation now compares exactly. ServiceUnavailable retry-count RED was loopback server auto-Date versus frozen signing clock: it caused an extra SDK clock-skew retry. Disabled only automatic Date header generation so headers come solely from the scripted response, retaining any explicitly scripted Date. Unmasked matrix20 scenarios plus host-version preservation regression GREEN (`bedrock-loopback-scripted-date-green.log`); all current request/event/result fields compare, with no error normalization. Added real loopback plain403 gateway scenario to bound the source bug: parsed SDK service exceptions stringify IncomingMessage, but unparseable raw HTTP bodies remain exact strings. That new RED prevented overgeneralizing `[object Object]` to gateway text. Native now distinguishes parsed service responses from raw bodies. Final matrix21 scenarios+host-version regression and17 behavioral tests GREEN (`bedrock-real-http-complete-green.log`).

## Exported symbols

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `BedrockThinkingDisplay` | `BedrockThinkingDisplay` | `accepts_tool_choice_and_thinking_display_options`, differential `thinking-*` |
| `BedrockOptions` (`region`, `profile`, `toolChoice`, `reasoning`, `thinkingBudgets`, `interleavedThinking`, `thinkingDisplay`, `requestMetadata`, `bearerToken`) | `BedrockOptions` + `BedrockOptions::from_stream_options` (API fields parsed from `StreamOptions::extra`) | behavioral options/region/profile tests; differential `thinking-*`, `rich-messages`, `profile-credentials`, `bearer` |
| `stream` | `stream` | `bedrock_converse_stream` behavioral suite; 18 differential scenarios |
| `streamSimple` | `stream_simple` | `stream_simple_adjusts_thinking_budget`; shared `simple-options` tests |

`BedrockToolChoice` is a Rust enum with a manual `Deserialize` matching
`"auto" | "any" | "none" | { type: "tool"; name }`.

## Behavior map

| Source branch | Rust | Coverage |
| --- | --- | --- |
| Client config: profile precedence, endpoint pinning, ARN region, bearer/skip auth | `resolve_runtime_config`, `standard_endpoint_region`, `arn_region`, `default_endpoint` | `resolves_regions_endpoints_and_inference_profile_arns`, `uses_bearer_token_and_skip_auth_credentials`, `reads_static_profile_credentials`; differential `arn-region`, `custom-endpoint`, `profile-credentials`, `skip-auth`, `bearer` |
| Credential chain: env keys, shared credentials/config profiles, SSO cache + `GetRoleCredentials`, dummy skip-auth | `bedrock_credentials::{env_credentials, profile_credentials, profile_region}` | `reads_static_profile_credentials`; differential `profile-credentials`; SSO implementation not exercised by a test (see gaps) |
| SigV4 signing, session token, payload hash, canonical path/headers | `bedrock_sigv4::sign_request` | differential signer check against the SDK's recorded `Authorization` for every non-bearer scenario |
| Custom header `build` middleware, reserved `x-amz-*`/`authorization`/`host` skip | `apply_custom_headers`, `is_reserved_header` | `ignores_reserved_custom_headers`; differential `rich-messages` |
| Request construction, `onPayload`/`onResponse`, `onPayload` replacement | `build_command_input`, `apply_on_payload`, `execute_request` | differential `thinking-*` captured payloads; `rich-messages` |
| Retry middleware (3 attempts, attempt header, retry-after) | `execute_request`, `is_retryable`, `retry_delay`, `retry_after_ms` (Smithy `parseRetryAfterHeader`: `Number` for `x-amz-retry-after`, numeric pattern/`" GMT, <seconds>"`/HTTP date/`Date.parse` for `retry-after`) | `retries_throttling_then_reports_validation_diagnostic`, `x_amz_retry_after_uses_javascript_number_parsing`, `retry_after_hints_follow_smithy_parsing`; differential `errors-retry`, `service-unavailable` |
| Message conversion: text/image/tool-use/tool-result/thinking/cache points, empty placeholders, empty-key document sanitization | `convert_messages`, `convert_user_message`, `convert_assistant_message`, `convert_tool_result`, `sanitize_bedrock_document` | `converts_replayed_messages_and_documents`, `adds_long_cache_points_to_system_and_last_user_message`; differential `rich-messages`, `cache-points` |
| System prompt + cache point (`PI_CACHE_RETENTION`, TTL `1h`) | `build_system_prompt`, `cache_point`, `resolve_cache_retention` | cache-point behavioral test; differential `cache-points` |
| Tool config, strict mode, tool choice | `convert_tool_config` | `converts_replayed_messages_and_documents`, `accepts_tool_choice_and_thinking_display_options`; differential `rich-messages` |
| Thinking payload: adaptive vs fixed budget, display, GovCloud omission, xhigh effort, interleaved beta | `build_additional_model_request_fields`, `map_thinking_level_to_effort`, `is_gov_cloud_bedrock_target` | `builds_adaptive_and_budget_thinking_payloads`, `accepts_tool_choice_and_thinking_display_options`; differential `thinking-*` |
| Event stream: message start, block start/delta/stop, metadata, stop reason | `handle_message`, `handle_content_block_delta`, `handle_content_block_stop`, `handle_metadata`, `map_stop_reason` | `streams_text_usage_and_stop_reason`, `streams_reasoning_with_signature`, `streams_tool_use_with_partial_json`; differential `text`, `reasoning`, `tool-use` |
| Event-stream framing + CRC32 checks | `bedrock_eventstream::EventStreamDecoder` | `reports_crc_checksum_failures`; differential `corrupt-message-crc`, `corrupt-prelude-crc` |
| Mid-stream exceptions | `handle_message` exception branch, `format_bedrock_error` | differential `mid-stream-exception` |
| HTTP errors + diagnostics (`status`, `errorCode`, `requestId`, data-retention hint) | `http_error`, `format_bedrock_error`, `append_bedrock_failure_diagnostic` | `retries_throttling_then_reports_validation_diagnostic`, `surfaces_gateway_bodies_and_stop_reason_errors`; differential `errors-retry`, `service-unavailable` |
| Abort signal | `BedrockOptions::signal`, fetch/body cancellation, post-loop abort check | `aborts_before_the_request` |
| `streamSimple` budget clamping | `stream_simple` + shared `simple_options` | `stream_simple_adjusts_thinking_budget` |

## Differential oracle

`oracle/bedrock-converse-stream-oracle.ts` imports the pinned real module, fixes
`Date` and `crypto.randomUUID`, patches the Smithy `NodeHttpHandler.handle` hook
(forcing `AWS_BEDROCK_FORCE_HTTP1=1`), records signed requests, and replays
scripted event-stream bytes split at awkward boundaries. It never contacts AWS.

`crates/pi-ai/tests/differential_bedrock_converse_stream.rs` compares URL, body
bytes, all headers except environment/clock/random-derived values, the full
event sequence, final message, and captured `onPayload`. Every non-bearer
scenario also re-signs the recorded request with the fixed clock and compares
`Authorization` byte-for-byte.

Normalized values: `timestamp` (clock), `x-amz-date` and `Authorization`
(clock; Authorization is separately proven), `amz-sdk-invocation-id` (random),
`user-agent` (runtime-derived, unsigned). The transient event-partial scratch
fields `index` and `partialJson` are stripped; Rust's typed content model does
not carry them and the final message has neither.

Scenarios: text, reasoning/signature, tool-use streaming, cache points, adaptive
high/xhigh/GovCloud thinking, budget thinking, rich messages/images, bearer,
ARN region, custom endpoint, retry+validation error, service error, mid-stream
exception, corrupted message/prelude CRC, explicit profile credentials, skip
auth.

## Gaps and discrepancies

- SSO credential exchange is implemented (token cache + `GetRoleCredentials`)
  but has no test: it needs a controlled `HOME`/SSO cache and a fake SSO
  endpoint. Token refresh through SSO-OIDC is not implemented; expired tokens
  produce the SDK's "run aws sso login" error.
- Not implemented credential providers: `credential_process`, web identity,
  role assumption (`role_arn`/`source_profile`), ECS container and EC2 IMDS.
  These fail with "Could not load credentials from any providers".
- Ambient `AWS_PROFILE` resolution is implemented, but not differential-tested
  because Rust tests cannot safely mutate process environment under
  `unsafe_code = "forbid"`; scoped/explicit profiles cover the same profile
  reader.
- Tool-call arguments that parse to a non-object JSON value become an empty
  object in Rust (`ToolCall::arguments` is a typed object). TypeScript would
  retain the non-object value; this is an intentional typed-core exclusion.
- `StreamOptions.maxRetries` is ignored, matching the source module (the Bedrock
  client config never forwards it).
- Event partial snapshots omit the transient `index`/`partialJson` scratch
  fields described above.
- Abort timing is not differential-tested: the synthetic request handler cannot
  reproduce real transport teardown deterministically. Behavioral tests cover
  immediate abort mapping; the source's abort unit tests are live-only.
- The retry delay is the port's attempt-based backoff; it does not reproduce
  Smithy's `max(backoff, min(hint, backoff + 5s))` clamp. Header parsing does
  follow Smithy: `x-amz-retry-after` uses `Number`, and `retry-after` uses the
  numeric pattern, the `" GMT, <seconds>"` suffix, an HTTP date, or
  `Date.parse` (V8 legacy formats and pre-1970 HTTP dates are not reproduced).

Q18 shared UTF-16 fields, argument parser and outbound-tree migration: [boundary/proof matrix](lossless-content.md). Checkpoint evidence does not establish unchanged consumer-fixture compatibility or full workspace parity.
