# google-shared traceability

Source: `packages/ai/src/api/google-shared.ts`.
Rust: `crates/pi-ai/src/api/google_shared.rs`.
Tests: `crates/pi-ai/tests/google_shared.rs` (30 behavioral, ported from `google-thinking-signature.test.ts`, `google-shared-signed-empty-blocks.test.ts`, `google-shared-gemini3-unsigned-tool-call.test.ts`, `google-shared-image-tool-result-routing.test.ts`, `google-shared-convert-tools.test.ts`, `google-shared-retry.test.ts`), plus `crates/pi-ai/tests/differential_google_api.rs` (request bodies exercise `convertMessages`/`convertTools`).

## Representation
- `Part`/`Content` are request-side JSON records. `convert_messages` returns typed `GoogleContent { parts: Vec<Value>, role }`; parts are `serde_json::Value` because the Gemini part union has many provider-only members that Pi never sets.
- `GoogleThinkingLevel` is an enum with the exact Google enum wire values (`THINKING_LEVEL_UNSPECIFIED`, `MINIMAL`, `LOW`, `MEDIUM`, `HIGH`).
- `FunctionCallingConfigMode` is an enum with the exact Google enum wire values (`AUTO`, `NONE`, `ANY`, `VALIDATED`).
- `retryGoogleRequest` maps the SDK error shape (`status` without `headers`) onto the shared `utils::provider_retry` policy. `GoogleRequestError.status = None` means "not an SDK provider error" and is never retried, matching the TypeScript `"status" in error` guard.
- `convertMessages` takes an injected `now` for `transformMessages`' synthetic tool results (TypeScript uses `Date.now()`).
- `sanitizeSurrogates` is the crate identity function for valid Rust strings.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `GoogleThinkingLevel` | `GoogleThinkingLevel` | `thinking_levels_serialize_as_google_enum_values` |
| `isThinkingPart` | `is_thinking_part(thought: Option<bool>)` | `thinking_detection_requires_the_thought_flag` |
| `retainThoughtSignature` | `retain_thought_signature` | `thought_signatures_are_retained_until_a_new_non_empty_one_arrives` |
| `requiresToolCallId` | `requires_tool_call_id` | `tool_call_ids_are_required_for_gemini3_claude_and_gpt_oss` |
| `getGeminiMajorVersion` (private) | `gemini_major_version` | same (gemini-live and case variants) |
| `supportsMultimodalFunctionResponse` (private) | `supports_multimodal_function_response` | `gemini2_image_tool_results_use_a_separate_user_turn`, `gemini3_nests_image_tool_results_inside_the_function_response` |
| `convertMessages` | `convert_messages` | 15 behavioral tests + differential request bodies |
| tool-call-id normalization (`/[^a-zA-Z0-9_-]/g`, `slice(0, 64)`) | `normalize_tool_call_id` closure | `gemini3_history_preserves_tool_call_ids_...` |
| empty signed text/thinking retention | `convert_assistant_message` | `signed_empty_blocks_are_kept_...`, `unsigned_empty_blocks_are_dropped` |
| signature validity (base64, `len % 4 == 0`) | `is_valid_thought_signature` | `valid_signatures_survive_for_the_same_provider_and_model_only` |
| `sanitizeForOpenApi` | `sanitize_for_open_api` | `legacy_parameters_strip_json_schema_meta_keys_recursively`, `nested_meta_keys_are_stripped_and_refs_preserved` |
| `convertTools` | `convert_tools` | 5 convert-tools tests + differential `tools` request bodies |
| `supportsGoogleStrictToolSampling` | `supports_google_strict_tool_sampling` | `strict_tools_use_validated_mode_on_gemini3` |
| `mapToolChoice` | `map_tool_choice` | `tool_choice_maps_to_function_calling_modes` |
| `resolveGoogleFunctionCallingMode` | `resolve_google_function_calling_mode` | same |
| `mapStopReason` | `map_stop_reason` | `finish_reasons_map_to_stop_reasons` |
| `mapStopReasonString` | `map_stop_reason_string` | same |
| `retryGoogleRequest` | `retry_google_request` | `retry_google_request_*` (3 tests) |

## Branch notes
- Empty text/thinking blocks are dropped only when unsigned; a signed empty block is kept so Gemini can echo the signature back.
- Cross-provider/model thinking becomes plain text with no `thought` flag and no signature.
- Tool results merge into the preceding user turn only when that turn already contains a `functionResponse` part.
- Gemini < 3 gets a separate `Tool result image:` user turn; Gemini 3+ nests image parts inside `functionResponse.parts`.
- `sanitizeForOpenApi` does not recurse into arrays (TypeScript returns arrays unchanged); meta keys inside array elements survive.
- `retryGoogleRequest` retries 408/409/429/5xx and honours `Retry-After`/`Retry-After-Ms`; non-SDK errors (no `status`) are never retried.

## Gaps
- `sanitizeSurrogates` cannot observe unpaired surrogates because Rust strings are valid Unicode; the UTF-16 variant exists in `utils::sanitize_unicode`.
- JavaScript `maxRetries` is a number; Rust truncates a fractional value to `usize` (typed core excludes fractional retry counts).

## Discrepancies
- None.
