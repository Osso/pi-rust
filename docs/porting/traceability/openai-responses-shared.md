# openai-responses-shared traceability

## Q4 partial re-port

Recorded Responses/Azure oracles now reach actual frozen SDK streams. Removed source-absent rawStopReason metadata; restored input_tokens-minus-cached usage with cacheWrite0 (no native cache-write subtraction/clamp), and function-only tools with unchanged parameters/explicit strict option. RED boundaries: `responses-floor-green.log` and `responses-raw-stop-green.log` in the lane proof ledger. Whole-module parity remains unproven; historical counts below are superseded where these branches differ.

DISCREPANCY preserved: frozen empty tool results without images still emit `(see attached image)` (`openai-responses-shared.ts:407`). Native's `(no tool output)` was an unapproved source correction; exact conversion scenario3 RED restored the literal frozen placeholder. Stream fixtures now initialize stop, not old pending.

Removed old-only custom-tool/namespace replay and streaming slots, custom-input deltas/done events, and deferred additional_tools/tool_search emissions after exact conversion9/10 and stream2/3 REDs (`responses-shared-{placeholder,replay}-green.log`, diagnostic names retain earlier RED outputs). Public obsolete option fields and producer-side validation remain pending wider type cleanup. Removed source-absent final-response encrypted-reasoning backfill and its tracking map after exact stream scenario11 RED (`responses-shared-custom-deferred-green.log`).

Q18 human-error representation and bounded proof: [lossless provider errors](lossless-provider-errors.md). Earlier counts are historical, not rerun evidence.

Q18 ingress representation and current bounded proof: [lossless provider ingress](lossless-provider-ingress.md). Earlier suite counts below are historical, not new-pin rerun evidence.

Source: `packages/ai/src/api/openai-responses-shared.ts`.
Rust: `crates/pi-ai/src/api/openai_responses_shared.rs`.
Tests: `crates/pi-ai/tests/openai_responses_shared.rs` (7 behavioral), `crates/pi-ai/tests/differential_openai_responses_shared.rs` (3 differential suites: 12 conversion scenarios, 7 tool scenarios, 12 push-time stream scenarios) against the pinned TS module.

Current bounded proof at9f327aad: all3 shared differentials (conversion12, tools7, stream12 scenarios) and7 behavioral tests pass; Responses20 and Azure10 recorded SDK scenarios pass in the same invocation (`responses-shared-current-green.log`). No assertion normalization was added to waive these removals. Obsolete option/API fields and producer validation remain open; no whole-module completion claim.

## Representation
- Request bodies and SSE events are `JsValue`; this preserves TypeScript key order and omitted-vs-null distinctions without inventing wire structs for the OpenAI SDK types.
- `ResponsesEventSource` abstracts the event iterator; production uses the SDK SSE decoder, tests use `VecResponsesEventSource`.
- `ResponsesStreamFailure<E>` separates event-source failures (SDK errors) from state-machine rejections (plain `Error` messages), matching TypeScript's `throw` propagation.
- `TextSignatureV1`/`TextSignatureVersion` from `types.rs` encode and parse the message signature.
- Streaming scratch fields (`partialJson`, `customInput`, `streamIndex`, `partialArgs`) are typed slot state, not fields on `ToolCall`; they are never persisted.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `encodeTextSignatureV1` / `parseTextSignature` | `encode_text_signature_v1` / `parse_text_signature` | stream scenarios 0-2; differential |
| `convertToolResultOutput` | `convert_tool_result_output` | conversion 3,7,8; `empty_tool_result_uses_no_tool_output_placeholder` |
| citation constants + `isRawHostedSearchCitation` | `RAW_*`, `is_raw_hosted_search_citation`, `is_citation_reference` | unit tests; stream scenario 4 |
| `getCitationBoundarySeparator` | `get_citation_boundary_separator` | stream scenario 4; split-delta unit test |
| `appendVisibleCitationText` | `append_visible_citation_text` | unit tests; stream scenario 4 |
| `findTrailingCitationBufferStart` | `find_trailing_citation_buffer_start` | split-delta unit test |
| `normalizeClosedCitationMarkers` | `normalize_closed_citation_markers` | unit tests |
| `normalizeCitationTextDelta` / `finishCitationText` / `normalizeCitationText` | same-named snake_case functions | unit tests; stream scenario 4 |
| `OpenAIResponsesStreamOptions` | `OpenAIResponsesStreamOptions` | stream scenarios; service-tier callbacks exercised by `openai-responses` tests |
| `ConvertResponsesMessagesOptions` / `ConvertResponsesToolsOptions` | same-named structs | conversion/tool scenarios |
| `convertResponsesMessages` | `convert_responses_messages` | conversion 0-11; behavioral tests |
| system role (`developer`/`system`) | `convert_responses_messages` | conversion 0 vs 1 |
| provider-native input insertion | `convert_responses_messages` | conversion 6; behavioral test |
| tool-call ID normalization (`normalizeIdPart`, `buildForeignResponsesItemId`, pipe split) | `normalize_id_part`, `build_foreign_responses_item_id`, `split_tool_call_id` | conversion 3-4; behavioral test |
| message IDs/fallbacks/`shortHash` | text-block branch | conversion 2; behavioral test |
| custom-tool replay + namespace gating | tool-call branch | conversion 9; stream scenario 3 |
| deferred tools (`additional-tools`, `tool-search`) | tool-result branch | conversion 10-11 |
| `convertResponsesTools` | `convert_responses_tools` | tool scenarios 0-6 |
| `processResponsesStream` | `process_responses_stream` | stream scenarios 0-11; behavioral tests |
| reasoning slots and signature backfill | `Slot::Thinking`, `backfill_reasoning_signatures` | stream 1, 11 |
| text slots and citation streaming | `Slot::Text`, `append_provider_text_delta` | stream 0, 4 |
| function/custom tool slots | `Slot::ToolCall` | stream 2-3 |
| delayed function/custom-tool consumers | `process_responses_stream`, queued `EventStream::next` | approved snapshot deviation below; push-time proof: `differential_openai_responses_shared_stream`, scenarios 2-3 |
| usage/cost and service-tier resolution | `finalize_response` | stream 0; service-tier pricing in `openai-responses` |
| stop-reason mapping | `map_stop_reason` | stream 0, 6, 9 |
| terminal-event requirement | end-of-loop check | stream 8; behavioral test |
| `response.failed` / `error` messages | event arms | stream 7 |

