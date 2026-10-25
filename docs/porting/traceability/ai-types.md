## Q15c KnownProvider literal reconciliation (2026-10-24)

Actual frozen TypeScript compiler API exposes the complete exported KnownProvider union, compared to native KNOWN_PROVIDERS as public ordered data. Genuine41vs36 RED → exact36 GREEN; openai-codex-team added, six old-only ids removed. Custom ProviderId strings and frozen Azure API adapter remain valid. [Contract and old-provider implementation audit](../../specs/known-providers.md), [Radius deletion ledger](radius-config.md). This closes only KnownProvider, not the whole types.ts row.

## Q4 common-pin status (2026-10-22)

Partial. Frozen `types.ts:407` has only stop/length/toolUse/error/aborted. Frozen provider initializers (`api/anthropic-messages.ts:490`, `openai-completions.ts:174`, `openai-responses.ts:110`, `azure-openai-responses.ts:89`, `openai-codex-responses.ts:250`, `google-generative-ai.ts:75`, `google-vertex.ts:92`, `bedrock-converse-stream.ts:127`, `mistral-conversations.ts:148`) initialize stop, not pending. `AssistantMessage::empty` now matches. RED new initial-reason regression; GREEN 15 type tests. `/tmp/pi-rust-repin-delta-ai/ledger.md`, revision `18487b42` + initial-stop slice.

Removed old-only `StopReason::Pending/Deferred`, `DoneReason::Deferred`, and `AssistantMessage.deferred` per frozen `types.ts:407–438`; whole frozen package search found no renamed DeferredHandle/fetchDeferred/cancelDeferred equivalent. Stream producer terminal matches, supervisor/print labels, imported assistant construction and fixtures adapted; proxy's temporary fail-fast wildcard removed (frozen proxy.ts:345–352). RED variant regression → GREEN 16 types tests. Persisted pending/deferred records remain raw, including old deferred handles/unknown fields: `lossless_session_persistence::old_pending_deferred_and_end_turn_fields_load_and_save_as_frozen_raw_records` compares actual frozen parse/serialization and file round-trip bytes, GREEN 1. Ledger: `/tmp/pi-rust-repin-delta-ai/ledger.md`, revision `bc681593` + stop-reason slice.

Removed source-absent AssistantMessage.endTurn and all constructor literals; SSE/WebSocket Codex producers no longer read or emit it. Exact provider differential RED precedes removal. Persisted records keep legacy endTurn raw: `lossless_session_persistence::old_pending_deferred_and_end_turn_fields_load_and_save_as_frozen_raw_records` compares actual frozen serialization and filesystem roundtrip bytes. The coding-agent end-turn tool is unchanged.

Deferred request interfaces, constrained sampling and other pre-repin exports below remain pending deletion, not new-pin parity evidence. The historical mapping below is retained only for migration inventory.

## ImageContent unknown-property carrier (2026-10-25)

[Contract and proof targets](../../specs/image-content-properties.md). Frozen types.ts:363–367 known fields are stored once; public ModelProperties retains only unknown JsValue properties and original key order. Shared decode/serialize uses current typed known fields, preserving interleaved order after mutation. No unknown-field JSON/string projection occurs before serialization; native Infinity remains native, nested lone UTF16 keys/values remain exact. Frozen Completions :905–910 explicitly maps image_url/data URI and ignores extras; actual-source request negative control exercises that boundary. Real RPC prompt/get_messages and session append/load proofs cover the observable persistence/IPC paths. The obsolete RPC extra-key guard is removed with parent approval after actual-source RED; known-field scalar exclusions remain unchanged. Whole types.ts is still partial.

At8c9131d69 (parent0c700e231 integrated): image carrier3, existing lossless content12 and types19 GREEN; `q15f-image-carrier-green-2.log`. Actual-source RPC/session target2 GREEN atba8d30064 (`q15f-image-roundtrip-green.log`), with no later production change. Existing RPC typed-boundary target1 GREEN (`q15f-image-rpc-boundary-green.log`), now retaining extras while still rejecting unpaired data/mimeType. Session proof writes frozen→native, appends native→frozen, appends frozen→native and checks image JSON order/values after reopening. Frozen session-manager.ts:1748 appendMessage and RPC :408–416/:671–672 are the tested boundaries; existing oracles run actual common-pin modules unchanged.

