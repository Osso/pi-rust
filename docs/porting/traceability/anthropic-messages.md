# anthropic-messages traceability

Shared pinned SDK request reasons/deadlines: [OpenAI/Anthropic SDK boundary proof](openai-sdk.md#shared-sdk-reasondeadline-boundary). Actual SDK request matrices cover38 cases; whole-provider event/body rejection reason behavior remains partial.

## Q4 common-pin status

Partial only at uncompleted shared interface/boundary audit. Corrected global-fetch oracle drives real frozen SDK/SSE. Current whole scenario matrix GREEN (1 differential target), plus 26 behavioral tests. Revision `495a87d6` + Anthropic slice; `/tmp/pi-rust-repin-delta-ai/ledger.md`, `anthropic-frozen-stream-final.log`, `anthropic-unit-final.log`. Historical fetch claims were invalid at that boundary: [audit](../ai-scripted-provider-oracles.md).

| Re-pin branch / exported flow | Frozen refs | Current proof |
|---|---|---|
| stream/streamSimple terminal output, no rawStopReason | 490–740, mapStopReason 1215–1238 | Whole actual-import differential; exact result/event equality, no field dropped |
| convertTools: plain properties/required, no strict/constrained schema rewrite | 1188–1210 | Prior scenario16 RED → GREEN; old-only strict/deferred/reference machinery removed |
| tool-result conversion, no synthetic tool references | 1007–1015 | Whole differential; old feature-only tool-reference family test deleted |
| SDK retries report per-attempt x-stainless-retry-count, custom-header override retained | frozen installed SDK `client.mjs:447–473` | Prior scenario27 RED → GREEN |
| text/thinking start blocks begin empty, signatures start empty | 561–574 | Prior scenario42 RED → GREEN; `frozen_source_discards_initial_text_thinking_and_signature` full-content assertion |

**DISCREPANCY:** frozen lines 561–574 discard nonempty text/thinking/signature supplied in content_block_start. Native now preserves this source behavior literally; it does not silently retain the old fix. RED full-content behavioral test preceded production change; signature behavior additionally proven by the actual frozen oracle.

Approved `request.fetch` seam: unset must match frozen default fetch (`AGENTS.md`). Remaining shared gaps are scalar Tool/schema interface boundaries and source/runtime timestamp-number migration, tracked under [types](ai-types.md). No other deviation inferred; no oracle assertion narrowed.

Q18 human-error representation and bounded proof: [lossless provider errors](lossless-provider-errors.md). Earlier counts are historical, not rerun evidence.

Source: `packages/ai/src/api/anthropic-messages.ts` (1366 lines) and
`packages/ai/src/api/anthropic-messages.lazy.ts` (4 lines).

Rust: `crates/pi-ai/src/api/anthropic_messages.rs`,
`crates/pi-ai/src/api/anthropic_sdk.rs` (private SDK request path),
`crates/pi-ai/src/api/anthropic_messages_lazy.rs`.

Tests: `crates/pi-ai/tests/anthropic_messages.rs` (27 behavioral tests),
`crates/pi-ai/tests/differential_anthropic_messages.rs` (44 recorded-fixture
scenarios against the pinned TypeScript module).

## Exports → proof

| TypeScript | Rust | Test |
|---|---|---|
| `AnthropicEffort` | `AnthropicEffort` | adaptive/effort request bodies in differential scenarios |
| `AnthropicThinkingDisplay` | `AnthropicThinkingDisplay` | `thinkingDisplay` scenarios |
| `AnthropicOptions` | `AnthropicOptions` + `from_stream_options` | every request-body test |
| `stream` | `stream`, `AnthropicMessagesApi::stream` | behavioral suite + 44 differential scenarios |
| `streamSimple` | `stream_simple` | seven thinking-level scenarios, legacy budgets, no-reasoning |
| `anthropicMessagesApi` (`lazy.ts`) | `anthropic_messages_api` | `lazy_api_wrapper_streams_without_deferred_capabilities` |

`AnthropicOptions` is parsed from `StreamOptions::extra`; invalid typed values
produce a stream error (`Invalid Anthropic option …`). The TS `client` injection
option has no Rust representation (typed exclusion).

## Request construction (`anthropic_sdk.rs`)

The TS module delegates to `@anthropic-ai/sdk` 0.91.1. The private Rust port
reproduces the observable request:

| Branch | Behavior | Test |
|---|---|---|
| URL | `{baseUrl}/v1/messages` with a single slash at the join | header tests; all differential requests |
| Auth | `x-api-key` for API keys, `Authorization: Bearer` for Copilot/OAuth, no auth header when headers own auth | `request_headers_carry_sdk_identity_and_auth`, `header_owned_auth_skips_api_key_and_oauth_shaping`, `oauth_tokens_use_claude_code_shaping_and_map_tool_names`, `copilot_uses_bearer_auth_and_dynamic_headers` |
| SDK headers | `Accept`, `User-Agent: Anthropic/JS 0.91.1`, `X-Stainless-Retry-Count: 0`, `X-Stainless-Timeout`, Stainless platform identity, `anthropic-dangerous-direct-browser-access`, `anthropic-version: 2023-06-01`, `content-type: application/json` | same |
| Default headers | base, session affinity, model headers, caller headers; later sources override and `null` suppresses | cache/session scenarios |
| Body | `JSON.stringify` key order (`model`, `messages`, `max_tokens`, `stream`, then optional `system`, `temperature`, `tools`, `thinking`, `output_config`, `metadata`, `tool_choice`) | exact body-string comparison in all differential scenarios |
| Status errors | `APIError.makeMessage` shape (`status message`, JSON body fallback, `status status code (no body)`) | `retries_retryable_statuses_and_reports_final_http_error`, 400/500 differential scenarios |
| Retry | SDK called with `maxRetries: 0`; `retryProviderRequest` policy with fresh headers per attempt | retry/retry-exhausted differential scenarios |

## Message conversion

| TypeScript helper/branch | Rust | Test |
|---|---|---|
| `normalizeToolCallId` | `normalize_tool_call_id_value` | `normalizes_tool_call_ids_in_request_messages` |
| `convertContentBlocks` (text join, images, placeholder) | `convert_content_blocks` | `images_are_sent_as_base64_blocks`, image differential scenarios |
| user string/block filtering | `push_user_message` | image and text differential scenarios |
| assistant text/thinking/redacted/tool_use | `convert_assistant_content` | `redacted_thinking_is_replayed_as_redacted_thinking`, empty-signature tests |
| consecutive tool results and sibling content | `convert_tool_result`, tool-result loop | tool-reference and image tool-result differential scenarios |
| deferred tool references | `collect_deferred_tool_references` | `tool_references_default_by_model_family`, tool-reference differential scenario |
| cache control on system/last user block | `get_cache_control`, cache-control tail | `cache_retention_controls_cache_control_ttl`, `provider_env_long_cache_retention_is_used_as_a_fallback`, retention scenarios |
| tool conversion (eager streaming, strict, defer loading, cache control) | `convert_tools` | `eager_tool_input_streaming_defaults_and_legacy_beta`, strict-tool differential scenario |
| thinking adaptive/budget/disabled | `build_params` | thinking-level, force-adaptive, disabled scenarios |
| metadata and tool choice | `build_params` | metadata/tool-choice/effort differential scenarios |
| OAuth Claude Code identity and tool-name mapping | `create_client_headers`, `to_claude_code_name`, `from_claude_code_name` | OAuth behavioral test and differential scenario |
| Copilot dynamic headers and Bearer auth | `create_client_headers` | Copilot behavioral test and differential scenario |
| beta headers | `create_client_headers` | eager/interleaved tests and scenarios |
| session affinity | `create_client_headers` | session-affinity differential scenario |
| `defaultSupportsToolReferences` | `default_supports_tool_references` | `tool_references_default_by_model_family` |
| `mapStopReason` (all branches) | `map_stop_reason` | `stop_reason_mapping_covers_provider_values`, refusal/sensitive tests |

## Stream state machine

| Branch | Rust | Test |
|---|---|---|
| `message_start` usage/response id/cost | `apply_message_start` | usage/cost tests and scenarios |
| `content_block_start` text/thinking/redacted/tool_use | `apply_content_block_start` | content-start, redacted, tool-call tests |
| `content_block_delta` text/thinking/signature/input_json | `apply_content_block_delta` | multi-block and malformed-JSON scenarios |
| `content_block_stop` end events and tool JSON finalization; remove `index` and tool `partialJson` before end snapshots | `apply_content_block_stop` | strict multi-block differential scenario |
| text/thinking/redacted/tool start and delta retain literal `index`; tool start/delta retain `partialJson` | content start/delta handlers | strict text and multi-block differential scenarios |
| error before block stop clears indices and tool JSON buffer before error/result snapshots | `run_stream` error branch | three open-block SSE error differential scenarios (text/thinking/tool, index 3) |
| `message_delta` stop reason, usage, reasoning tokens, cost | `apply_message_delta` | usage/reasoning and stop-reason tests |
| unknown SSE events skipped; `error` event throws data | `SseState`, `AnthropicEventReader` | `ignores_unknown_sse_events_after_message_stop` |
| stream ends without `message_stop` | `check_complete` | `truncated_stream_without_message_stop_is_an_error` |
| unparseable event context | `decode_event` | `unparseable_sse_event_reports_decoder_context` |
| abort before/mid stream | signal checks in `execute` and reader | `aborting_mid_stream_emits_an_aborted_error_event` and abort differential scenario |
| malformed SSE/streamed tool JSON repair | `parse_json_with_repair`, `parse_streaming_json` | `repairs_malformed_sse_json_and_streamed_tool_json` and differential scenario |

## Differential coverage

The oracle injects a recording/replaying `fetch` into the real TS module and
records the outgoing request, scripted HTTP response, every emitted event, and
the final message. The Rust side uses the shared `RecordingFetch`. Scenarios
cover: text, multiple blocks, thinking/signature/redacted thinking, adaptive
thinking for all seven levels, legacy budgets, thinking disabled (including an
explicit `off: null`), eager tool streaming (default/legacy/no tools), strict
tools, deferred tool references, usage/cost including 1h cache writes, all
cache-retention modes, images, 4xx/5xx errors, retry-after, retry exhaustion,
mid-stream abort, truncated streams, repairable malformed JSON, OAuth shaping,
Copilot headers, header-owned auth, metadata/tool choice/effort, session
affinity, empty signatures, temperature compat, and tool-call ID normalization.

Normalizations:

- `timestamp` is set to `0` in both event snapshots and the final message.
- No `index`/`partialJson` filtering remains in either oracle or Rust observation
  helpers. Prior 41-scenario normalized proof was **not strict event parity**;
  the current 44-scenario comparison preserves these valid output properties.
- Request bodies are compared as exact strings; events and the final message are
  compared as exact JSON values (the oracle transport uses `serde_json` with
  `float_roundtrip`, matching JavaScript decimal parsing).

## Discrepancies / explicit exclusions

1. **Bounded scratch proof.** `index` and `partialJson` now match this adapter's
   source start/delta/stop/error lifecycle. This does not prove other providers'
   scratch fields (`customInput`, `streamIndex`, `partialArgs`) or the custom
   Anthropic example's distinct SDK/error lifecycle. Shared types preserve that
   example's valid fields; its native producer proof belongs to the example slice.
   See [contract](../../specs/content-stream-scratch.md).
2. **`client` option.** A pre-built `Anthropic` SDK instance cannot be
   represented; the Rust API always constructs the private SDK request path.
3. **`streamSimple` auth failure.** TS throws synchronously; the Rust
   `ProviderStreams` contract returns a stream, so it returns an error stream.
4. **SDK identity headers.** `x-stainless-runtime` / `x-stainless-runtime-version`
   report the Rust runtime (`rust` / `unknown`) instead of Node; tests pin all
   host-derived identity headers through `model.headers` for exact comparison.
5. **Mapped effort strings.** TS casts `thinkingLevelMap` values to
   `AnthropicEffort` without validation. Rust accepts only the five declared
   values and otherwise falls back to the level mapping.
6. **Unparseable JSON error text.** `JSON.parse` messages are engine-specific;
   the Rust test asserts the decoder context prefix instead of the inner parser
   text. Repairable malformed JSON is compared exactly.
7. **URL normalization.** The Rust URL is a direct base-URL/path join; WHATWG
   normalization (default ports, dot segments) is not reproduced.
8. **`telemetryContext`.** `buildBaseOptions` copies it in TS; the Rust request
   contract has no telemetry field yet.
9. **Non-integer `maxRetries`.** TS decrements a fractional counter; Rust floors
   it to an integer (typed exclusion).

Q18 shared UTF-16 fields, argument parser and outbound-tree migration: [boundary/proof matrix](lossless-content.md). Checkpoint evidence does not establish unchanged consumer-fixture compatibility or full workspace parity.

## Q18 provider request integration (2026-10-22)

The shared `anthropic_sdk::Auth` variants and `SdkRequest` URL/header values are native. `execute` performs URL and ByteString conversion at `HttpRequest` construction and propagates conversion failures through the existing provider-request error path.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.

## Direct Simple acquisition boundary (2026-10-25)

`differential_simple_acquisition` invokes actual frozen modules and native ProviderStreams for missing/empty API keys across six APIs. All twelve cases were genuine RED: native returned a stream instead of rejecting acquisition. Direct stream_simple now returns Result and adapters propagate it; lazy acquisition continues through its existing catch. Mechanical consumer changes preserve valid-stream assertions. The remaining module audit is separate from this bounded proof.

## Native callback catch audit (2026-10-25)

`differential_api_catches` compares full actual-source/native error events and final messages for eight APIs × four rejection values: unmatched-UTF16 string, raw Error-lookalike object, and genuine Errors with status/body or statusCode/body. All callbacks reject before HTTP (source records zero requests). Genuine REDs were empty native messages for raw rejections and discarded branded status/body fields. Anthropic keeps its literal Error.message/JSON catch; Mistral probes only statusCode with its own prefix/body truncation; Google/OpenAI/Azure/images adapt the original branded fields to the existing shared normalizer. No utility algorithm or dependency changed. Logs: `/tmp/ai-api-catches-{red,red-2,images-red,green}.log`.
