# pi-messages.ts

Source: `packages/ai/src/api/pi-messages.ts` (pinned revision).
Rust: `crates/pi-ai/src/api/pi_messages.rs`.
Tests: `crates/pi-ai/tests/pi_messages.rs`,
`crates/pi-ai/tests/differential_pi_messages.rs`.

## Exports

| TypeScript | Rust | Tests |
| --- | --- | --- |
| `PiMessagesOptions` (`reasoning`, `toolChoice`, `debug`) | `PiMessagesOptions`, `PiMessagesToolChoice`; `from_stream_options` parses `StreamOptions::extra` | request tests, differential scenarios 0/11/12 |
| `PiMessagesRewriteImpact` | `PiMessagesRewriteImpact` | rewrite-diagnostic test, differential scenarios 2/3 |
| `PiMessagesEvent` | `PiMessagesEvent` (tagged enum, `toolcall_*` wire tags) | all behavioral tests, differential |
| `PiMessagesResponseError` | `PiMessagesResponseError` | backend-error test, differential scenarios 4/5 |
| `stream` | `stream` + `PiMessagesApi::stream` (`ProviderStreams`) | behavioral tests, differential 13 scenarios |
| `streamSimple` | `stream_simple` + `PiMessagesApi::stream_simple` | debug/onResponse test, differential scenario 11 |
| `piMessagesApi` (lazy wrapper) | `pi_messages_api()` returning `LazyApi` | compile-time `ProviderStreams` use |

## Branches

| Source branch | Rust | Tests |
| --- | --- | --- |
| missing/empty API key | `produce_stream` key check | `errors_when_no_api_key_is_provided` |
| URL `<baseUrl>/messages` + `?debug=1` | `messages_url` | request assertions, differential scenario 8 |
| payload `{model, context, options}` with undefined omission | `build_payload`, `build_payload_options` | request test, differential |
| `resolveCacheRetention` (`option`, `PI_CACHE_RETENTION=long`) | `resolve_cache_retention` | differential scenarios 12/13 |
| `onPayload` replacement | `on_payload` callback | differential scenario 9 |
| header assembly + `providerHeadersToRecord` null filtering | `build_headers` | request test, differential scenario 12 |
| `onResponse` | `on_response` callback | debug/onResponse test |
| non-2xx response error (message, code, diagnostics) | `create_response_error` | backend-error test, differential scenarios 4/5 |
| missing body | not representable: `HttpResponse::body` is mandatory | explicit gap |
| SSE `\r\n` normalization, `\n\n` framing, `data:` line, `[DONE]` | `produce_stream` loop, `parse_event` | all behavioral tests, differential bytewise scenario |
| incremental UTF-8 decoding | `Utf8StreamDecoder` | differential bytewise scenario |
| terminal `done`/`error` handling | `EventConverter::convert` | text/tool test, server error test |
| rewrite diagnostic append | `append_rewrite_diagnostic` | rewrite test, differential 2/3 |
| response-failure diagnostic append | `create_error_event` | backend-error test |
| no terminal event | `produce_stream` end check | end-without-terminal test, differential 6 |
| abort signal | `cancelled` select + `create_error_event` | abort test |
| text/thinking/tool-call conversion (start/delta/end, streaming JSON) | `EventConverter` methods | text/tool test, thinking differential scenario 1 |
| content index assignment | `set_block` (ordered indices) | differential scenario 0 |

## Differential normalizations

- `timestamp`/`timestampMs` → `0` (clock-derived).
- `stack` removed (environment-derived).
- object key order sorted (TypeScript property order follows mutation history).
- JSON parser error text for malformed `data:` payloads → `<json-parse-error>`.
- integral numbers canonicalized to integers.
- extra fields the converter spreads into events but that are not part of
  `AssistantMessageEvent` are stripped: `contentSignature`/`redacted` on
  `text_end`/`thinking_end`, `id`/`toolName` on `toolcall_start`. The same values
  remain on the partial message blocks.

## Gaps / discrepancies

- `HttpResponse` has no optional body, so the "response has no body" branch is
  unrepresentable.
- `HttpResponse` gained `status_text` for `Response.statusText`; the reqwest
  default fetch uses the canonical reason phrase.
- TypeScript assigns content blocks by array index, which can create sparse
  arrays; Rust rejects an out-of-order start index instead of representing holes.
- `Error.stack` is not representable.
- `truncateDiagnosticString` counts UTF-16 code units in TypeScript; Rust counts
  characters (8192) because lone surrogates cannot be represented.
- `ToolCall.arguments` is a JSON object; non-object streaming JSON becomes `{}`.
- The emitted `text_end`/`thinking_end`/`toolcall_start` events carry only the
  declared `AssistantMessageEvent` fields; TypeScript's spread also carries the
  backend event's extra fields (see normalizations).
