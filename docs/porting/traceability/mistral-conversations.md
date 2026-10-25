# mistral-conversations.ts

## Reason-bearing Request and SDK failures

Common-pin provider differential `differential_provider_reasons` now exercises actual Mistral SDK send/body rejection boundaries. `RequestSignal` follows WHATWG Request dependent-signal semantics: synchronous native reason forwarding from a supplied caller, no competing deadline in that branch; only an absent caller gets the SDK's default AbortSignal.timeout(60000). Source ignores provider timeoutMs; the explicit5ms option test observes source60000ms and native60s virtual elapsed. Retained fetch signals still forward caller abort after provider completion.

SDK rejection classification matches structural name/code AbortError/TimeoutError/econnaborted checks, real TypeError constructor checks for fetch-failure prefixes, ConnectionError/econnreset, and unexpected errors. Its truthy cause suffix uses native String conversion, including conversion failure; body catches use Error.message or JSON.stringify(raw value). No class is inferred from a mutable name: [shared class proof](native-error-classes.md).

`google-mistral-reasons-red-2.log`, `mistral-typeerror-class-red.log` → `google-mistral-reasons-expanded.log`:27 Mistral cases plus default deadline GREEN (paired Google27 cases). Checks compare final stopReason/errorMessage and retained request-signal reason, not full new event matrices. Nested SDK validation and full-module completion remain partial.

## Q4 common-pin status

Partial. Global-fetch correction now drives the frozen Mistral SDK, including Request URL/method/body capture. Scenario 0's model-less SSE is rejected by frozen SDK `data.model` validation but accepted by native code; Restored exact SDK User-Agent `mistral-client-typescript/2.2.6`, default empty Cookie header and source's absent onResponse call. Whole scenario0 now matches request/onResponse and remains RED on SDK SSE validation (`mistral-sdk-headers-green.log`). Added native frozen CompletionChunk top-level validation (SDK `completionchunk.ts:29–36`): required id/model/choices, optional object and safe-integer created, issue order and exact Zod error formatting. Existing original model-less scenarios0–16 now match source rejection without changing fixtures. Five additional whole-provider header error cases GREEN, including fractional and unsafe created (`mistral-chunk-header-matrix-{red,green-1,green-2}.log`). Original scenario22 empty HTTP500 formatting now matches SDKError's status/content-type/empty-body message (`sdkerror.ts:19–39`); captured error text compares exactly, no replacement. Header validation helpers split by issue/type/range for readability; five header cases remain GREEN. Ported scenario31 malformed JSON Zod transform wrapper (custom issue plus piped required-field issues) and choice-required finish_reason invalid_union after scenario32 RED; both now compare exactly. Scenario40 then hung: oracle fetch aborted its signal but returned an unclosed fake body ignoring cancellation, unlike native fetch. Corrected the oracle to close the fake body and reject with request.signal.reason before returning the response. Bounded90s instrumentation established scenario40, terminated all children and verified none remained after the first unexpected10-minute detached test. Matrix now terminates and exposed native abort-start ordering/error wrapping RED (`mistral-abort-fetch-boundary-red.log`). Native fixture also previously cancelled during body.next rather than before fetch resolved; replaced only that boundary with AbortBeforeResponse recorder, then ported SDK request-abort wrapper. Scenario40 now compares exactly with no start and `Request aborted by client: AbortError: This operation was aborted`; matrix advanced to SDK request key-order/strict flag RED50 (`mistral-abort-before-start-green-2.log`). Restored frozen function strict:false with unchanged parameters and actual SDK schema field order (chatcompletionstreamrequest357–416, assistantmessage95–107, function31–42); whole request bytes now compare in50. RED51 case-sensitive source header merging followed by Headers comma-join/null stringification (`mistral-sdk-order-green-1.log`) is restored: merge original header keys first, choose affinity on literal lowercase nullable value, then construct normalized SDK headers; custom UA is overwritten by the frozen hook. Original43 scenarios GREEN (`mistral-header-merge-green-2.log`); these original streaming fixtures lack required model and prove validation/request/error paths, not valid streaming. Added four separate valid SDK event fixtures with required model, keeping all original invalid fixtures unchanged. Actual valid text case0 exposed source-absent rawStopReason; removed it and derived source-absent error reasons. Frozen unknown finish reason maps stop, error maps generic unknown error. Valid text case now GREEN; valid tool case1 exposes missing observable partialArgs (`mistral-valid-sdk-events-red.log`, `mistral-valid-stop-fields-green-1.log`). Native ToolCall.partial_args now owns the literal buffer; removed private StreamState duplicate. Start/delta snapshots retain it, finalization parses/removes it, and owner catch removes it before failure/result. Four valid complete/open-ended text/tool source scenarios GREEN (`mistral-valid-scratch-green-1.log`); added a fifth tool-then-malformed case to prove error cleanup. Original matrix proves mostly invalid-input paths; these new valid events are additional independent fixtures, not a fixture repair or waiver. Current differential target3 GREEN:43 original request/error cases,5 header-error cases and5 separate valid event cases, including tool-then-malformed cleanup (`mistral-scratch-behavior-matrix.log`).16 behavioral cases now GREEN (`mistral-unit-migration-green.log`): intended valid event fixtures explicitly include required model; raw-stop/strict/header/callback/EOF assertions now assert literal frozen behavior. Original invalid oracle scenarios remain unchanged. Nested SDK schema/default/unknown-key branches and broader old-only API cleanup remain open; no whole-module completion claim. This is an oracle fetch repair, not source/error masking. Nested choice/delta/content schema validation/default/unknown-key handling still pending; top-level validation is not whole-SDK proof. Full assertions retained. [Current audit/proof](../ai-scripted-provider-oracles.md); historical differential claims below are superseded. `request.fetch` is an approved Rust-only seam; unset must match frozen default fetch (`AGENTS.md`).

