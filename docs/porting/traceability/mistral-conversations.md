# mistral-conversations.ts

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
| error formatting and 4000-char truncation | `format_mistral_error`, `truncate_error_text` | HTTP error test |

## Differential normalizations

- `timestamp`/`timestampMs` → `0` (clock-derived).
- `partialArgs` removed (JS streaming scratch, deleted before persistence).
- object key order sorted (TypeScript property order follows `Object.assign` mutation history).
- JSON parser error text for malformed `data:` payloads → `<json-parse-error>` (runtime-specific wording).
- Integral numbers canonicalized to integers (JS `200` vs Rust `200.0` representation).

## Gaps / discrepancies

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
