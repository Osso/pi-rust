# openai-responses-shared traceability

Source: `packages/ai/src/api/openai-responses-shared.ts`.
Rust: `crates/pi-ai/src/api/openai_responses_shared.rs`.
Tests: `crates/pi-ai/tests/openai_responses_shared.rs` (7 behavioral), `crates/pi-ai/tests/differential_openai_responses_shared.rs` (3 passing differential suites: 12 conversion scenarios, 7 tool scenarios, 12 push-time stream scenarios; 2 failing delayed-consumer probes) against the pinned TS module.

## Representation
- Request bodies and SSE events are `serde_json::Value`; this preserves TypeScript key order and omitted-vs-null distinctions without inventing wire structs for the OpenAI SDK types.
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
| delayed function-call consumer, arguments split across two deltas | `process_responses_stream`, queued `EventStream::next` | `delayed_consumer_function_call_matches_typescript`: RED; proves final TS partials differ from Rust push-time clones (4 events) |
| delayed custom-tool consumer, input split across two deltas | `process_responses_stream`, queued `EventStream::next` | `delayed_consumer_custom_tool_matches_typescript`: RED; proves final TS partials differ from Rust push-time clones (5 events, including closing JSON delta) |
| usage/cost and service-tier resolution | `finalize_response` | stream 0; service-tier pricing in `openai-responses` |
| stop-reason mapping | `map_stop_reason` | stream 0, 6, 9 |
| terminal-event requirement | end-of-loop check | stream 8; behavioral test |
| `response.failed` / `error` messages | event arms | stream 7 |

## Differential normalizations
- Default-mode event `partial` payloads are snapshotted by the oracle at push time. TypeScript events carry a live reference; Rust events carry a value snapshot. This proves normalized push-time payloads, not consumer-time parity.
- The oracle strips TypeScript streaming scratch keys (`partialJson`, `customInput`, `streamIndex`, `partialArgs`) from event snapshots. Rust keeps them in slot state and never exposes them.
- Numbers compare as `f64` values (`float_roundtrip` parsing), not lexical forms.
- Timestamps are supplied by the test fixture, so no clock normalization is needed.
- `consumption: "delayed"` uses the pinned `createAssistantMessageEventStream`, awaits the real `processResponsesStream`, ends the queue, then serializes each yielded event. No scratch/index fields are stripped. Rust likewise finishes processing and ends its sender before draining `next()`.

## Gaps
- `applyServiceTierPricing`/`resolveServiceTier` callbacks cannot be expressed in the JSON oracle; Rust behavior is covered in `openai-responses` behavioral tests.
- TypeScript untyped escape hatches (`as any`, deleting scratch fields) have no Rust equivalent by design.

## Discrepancies
- Phase F1 (2026-10-20): delayed-consumer parity is disproven for both valid-output fixtures. Event types, content indexes, deltas and `toolCall` payloads agree, but `partial` messages do not. Every TS partial observes final content and terminal metadata through the same mutable message reference. Rust retains each push-time clone.
  - Function call: TS `toolcall_start.partial.content[0].arguments` is `{"path":"README.md"}`; Rust is `{}`. The first argument delta likewise differs.
  - Custom tool: TS start arguments are `{"input":"hello"}`; Rust is `{"input":""}`. The first delta has Rust `{"input":"he"}` while TS already has `hello`.
  - All TS partials include `responseId: "resp_delayed"`, `rawStopReason: "completed"`, and final `stopReason: "toolUse"`. Rust queued partials omit those response fields and retain `stopReason: "pending"`, including `toolcall_end` emitted before response completion.
  - Neither side exposes `partialJson`, `customInput`, `partialArgs`, `streamIndex`, or `index` on the consumed tool block; delayed mode performs no filtering. Scratch deletion is not the divergence.
- Classification: mutable-aliasing compatibility gap, not an approved semantic deviation. Production changes stopped at the explicit Phase F1 boundary: matching TS would require changing value-snapshot semantics already covered by the push-time suite. Both new tests intentionally remain RED; no production code changed and no GREEN claimed.
- Evidence: `/tmp/pi-rust-streaming-consumer-proof/02-targeted-red.log` contains both complete serialized event sequences; `ledger.md` records command, revision/diff scope and results. Targeted run: 4 existing tests pass, 3 new delayed tests fail across the two modules. `--no-fail-fast` permits both requested test binaries to run.
- `mapStopReason`'s `never` branch throws `Unhandled stop reason: <status>`; Rust returns the same message as an error.