Q18 human-error representation and bounded proof: [lossless provider errors](lossless-provider-errors.md). Earlier counts are historical, not rerun evidence.

Q18 ingress representation and current bounded proof: [lossless provider ingress](lossless-provider-ingress.md). Earlier suite counts below are historical, not new-pin rerun evidence.

Source: `packages/ai/src/api/mistral-conversations.ts` (pinned revision).
Rust: `crates/pi-ai/src/api/mistral_conversations.rs`.
Tests: `crates/pi-ai/tests/mistral_conversations.rs`,
`crates/pi-ai/tests/differential_mistral_conversations.rs`.

## Exports

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `MistralOptions` (`toolChoice`, `promptMode`, `reasoningEffort`) | `MistralOptions`, `MistralToolChoice`, `MistralPromptMode`, `MistralReasoningEffort`; `MistralOptions::from_stream_options` parses `StreamOptions::extra` | serialization, reasoning, differential request scenarios |
| `stream` | `stream` + `MistralConversationsApi::stream` (`ProviderStreams`) | behavioral tests, differential 43 scenarios |
| `streamSimple` | `stream_simple` + `MistralConversationsApi::stream_simple` | reasoning-mode tests, differential thinking-level matrix |
| `mistralConversationsApi` (lazy wrapper) | `mistral_conversations_api()` returning `LazyApi` | API-lazy coverage plus compile-time `ProviderStreams` use |

## Branches

