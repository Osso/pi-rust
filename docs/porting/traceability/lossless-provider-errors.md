# Human provider errors — Q18

Source: immutable common pin in [source lock](../source-lock.json).
Scope: human error text only; existing ID/name/type/code metadata policy remains pending approval and unchanged. Source Mistral text/thinking sanitizer remains untouched. Nonstream image payload ingress remains outside this slice.

## Native API

`AssistantMessage.error_message`, `AssistantImages.error_message`, `CallbackError.message`, SDK/provider error messages and bodies, retry-limit provider messages, diagnostic messages and thrown text use `JsString`. Provider inner-error payloads use `JsValue`. Error formatters return `JsString`; formatting prefixes concatenate UTF16 units. Callback/fetch and retry callbacks carry native error text. Coupled OAuth/auth, image and other-provider callback consumers use the same native contract; actual Rust Display and rendered OAuth HTML are UTF8 display boundaries.

## Proof matrix

| Boundary | Behavioral proof | State |
|---|---|---|
| SDK SSE error → completions/Responses/Azure → Error event + final result | `lossless_provider_errors::sse_human_error_is_persistent_and_final_exact_utf16` | Initial RED: scalar SDK inner-error projection replaced the provider message with a parse error |
| Codex SSE error → native API error + diagnostics | Same loopback test | PASS at `0db07504`; expanded parser cases pending |
| Codex HTTP human errors, raw SSE/WS parser excerpts, Mistral HTTP UTF16 truncation | `codex_http_error_message_keeps_unmatched_units`, `sse_parser_failure_keeps_split_surrogate_excerpt`, `codex_websocket_error_and_parser_failure_keep_native_units`, `mistral_http_body_truncation_splits_exact_utf16_unit` | Behavioral RED established before these fixes; GREEN pending |
| Callback, retry/overflow | Focused cases to follow | Open |

No whole-provider parity or broad verification claimed. Final proof ledger will be recorded in `/tmp/pi-rust-repin-lossless-provider-errors/REPORT.md`.