RED evidence: `q15f-image_content_properties-red.log` drops unknowns/native Infinity; `q15f-image-roundtrip-red-2.log` drops persisted image extras and explicitly rejects RPC extras. The initial provider fixture filename and duplicate import were test setup errors, corrected before the genuine RED. An initial Infinity assertion wrongly expected native values after js_json::to_value, which is already a JSON serialization boundary; corrected to inspect stored ModelProperties before JSON, clone/mutate without projection, then prove null at JSON output. No native-preservation requirement was removed.

Workspace/all-target JSONcheck0errors/0warnings and all-feature/all-target warning Clippy0errors/1664existing warnings/0changed-line findings/0new warning signatures versus pre-carrier baseline; `q15f-image-{check,clippy}.jsonl`, clippy-owned/new JSON. Manual changed-line Rust readability has no blocker; metrics executable unavailable. No new dependencies, lossy conversions, alternate property authority, source mutation or provider fallback.

# ai types + event stream traceability

Sources: `packages/ai/src/types.ts`, `packages/ai/src/utils/event-stream.ts`.
Rust: `crates/pi-ai/src/types.rs`, `crates/pi-ai/src/utils/event_stream.rs`, `crates/pi-js-compat/src/number.rs`.

## Enumerable AgentTool metadata

[Contract](../../specs/tool-enumerable-metadata.md). Frozen AgentTool extends Tool with label, optional executionMode and function-valued prepareArguments/prepareExecution/execute (`agent-core/src/types.ts:383–411`). Only label/executionMode survive JSON. Frozen coding-agent `tool-definition-wrapper.ts:100–119,141–156` emits name,label,description,parameters,executionMode in that order after function/undefined omission. Rust Tool retains optional JsString label/execution_mode in that order; constructors leave both absent. These are JSON carriers, not an AI execution policy.

| Whole-object path / explicit mapping | Frozen evidence | Metadata behavior |
|---|---|---|
| Faux prompt serialization | `ai/providers/faux.ts:190–202` | JSON.stringify(context.tools) counts both fields |
| Context token estimate | `ai/utils/estimate.ts:21–27,101–103` | safeJsonStringify(context.tools) counts both fields |
| Whole-context proxy, outside AI package | `agent-core/proxy.ts:158–162` | JSON.stringify({model,context,options}) retains both fields; raw-context request proof |
| Completions / Responses / Azure / Codex | `openai-completions.ts:1095–1108`, `openai-responses-shared.ts:425–437`; other three delegate | Explicit name/description/parameters mapping ignores both fields |
| Anthropic / Google / Bedrock / Mistral | `anthropic-messages.ts:1195–1208`, `google-shared.ts:308–317`, `bedrock-converse-stream.ts:905–911`, `mistral-conversations.ts:485–495` | Explicit schema/name/description mapping ignores both fields |

Whole AI-source JSON.stringify/structuredClone/spread audit found no additional whole-Tool consumer: other serializers handle arguments, messages, reasoning items, OAuth/error/proxy data or already-mapped request bodies; schema spreads/clones affect parameters only. No generic unknown-property bag or function serialization is added. AgentTool population is owned by222; its label accessor's native representation is outside this carrier slice.

RED reproduced the shared harness fixture exactly: Native Faux input48 versus actual frozen53 (`tool-metadata-red.log`). `tool_metadata.rs` additionally compares whole Tool JSON order, both estimators, unpaired label units, optional execution modes and explicit provider mapping. Carrier availability alone does not claim harness integration.

## Q15 native Model display text and properties

[Contract and current proof](model-json-properties.md). Model.name is native human text per user2026-10-22; opaque Model.id remains scalar with explicit unpaired rejection. Unknown top-level cached properties and original interleaved key order are retained without duplicating known values. Three targeted carrier/cache JSON tests GREEN; nested cost/tier metadata also preserves native unknowns and order. Typed compat properties, registry/RPC integration and all-target migration remain pending.

## Human Tool descriptions

`Tool.description` is a native `JsString`, not opaque protocol metadata. Frozen adapters forward it unchanged in the request tool mapping: Anthropic, Completions, Responses/Azure/Codex, Google Generative/Vertex, Bedrock and Mistral. `tool_description.rs` compares unmatched description units against actual frozen HTTP bodies for all nine APIs; the initial Context decode rejected `\ud800` before any native request. Tool builders now use native JSON objects instead of scalar serde staging. Whole-tool JSON and both estimators remain covered by `tool_metadata.rs`, including unmatched description units. Schema parameters remain a separate pending native-boundary migration. Contract: [lossless content](../../specs/lossless-content.md).

