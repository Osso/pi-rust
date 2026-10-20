# ai types + event stream traceability

Sources: `packages/ai/src/types.ts`, `packages/ai/src/utils/event-stream.ts`.
Rust: `crates/pi-ai/src/types.rs`, `crates/pi-ai/src/utils/event_stream.rs`, `crates/pi-js-compat/src/number.rs`.

## Representation decisions
- TS `number` → `f64` serialized via `pi_js_compat::number` (integral values print without `.0`, like `JSON.stringify`). Unix-ms `timestamp` → `i64`. `contentIndex` → `usize`. `Number.parseInt` → `pi_js_compat::number::parse_int` (used by the `pi-ai` CLI selection prompt).
- `Api`/`ProviderId`/`ImagesApi`/`ImagesProviderId` (`Known… | string`) → `String`; known literals exported as `KNOWN_APIS`, `KNOWN_PROVIDERS`, `KNOWN_IMAGES_APIS`, `KNOWN_IMAGES_PROVIDERS`.
- Each content/message struct carries its literal discriminant (`type`/`role`) as a single-value enum field, so standalone values (`imageGenerationResult`, `toolCall`, `AssistantMessage`) serialize with the tag first like TS. Unions over them (`AssistantContent`, `TextOrImageContent`, `Message`) are `untagged` and dispatch on that tag.
- `Model<TApi>.compat` → `ModelCompat`, parsed by `api` (`openai-completions`; `openai-responses|azure-openai-responses|openai-codex-responses`; `anthropic-messages`; `bedrock-converse-stream`); other APIs ignore `compat` (TS `never`).
- `Tool.parameters: TSchema` → `serde_json::Value` (JSON Schema).
- `constrainedSampling: false | Config` → `ConstrainedSampling::{Disabled, Config}`.
- `AssistantMessage` serializes the base fields in the order every provider's object literal uses (`role`, `content`, `api`, `provider`, `model`, `usage`, `stopReason`, `timestamp`), then optional fields in interface declaration order. `createErrorEvent` in `pi-messages` places `errorMessage` before `timestamp`; that single path differs and is not asserted (see `pi-messages.md`).
- `ThinkingLevelMap` and grammar variants → `BTreeMap` (lookup-only; output order is not observable). `Model.headers`, `ProviderEnv`, `ProviderHeaders`, chat-template kwargs/args, and grammar-tool input properties → `IndexMap`, because TypeScript object insertion order reaches serialized payloads and HTTP request headers. `serde_json` `preserve_order` keeps object order for `arguments`, `details`, `samplingParams`, `value`. `ProviderResponse.headers` stays a `BTreeMap`: fetch `Headers` iteration is sorted.
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

## Request contracts (`crates/pi-ai/src/request.rs`, `crates/pi-ai/src/http.rs`)

Bounded native fetch error identity and approved native cause diagnostics: [shared HTTP failure traceability](native-http-failure-identity.md).
| TS | Rust | Proof |
|---|---|---|
| `ProviderRequestOptions` | `ProviderRequestOptions` (`signal` → `CancellationToken`) | consumed by provider slices |
| `StreamOptions`, `ProviderStreamOptions` (`& Record<string, unknown>`) | `StreamOptions { request, ..., extra }`; API modules parse typed options from `extra` | provider slices |
| `SimpleStreamOptions` (`deferred: boolean \| {window}`) | `SimpleStreamOptions`, `DeferredRequest`, `DeferredWindow` | provider slices |
| `DeferredFetchOptions`, `DeferredCancelOptions`, `ImagesOptions` | same names | provider slices |
| `onPayload`/`onResponse`/`onRetry` | `OnPayload`/`OnResponse`/`OnRetry` (`Arc<dyn Fn>`) | provider slices |
| `ProviderStreams` (optional `fetchDeferred`/`cancelDeferred`) | trait `ProviderStreams` with `supports_*` presence flags | runtime slice |
| `ProviderImages` | trait `ProviderImages` | images slice |
| `ImagesOptions` (`ProviderRequestOptions<ImagesModel>`) | `ImagesRequestOptions` + `ImagesOptions` (`request`, `metadata`, `extra`) | images slice |
| `ProviderImagesOptions` (`ImagesOptions & Record<string, unknown>`) | `ImagesOptions::extra` | typed exclusion: untyped remainder |
| `ImagesFunction` | subsumed by `ProviderImages::generate_images` | typed exclusion |
| `FetchFunction` (`typeof globalThis.fetch`) | trait `Fetch` | `tests/http.rs` |
| `fetch?: typeof globalThis.fetch` | trait `Fetch` + `HttpRequest`/`HttpResponse`/`HttpBody`; default `http::default_fetch()` (reqwest, rustls) | `tests/http.rs`: request line/headers/body, streamed chunked body, abort → `"This operation was aborted"` |
| `lazyStream` setup-error message shape | `request::error_stream` | runtime slice |

## Gaps
- `telemetryContext` option: added when pi-telemetry merges.
- `ApiOptionsMap`/`ApiStreamOptions<TApi>`: compile-time conditional mapping only; replaced by `StreamOptions::extra` parsing per API.
- `StreamFunction`/`ImagesFunction` type aliases: subsumed by `ProviderStreams`/`ProviderImages`.
- `ProviderStreamOptions`/`ProviderImagesOptions` untyped remainders: carried in `extra`; typed per-API options live on each API module.
- `FetchFunction`: typed exclusion; the Rust boundary is the `Fetch` trait (`HttpRequest`/`HttpResponse`/`HttpBody`).
- Crate-root re-export mapping is in `ai-index.md` (index.ts exports every name above).

## Discrepancies
- `EventStream.end()` without a result leaves `result()` pending forever in TS; preserved (`end_without_result_leaves_result_pending`).
- After a TS iterator throws the failure, later iteration returns done; Rust returns `Some(Err)` once then `None` (same observable sequence).