## Differential normalizations
- Event `partial` payloads are snapshotted by the oracle at push time. TypeScript events carry a live reference; Rust events carry a value snapshot. This proves normalized push-time payloads, not consumer-time parity.
- The oracle strips TypeScript streaming scratch keys (`partialJson`, `customInput`, `streamIndex`, `partialArgs`) from event snapshots. Rust keeps them in slot state and never exposes them.
- Numbers compare as `f64` values (`float_roundtrip` parsing), not lexical forms.
- Timestamps are supplied by the test fixture, so no clock normalization is needed.

## Gaps
- `applyServiceTierPricing`/`resolveServiceTier` callbacks cannot be expressed in the JSON oracle; Rust behavior is covered in `openai-responses` behavioral tests.
- TypeScript untyped escape hatches (`as any`, deleting scratch fields) have no Rust equivalent by design.

## Discrepancies
- Phase F1 (2026-10-20): delayed-consumer parity is disproven for both valid-output fixtures. Event types, content indexes, deltas and `toolCall` payloads agree, but `partial` messages do not. Every TS partial observes final content and terminal metadata through the same mutable message reference. Rust retains each push-time clone.
  - Function call: TS `toolcall_start.partial.content[0].arguments` is `{"path":"README.md"}`; Rust is `{}`. The first argument delta likewise differs.
  - Custom tool: TS start arguments are `{"input":"hello"}`; Rust is `{"input":""}`. The first delta has Rust `{"input":"he"}` while TS already has `hello`.
  - All TS partials include `responseId: "resp_delayed"`, `rawStopReason: "completed"`, and final `stopReason: "toolUse"`. Rust queued partials omit those response fields and retain `stopReason: "pending"`, including `toolcall_end` emitted before response completion.
  - Neither side exposes `partialJson`, `customInput`, `partialArgs`, `streamIndex`, or `index` on the consumed tool block; delayed mode performs no filtering. Scratch deletion is not the divergence.
- Approved deviation (user, 2026-10-20): “Approve snapshot deviation.” TS delayed consumers observe aliased mutable partials; Rust delivers immutable push-time snapshots identical to normalized TS push-time events, proven by `differential_openai_responses_shared_stream` (function/custom-tool scenarios 2-3). JS reference aliasing is not reproduced. Production semantics remain unchanged; the intentionally failing delayed-consumer probes and their exclusive support code were removed.
- Evidence: `/tmp/pi-rust-streaming-consumer-proof/02-targeted-red.log` preserves full TS/Rust sequences: OpenAI function 4 events, OpenAI custom 5 events, Mistral 6 events. The historical run passed 4 existing tests and failed 3 delayed-consumer probes. Scratch fields `partialJson`, `customInput`, and `partialArgs` are absent on both sides without filtering at delayed consumption; scratch deletion is not the divergence.
- `mapStopReason`'s `never` branch throws `Unhandled stop reason: <status>`; Rust returns the same message as an error.