| Source branch | Rust | Tests |
| --- | --- | --- |
| missing/empty API key | `required_api_key` | `errors_when_no_api_key_is_provided` |
| tool-call ID normalization (`deriveMistralToolCallId`, collision retry, 9-char rule) | `MistralToolCallIdNormalizer`, `derive_mistral_tool_call_id` | `normalizes_invalid_tool_call_ids_for_replay`, differential scenario 50 |
| `onPayload` replacement | `on_payload` callback | serialization test, differential scenario 50 |
| URL construction (`baseUrl` trailing slashes + `v1/chat/completions`) | `chat_completions_url` | request URL assertions, all differential scenarios |
| header defaults, model/request overrides, `null` suppression, automatic `x-affinity` | `build_mistral_headers`, `apply_model_header_overrides`, `apply_mistral_header_overrides` | header tests, differential scenario 51 |
| default60000ms only without caller; supplied Request signal and reason | `RequestSignal` | `differential_provider_reasons`, native elapsed deadline + actual source delay assertion |
| `onResponse` status/headers | `on_response` callback | serialization test |
| non-2xx `MistralHttpError` status/body/statusText | `MistralError::Http` | HTTP error test, differential 403/429/500 |
| missing response body | not representable: `HttpResponse::body` is mandatory | explicit gap |
| SSE boundary regex (9 CR/LF forms) | `find_mistral_event_boundary` | bytewise split test, differential bytewise scenario |
| `data:` line join/trim, `[DONE]`, invalid event | `parse_mistral_event`, `ParsedEvent` | malformed and end-without-finish tests, differential 30-32 |
| incremental UTF-8 decoding | `Utf8StreamDecoder` | bytewise split test |
| payload build (`model`, `stream`, messages, tools, sampling, reasoning, cache key, system prompt) | `build_chat_payload`, `to_chat_messages`, `to_function_tools` | serialization tests, differential scenario 50 |
| strict tool schema resolution | `resolve_json_schema_strict_sampling` + `get_json_schema_tool_parameters` | strict-schema test |
| camelCase → snake_case wire remapping (payload, messages, chunks, nested response format) | `to_mistral_wire_payload`, `to_mistral_wire_message`, `to_mistral_wire_content_chunk` | serialization test, differential request comparison |
| usage/cached-token extraction and cost | `update_usage`, `cached_prompt_tokens`, `calculate_cost` | native-parsing test, differential usage scenarios |
| response ID (`||=`) | `update_response_id` | native-parsing test, differential |
| finish-reason mapping (`stop`, `length`/`model_length`, `tool_calls`, `error`, unknown) | `apply_finish_reason` | raw-stop-reason test, differential finish matrix |
| text/thinking/tool-call delta accumulation and event emission | `consume_content_delta`, `append_text_delta`, `append_thinking_delta`, `consume_tool_call_delta`, `finish_current_block`, `finalize_tool_blocks` | native-parsing test, differential events |
| terminal checks (aborted, pending, error, done) | `produce_stream` | end-without-finish test, differential |
| delayed tool-call consumer, same call ID across two SSE chunks | `stream`, queued `next` | approved snapshot deviation below; push-time proof: `mistral_stream_matches_the_pinned_typescript_module` |
| error formatting and 4000-char truncation | `format_mistral_error`, `truncate_error_text` | HTTP error test |

## Differential normalizations

- `timestamp`/`timestampMs` → `0` (clock-derived).
- Push-time oracle removes `partialArgs` (JS scratch). Rust stores it in slot state.
- Object key order is preserved by the oracle and the Rust `canonical` pass; request
  bodies are compared as the exact captured `JSON.stringify` text. `remap_mistral_property`
  uses `shift_remove` because TypeScript `delete` preserves the remaining insertion order
  (serde_json `remove` is `swap_remove`).
- JSON parser error text for malformed `data:` payloads is the V8 `SyntaxError` message from `pi_js_compat::js_json` and is compared byte-for-byte.
- Integral numbers canonicalized to integers (JS `200` vs Rust `200.0` representation).

## Gaps / discrepancies

### Approved delayed-consumer snapshot deviation (user, 2026-10-20)

Historical Phase F1 probes compared full event sequences for one
`abc123456` / `lookup` call with `{"query":` and `"pi"}` in distinct SSE chunks.
Both sides emit `start`, `toolcall_start`, two `toolcall_delta`, `toolcall_end`, `done`.
Types, content indexes, deltas, final `toolCall` and final message agree; partials do not:

- TS `start.partial.content` already contains the completed tool call with
  `arguments: {"query":"pi"}`. Rust's queued `start.partial.content` is empty.
- TS start/first-delta tool arguments are final; Rust retains `{}`.
- TS early partials carry final `stopReason: "toolUse"`, `responseId: "delayed"`
  and `rawStopReason: "tool_calls"`; Rust early clones retain `pending` and lack
  metadata not assigned at push time.
- `partialArgs`, `partialJson`, `customInput`, `streamIndex` and `index` are absent
  from consumed tool blocks on both sides without filtering. TS deletes scratch
  in place before completion, also affecting previously queued references.

Approved deviation (user, 2026-10-20): “Approve snapshot deviation.” TS delayed
consumers observe aliased mutable partials; Rust delivers immutable push-time
snapshots identical to normalized TS push-time events, proven by
`mistral_stream_matches_the_pinned_typescript_module`. JS reference aliasing is
not reproduced. Production semantics remain unchanged; intentionally failing
probes and their exclusive support code were removed.

Evidence: `/tmp/pi-rust-streaming-consumer-proof/02-targeted-red.log` preserves
full TS/Rust sequences: OpenAI function 4 events, OpenAI custom 5 events,
Mistral 6 events. Historical run: 4 existing tests passed, 3 delayed probes failed.
Scratch fields `partialJson`, `customInput`, and `partialArgs` are absent on both
sides without filtering at delayed consumption; scratch deletion is not the divergence.

### Other gaps

- `HttpResponse` has no optional body, so the "response has no body" branch is unrepresentable;
  the default fetch always supplies a body stream.
- `HttpResponse` gained a `status_text` field to carry `Response.statusText`; the reqwest default
  fetch fills it with the canonical reason phrase because reqwest does not expose a custom one.
- `streamSimple` missing-key throws synchronously in TypeScript; `ProviderStreams` cannot throw, so
  the Rust function returns an error stream carrying the same message.
- `Error.stack` is not representable; the oracle strips stacks.
- Error-body truncation counts UTF-16 code units in TypeScript; Rust truncates at the last whole
  character within the same code-unit budget (lone-surrogate splits are unrepresentable).
- `MistralReasoningEffort` is a two-value enum (`none`, `high`); a `thinkingLevelMap` value other
  than `none` maps to `high` (TypeScript casts arbitrary mapped strings).
- `ToolCall.arguments` is a JSON object in Rust; a non-object parsed streaming value becomes `{}`.

## Q18 provider request integration (2026-10-22)

Mistral bearer/auth/session-affinity and model/options header assembly retain native units; HttpRequest construction applies ByteString conversion. Endpoint parsing uses shared request_url at the URL boundary.

[Focused public-provider runtime proof](lossless-sdk-boundaries.md) records exact covered cases, revisions and remaining acceptance limits.

## Direct Simple acquisition boundary (2026-10-25)

`differential_simple_acquisition` invokes actual frozen modules and native ProviderStreams for missing/empty API keys across six APIs. All twelve cases were genuine RED: native returned a stream instead of rejecting acquisition. Direct stream_simple now returns Result and adapters propagate it; lazy acquisition continues through its existing catch. Mechanical consumer changes preserve valid-stream assertions. The remaining module audit is separate from this bounded proof.

## Native callback catch audit (2026-10-25)

`differential_api_catches` compares full actual-source/native error events and final messages for eight APIs × four rejection values: unmatched-UTF16 string, raw Error-lookalike object, and genuine Errors with status/body or statusCode/body. All callbacks reject before HTTP (source records zero requests). Genuine REDs were empty native messages for raw rejections and discarded branded status/body fields. Anthropic keeps its literal Error.message/JSON catch; Mistral probes only statusCode with its own prefix/body truncation; Google/OpenAI/Azure/images adapt the original branded fields to the existing shared normalizer. No utility algorithm or dependency changed. Logs: `/tmp/ai-api-catches-{red,red-2,images-red,green}.log`.
