# mistral-conversations.ts

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
| timeout (default 60000 ms) and user abort combined signal | `RequestSignal` | abort and timeout tests, differential abort scenario |
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
