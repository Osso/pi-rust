# ai types + event stream traceability

Sources: `packages/ai/src/types.ts`, `packages/ai/src/utils/event-stream.ts`.
Rust: `crates/pi-ai/src/types.rs`, `crates/pi-ai/src/utils/event_stream.rs`, `crates/pi-ai/src/js_number.rs`.

## Representation decisions
- TS `number` → `f64` serialized via `js_number` (integral values print without `.0`, like `JSON.stringify`). Unix-ms `timestamp` → `i64`. `contentIndex` → `usize`.
- `Api`/`ProviderId`/`ImagesApi`/`ImagesProviderId` (`Known… | string`) → `String`; known literals exported as `KNOWN_APIS`, `KNOWN_PROVIDERS`, `KNOWN_IMAGES_APIS`, `KNOWN_IMAGES_PROVIDERS`.
- Each content/message struct carries its literal discriminant (`type`/`role`) as a single-value enum field, so standalone values (`imageGenerationResult`, `toolCall`, `AssistantMessage`) serialize with the tag first like TS. Unions over them (`AssistantContent`, `TextOrImageContent`, `Message`) are `untagged` and dispatch on that tag.
- `Model<TApi>.compat` → `ModelCompat`, parsed by `api` (`openai-completions`; `openai-responses|azure-openai-responses|openai-codex-responses`; `anthropic-messages`; `bedrock-converse-stream`); other APIs ignore `compat` (TS `never`).
- `Tool.parameters: TSchema` → `serde_json::Value` (JSON Schema).
- `constrainedSampling: false | Config` → `ConstrainedSampling::{Disabled, Config}`.
- `ThinkingLevelMap`, headers, grammar variants, chat-template kwargs → `BTreeMap` (key order on output is sorted; TS insertion order is not preserved — semantic JSON equal). `serde_json` `preserve_order` keeps object order for `arguments`, `details`, `samplingParams`, `value`.
- `EventStream` is split into `EventSender` (push/end/fail) and `EventStream` (next/result). `fail(unknown)` → `StreamFailure { message }`.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| content types, `ToolCall`, `Usage`, messages, `Message` | `types::*` | `tests/types.rs` round-trip of TS-shaped JSON |
| `AssistantMessageEvent` (12 variants), `done`/`error` reason subsets | `AssistantMessageEvent`, `DoneReason`, `ErrorReason` | `stream_events_round_trip_with_snake_case_tags`, `done_event_rejects_error_reasons` |
| `Model`, `ModelCost(+tiers)`, compat interfaces, routing | `Model`, `ModelCompat`, `*Compat`, `OpenRouterRouting` | `model_compat_is_typed_by_api`, `custom_api_model_parses_without_compat` |
| `Context`, `Tool`, `ConstrainedSamplingConfig` | same | `context_round_trips_tools_with_json_schema_parameters` |
| `EventStream.push/end/fail/iterator/result` | `EventSender`/`EventStream` | `tests/event_stream.rs`; `tests/differential_event_stream.rs` (8 scripted scenarios vs real TS class) |
| `AssistantMessageEventStream`, `createAssistantMessageEventStream` | `create_assistant_message_event_stream` | covered by type tests; exercised by providers |
| `ImagesContext`, `AssistantImages`, `ImagesModel`, `ProviderRetryEvent`, `ProviderResponse`, `DeferredHandle`, `ProviderNativeContext`, `ThinkingBudgets`, `TextSignatureV1` | same names | serde derive; exercised by consuming slices |

## Gaps (owned by later slices)
- `ProviderRequestOptions`, `StreamOptions`, `SimpleStreamOptions`, `DeferredFetchOptions`, `ImagesOptions`, `ApiOptionsMap`, `ProviderStreams`, `ProviderImages`, `StreamFunction`, `ImagesFunction`: callback/function interfaces → provider-runtime slice (`api/lazy.ts`, `models.ts`).
- `ProviderEnv`/`ProviderHeaders` are type aliases only here.

## Discrepancies
- `EventStream.end()` without a result leaves `result()` pending forever in TS; preserved (`end_without_result_leaves_result_pending`).
- After a TS iterator throws the failure, later iteration returns done; Rust returns `Some(Err)` once then `None` (same observable sequence).