## Representation decisions
- `StreamOptions.metadata` is native `JsObject`, preserving arbitrary nested JSON and lone UTF-16 keys/values through buildBaseOptions. Anthropic forwards only string user_id, matching frozen; proxy forwards the complete metadata object. RED: metadata deserialization rejected a lone surrogate; proxy silently omitted native metadata. Proof: `lossless_metadata` and core `proxy` UTF-16 regression (`metadata-{red,proxy-red}.log` in the lane ledger). ImagesOptions.metadata and Bedrock's separate requestMetadata extra are outside this common-field slice.
- TS `number` → `f64` serialized via `pi_js_compat::number` (integral values print without `.0`, like `JSON.stringify`). User/Assistant/ToolResult `timestamp` → `f64`, including nonfinite/signed-zero values; [integration proof](message-timestamps.md). `contentIndex` → `usize`. `Number.parseInt` → `pi_js_compat::number::parse_int` (used by the `pi-ai` CLI selection prompt).
- `Api`/`ProviderId`/`ImagesApi`/`ImagesProviderId` (`Known… | string`) → `String`; known literals exported as `KNOWN_APIS`, `KNOWN_PROVIDERS`, `KNOWN_IMAGES_APIS`, `KNOWN_IMAGES_PROVIDERS`.
- Each content/message struct carries its literal discriminant (`type`/`role`) as a single-value enum field, so standalone values (`imageGenerationResult`, `toolCall`, `AssistantMessage`) serialize with the tag first like TS. Unions over them (`AssistantContent`, `TextOrImageContent`, `Message`) are `untagged` and dispatch on that tag.
- `Model<TApi>.compat` → `ModelCompat`, parsed by `api` (`openai-completions`; `openai-responses|azure-openai-responses|openai-codex-responses`; `anthropic-messages`; `bedrock-converse-stream`); other APIs ignore `compat` (TS `never`).
- `Tool.parameters: TSchema` → `serde_json::Value` (JSON Schema).
- `constrainedSampling: false | Config` → `ConstrainedSampling::{Disabled, Config}`.
- `AssistantMessage` serializes the base fields in the order every provider's object literal uses (`role`, `content`, `api`, `provider`, `model`, `usage`, `stopReason`, `timestamp`), then optional fields in interface declaration order. `createErrorEvent` in `pi-messages` places `errorMessage` before `timestamp`; that single path differs and is not asserted (see `pi-messages.md`).
- `ThinkingLevelMap` and grammar variants → `BTreeMap` (lookup-only; output order is not observable). `Model.headers`, `ProviderEnv`, `ProviderHeaders`, chat-template kwargs/args, and grammar-tool input properties → `IndexMap`, because TypeScript object insertion order reaches serialized payloads and HTTP request headers. `serde_json` `preserve_order` keeps object order for `arguments`, `details`, `samplingParams`, `value`. `ProviderResponse.headers` uses `IndexMap`: custom providers supply arbitrary insertion order. Built-in fetch adapters normalize through `headers_to_record`, retaining lowercase-sorted WHATWG order. `tests/headers.rs` asserts serialized built-in order; coding-agent `tests/sdk.rs::custom_provider_response_preserves_header_order_through_sdk_and_json` proves custom-provider callback → SDK handler → event JSON order.
- `EventStream` is split into `EventSender` (push/end/fail) and `EventStream` (next/result). The existing `StreamFailure` wraps the shared `CallbackError` authority; From<CallbackError> retains its exact rejection and Error source instead of a message projection. Its legacy new(message) constructor still creates Error(message). PartialEq follows the rejection carrier; no Eq promise is made for arbitrary JS values. The required low-level registered-acquisition iteration/result proof is [mapped here](agent-loop.md#registered-stream-acquisition-propagation-2026-10-24).

## Valid stream scratch fields

`TextContent.index`, `ThinkingContent.index`, and `ToolCall.index` are optional
`f64` fields, serialized with the shared JS-number convention. `ToolCall.stream_index`
uses that same convention for Completions `streamIndex`. `ToolCall.partial_json` and
`partial_args` are optional native strings serialized as `partialJson`/`partialArgs`. Constructors set them to `None`;
absent fields stay absent. These are literal runtime output properties in the pinned
Anthropic adapter and custom Anthropic example, not invalid typed inputs.

`tests/types.rs::valid_stream_scratch_*` reproduces the lost `index: 3` event
and tool JSON buffer, then proves complete event round trips (including fractional
and large numeric indices). Producer lifecycle proof covers
[Anthropic Messages](anthropic-messages.md), [Responses](openai-responses-shared.md#native-tool-scratch-lifecycle)
and [Completions](openai-completions.md#native-completions-scratch); other providers' scratch lifecycle remains pending.
Contract: [content stream scratch](../../specs/content-stream-scratch.md).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| content types, `ToolCall`, `Usage`, messages, `Message` | `types::*` | `tests/types.rs` round-trip of TS-shaped JSON |
| `AssistantMessageEvent` (12 variants), `done`/`error` reason subsets | `AssistantMessageEvent`, `DoneReason`, `ErrorReason` | `stream_events_round_trip_with_snake_case_tags`, `done_event_preserves_frozen_error_discrepancies_and_rejects_invalid_reasons`; literal [custom Anthropic](done-error.md) and [Codex terminal status](openai-codex-responses.md#discrepancies-preserved-literally) done/error; aborted/unknown and existing pending/deferred rejections retained |
| `Model`, `ModelCost(+tiers)`, compat interfaces, routing | `Model`, `ModelCompat`, `*Compat`, `OpenRouterRouting` | `model_compat_is_typed_by_api`, `custom_api_model_parses_without_compat` |
| `ModelThinkingLevel`, `ThinkingLevelMap` | enum-known nullable JsString values plus disjoint unknown/order carrier | `model_registry_repin`35real-file models/order snapshots; selected values through49-case SDK request matrix and4real Bedrock HTTP cases; models19/types19; source consumer matrix in [registry](model-registry.md#thinking-level-map-dependency-2026-10-24) |
| `Context`, `Tool`, `ConstrainedSamplingConfig` | same | `context_round_trips_tools_with_json_schema_parameters` |
| `EventStream.push/end/fail/iterator/result` | `EventSender`/`EventStream` | `tests/event_stream.rs`; `tests/differential_event_stream.rs` (8 scripted scenarios vs real TS class) |
| `AssistantMessageEventStream`, `createAssistantMessageEventStream` | `create_assistant_message_event_stream` | covered by type tests; exercised by providers |
| `ImagesContext`, `AssistantImages`, `ImagesModel`, `ProviderRetryEvent`, `ProviderResponse`, `DeferredHandle`, `ProviderNativeContext`, `ThinkingBudgets`, `TextSignatureV1` | same names | serde derive; exercised by consuming slices |

## Request contracts (`crates/pi-ai/src/request.rs`, `crates/pi-ai/src/http.rs`)

Approved deviation (2026-10-22), binding rule in AGENTS.md: `options.request.fetch` remains a Rust-only injection seam for tests/embedders. Frozen has no such option; unset must behave exactly like frozen default fetch. This closes the prior pending interface delta. Source-side recorded SDK oracles intercept global fetch or the SDK transport instead; never pass this native option as frozen proof. [Current oracle boundary](../ai-scripted-provider-oracles.md).

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

## Response-header conversion audit (2026-10-20)

Changed direct sites:
- `types.rs::ProviderResponse.headers` and coding-agent `extensions/types.rs::AfterProviderResponseEvent.headers`: insertion-ordered fields.
- `utils/headers.rs::headers_to_record`: normalize/merge in a sorted map, then collect into an ordered record. Existing callers in `anthropic_messages`, `azure_openai_responses`, `mistral_conversations`, `openai_completions`, `openai_responses`, `openrouter_images`, and `pi_messages` inherit the new return type without call-site edits. `anthropic_sdk` iterates the same sorted output.
- `api/openai_codex_responses.rs::fetch_sse_with_retries`: use the same normalization instead of collecting raw response entries; pinned TS uses `headersToRecord` here too.
- `api/bedrock_converse_stream.rs::drive`: construct the empty/single request-id record with `IndexMap`.
- `providers/faux.rs`: ordinary streaming, deferred fetch, and deferred cancellation response callbacks construct empty ordered records. `tests/support/anthropic.rs::requests_json` updates its explicit type annotation.

The SDK forwards headers unchanged; its source needs no conversion. `tests/sdk.rs` covers the real registered custom-provider path and serialized `ExtensionEvent`. Targeted proof: SDK 11/11; AI headers 2/2, differential headers 2/2, OpenRouter images 13/13. This is not a broad provider-suite or RPC process test.

Q18 shared UTF-16 fields, argument parser and outbound-tree migration: [boundary/proof matrix](lossless-content.md). Checkpoint evidence does not establish unchanged consumer-fixture compatibility or full workspace parity.

## Native union number decoding

TextOrImageContent/AssistantContent/Message select their discriminators from JsValue and decode records directly; UserContent takes native text directly and decodes block arrays natively. No stringify/from_str bridge remains in these unions. [Shared numeric correction/proof](js-json.md#typed-union-and-checked-adapter-follow-up) covers nonfinite numbers, signed zero and unpaired human text without changing serialized wire semantics.

2026-10-25: deleted the source-absent DeferredHandle, DeferredRequest/Window, DeferredFetch/CancelOptions, SimpleStreamOptions.deferred and ProviderStreams deferred methods. No native provider consumed them; only old-capability fixtures did. Constrained-sampling carrier removal waits for parent integration of coding-agent consumer cleanup.

## Constrained-sampling carrier retirement (2026-10-25)

Frozen types.ts:478–482 Tool has only name/description/parameters. After parent4eff32510 removed the coding-agent algorithms, deleted GrammarFormat/Variants, JsonSchemaStrictness, ConstrainedSamplingConfig/ConstrainedSampling, Tool.constrained_sampling, the AgentTool getter/loop forwarding and ToolDefinition field. Required declarations, constructor literals and fixtures were changed mechanically across lanes; real label/execution-mode/prepare metadata remains unchanged.

Retired source-absent metadata-only fixtures: `old_strict_metadata_does_not_rewrite_frozen_tool_parameters`, `grammar_metadata_does_not_change_frozen_function_tools`, `ignores_old_strict_tool_metadata_and_keeps_plain_parameters`, and `frozen_wrappers_do_not_copy_old_constrained_sampling_metadata`. Existing complete payload/stream/provider tests stay enabled; the Codex fixture keeps its real parameter schema and assertions without the removed flag. Experimental gate proof now compares the actual frozen experimental.ts export directly, not invented null tool-metadata fields.

Remaining known old-only carrier: ToolResultMessage.added_tool_names. Whole types.ts stays partial until its removal and the remaining public-type audit.

Also retired the tools-index `tool_definitions_do_not_add_removed_experimental_sampling`/private `experimental_sampling_probe` pair: after deleting the artificial observer field, indexing the missing property on both snapshots would merely compare null to null. Full real definition/value comparisons remain enabled.

## Tool-result field retirement (2026-10-25)

Parent approved removal of ToolResultMessage.usage and added_tool_names: neither is declared by frozen types.ts:443–452. Removed both typed fields and constructor entries, without changing the existing raw session-record representation. New `old_rust_tool_result_usage_survives_lossless_disk_read_and_rewrite` reads a real old-format JSONL file, compares serialization with actual frozen session-manager.ts, rewrites through the existing lossless serializer and asserts byte identity. This preservation guard passed before deletion; it is not a claimed RED bug fix.

The only production field read was in the source-absent, uncalled legacy get_usage_cost_breakdown export. Removed that export/type plus its `groups_usage_by_model_and_tool_bucket` and `uses_response_model_when_present` fixtures; retained the unchanged create/add arithmetic helpers used by the live footer on assistant usage. No visible footer stats algorithm changed. The reported-usage estimate fixture remains enabled with unchanged token assertions; its obsolete tool-name metadata alone was removed.

Parent fdcb8f8fe was searched explicitly after field deletion (34 matching source/test paths; no missed parent-only file). The remaining extension test-name mention describes an actual serialized-output absence assertion, not a constructor field. Parent session_autoname.rs has neither carrier field nor stale constrained-sampling constructor. AssistantMessage.raw_stop_reason still requires separate cleanup.

## Raw stop-reason field retirement (2026-10-25)

Removed AssistantMessage.raw_stop_reason and None-only constructor entries. Frozen AI source has no rawStopReason declaration or use; native had no Some assignments. Existing six stop/error assertions now check actual serialized output has no rawStopReason key, rather than inspecting an obsolete optional Rust field. Real stopReason/errorMessage/diagnostics and generic raw session records are unchanged. Parent fdcb8f8fe was searched for all raw-stop references; no missed parent-only carrier constructor remains.

Serialized-name audit found three stale fields in the generic typed round-trip fixtures (addedToolNames and two constrainedSampling inputs). The unchanged full-equality assertion reproduced the mismatch before fixture correction (`/tmp/ai-retired-types-fixture-red.log`). Fixtures now contain only frozen Tool/ToolResultMessage fields; legacy-data preservation remains separately asserted through raw session records. Print-mode fixed-order validation no longer permits the removed rawStopReason key, which strengthens its wire assertion. Provider differentials that deliberately send ignored legacy metadata remain unchanged.

AssistantImages.timestamp now uses the existing f64/number serialization authority rather than rejecting normal fractional JSON timestamps. Source type witness and bidirectional12.75 value/wire proof accompany the change; [timestamp map](message-timestamps.md). Whole current-pin type audit remains open.
