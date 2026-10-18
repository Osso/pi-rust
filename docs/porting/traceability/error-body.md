# utils/error-body traceability

Pinned source: `packages/ai/src/utils/error-body.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `MAX_PROVIDER_ERROR_BODY_CHARS` | `4000`. | `error_body::MAX_PROVIDER_ERROR_BODY_CHARS`; `truncates_the_body_at_the_cap`. |
| `normalizeProviderError(error)` | Non-`Error` → JSON string message, no body. Error → first numeric status in `statusCode`, `status`, `$metadata.httpStatusCode`, `$response.statusCode`; body from `body` text, plain non-empty `error` object, or `$response.body`; trimmed, empty → absent, truncated; `messageCarriesBody` when no body or message contains it. | `normalize_provider_error` over `ProviderErrorInput`; `extracts_status_and_body_from_a_mistral_shaped_error`, `reads_the_parsed_body_off_an_openai_shaped_error`, `preserves_a_message_that_already_carries_the_body`, `extracts_status_and_body_from_a_bedrock_shaped_error`, `ignores_non_object_response_bodies`, `ignores_class_like_error_fields_and_empty_objects`, `still_surfaces_a_plain_parsed_json_body_object`, `json_stringifies_a_non_error_thrown_value`, `truncates_the_body_at_the_cap`, `sets_message_carries_body_when_the_message_contains_the_extracted_body`. |
| `formatProviderError(norm, prefix?)` | Body carried/absent status/body → message (with `prefix (status): ` when a prefix and status exist); otherwise `status: body` or `prefix (status): body`. | `format_provider_error`; `formats_status_and_body_with_and_without_a_prefix`, `formats_a_message_that_carries_the_body_with_prefix_and_status`, `returns_the_bare_message_for_a_non_error_value`. |
| `truncateErrorText(text, maxChars)` | Uses JavaScript `text.length` (UTF-16 units) and appends `... [truncated N chars]`. | `truncate_error_text` counts UTF-16 units; `truncates_by_utf16_length_and_reports_the_removed_count`. |
| `safeJsonStringify(value)` | `JSON.stringify`, falling back to `String(value)`; `undefined` → `"undefined"`. | `safe_json_stringify` over `serde_json::Value`; `safe_json_stringify_matches_javascript_output`. |
| Differential oracle | 16 Error shapes (precedence, status 0/429.5, empty/whitespace/padded/4050-char bodies, arrays, empty objects), 7 non-Error values, 9 truncation cases. | `tests/differential_error_body.rs` against `oracle/error-body-oracle.ts`. |

## Exclusions and deviations

- Stream/class-instance `$response.body` values are not representable in the typed core API; `ErrorBody` accepts text or already-parsed JSON. Non-object JSON bodies are ignored, matching the plain-object rule.
- Truncating in the middle of a surrogate pair cannot produce JavaScript's lone surrogate in Rust; the prefix stops at the preceding character boundary while the removed-count suffix keeps the UTF-16 arithmetic.
- `undefined`, symbols, and circular objects are not representable; `serde_json::Value` covers the JSON-shaped non-Error throws.
- No source bug found.
