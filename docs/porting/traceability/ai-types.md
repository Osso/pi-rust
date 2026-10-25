# AI public types — common-pin audit

Source: complete `packages/ai/src/types.ts` at `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`.
Native: `crates/pi-ai/src/types.rs`, `types/{compat,model_properties,thinking_level_map}.rs`, `request.rs`.

## Export and field audit (2026-10-25)

The entire source module was compared to the current native declarations, discriminants, optional fields, constructors and serializers. The source contains no deferred/constrained/sampling interfaces. Old Pending/Deferred stop variants, AssistantMessage.deferred/endTurn, request deferred handles and methods, constrained-sampling contracts, samplingParams known fields, tool-result usage and added-tool-name fields have been retired. Historical persisted properties stay on existing raw authorities; they do not become public native features.

The successor also retired the unused old-only ThinkingFormat::Baseten variant (main approved), ModelCostTier and ModelCost.tiers. Cost has exactly four known rates; historical tiers remain unknown lossless properties, with their original key position. A native regression first failed on the old `model.cost.tiers cannot be null` check, then exercises unknown null/nested metadata and the remaining raw custom compat authority. No cost algorithm changes. RED: `/tmp/ai-successor-types-red-final.log` (two earlier attempts were fixture compile errors, not behavioral evidence).

- API/provider unions: all nine KnownApi literals, one KnownImagesApi, all 36 KnownProvider literals, one KnownImagesProvider; custom strings remain supported. Existing `known_provider_contract` imports the actual frozen compiler union. `Api`/provider identifiers remain scalar under the opaque-ID approval.
- Thinking/options: ThinkingLevel/ModelThinkingLevel, nullable ThinkingLevelMap, ChatTemplateKwargValue, ThinkingBudgets, CacheRetention, Transport, ProviderEnv/Headers/Response/RetryEvent; StreamOptions/ProviderStreamOptions/SimpleStreamOptions and image equivalents map to request structs plus existing API-specific extra fields. ProviderStreams/ProviderImages implement the source function contracts; compile-time ApiOptionsMap/ApiStreamOptions are per-API parsing, not a runtime registry.
- Content/messages: TextSignatureV1, TextContent, ThinkingContent, ImageContent, ToolCallExecution/ToolCall, Usage and cost, StopReason, ProviderNativeContext, user/assistant/tool-result messages, Message, ImagesInput/OutputContent, ImagesContext, AssistantImages/ImagesStopReason, Tool and Context. Human text/errors, arbitrary argument/details/native-context JSON retain JsString/JsValue. Native assistant fields use the approved fixed output order; imported records retain raw order at persistence boundaries.
- Stream events: all twelve tags, contentIndex, delta/content/toolCall/partial/final payloads, done/error reason subsets. Source-discrepant done/error producers remain supported literally; ErrorReason stays error/aborted only. Event deserialization dispatches on JsValue rather than Serde's string-projecting internal-tag buffer. Existing `types` event/native-text tests cover this repaired boundary.
- Models/routing: Model and ImagesModel, all three compat interfaces and flags, OpenRouterRouting (including sorting/prices/percentile thresholds), VercelGatewayRouting. ImagesModel inherits thinkingLevelMap/autoCompactionThreshold/headers and omits chat-only api/provider/reasoning/contextWindow/maxTokens/compat before redeclaring image API/provider/output. Model/ImagesModel/cost/compat unknowns use one ModelProperties authority and retain original order.

## Existing behavioral and wire proof

`tests/types.rs` covers complete message/event/model/image/context records, initial stop, retired reasons, fractional and JS-number timestamps, stream scratch properties, native event text, image model display/inherited/unknown fields, and retained historical sampling keys. `public_oauth_types` proves the OAuth names exported through the AI entry. The predecessor's frozen-builder type witness and fixtures passed (`/tmp/ai3-contracts-typecheck.log`); this successor adds no frozen fixture/matrix.

Persistence/wire proof remains authoritative, not replaced by the declaration audit:

- [Lossless content](lossless-content.md), [model JSON properties](model-json-properties.md), [image content properties](../../specs/image-content-properties.md), [model registry](model-registry.md): native human text, unknown fields/order, real session/RPC/provider boundaries and model reconstruction. Session JSONL retains old pending/deferred/endTurn fields as frozen raw records; `lossless_session_persistence::old_pending_deferred_and_end_turn_fields_load_and_save_as_frozen_raw_records` is unchanged.
- [Tool metadata](../../specs/tool-enumerable-metadata.md): AgentTool's enumerable label/executionMode reach whole-tool JSON and estimators, but provider adapters explicitly map only their source fields. [Tool descriptions](../../specs/tool-render-paths.md) and provider tests retain human description units.
- [Content stream scratch](../../specs/content-stream-scratch.md): optional index/partialJson/partialArgs/streamIndex are real frozen runtime fields, not old-only type interfaces. Anthropic, Responses and Completions traces own their producer lifecycle.
- [AI API audit](../ai-api-repin-proof.md), [request error identity](native-http-failure-identity.md), [headers](headers.md), [agent loop](agent-loop.md): callback Result propagation, reason-bearing abort signals, fetched header ordering, stream acquisition and payload/request contracts.
- [EventStream](events.md): sender/receiver ownership replaces JS identity while preserving push/end/iteration/result behavior. No second event carrier was introduced.

## Approved boundaries and source discrepancies

DISCREPANCY: frozen custom Anthropic/Codex paths can emit done/error despite the nominal done subset; DoneReason::Error is retained, with [native terminal proof](done-error.md). DISCREPANCY: EventStream.end without a result leaves result pending forever; preserved by the existing event-stream test. Snapshots are immutable per the owned-value approval.

The Rust-only request.fetch seam remains approved and unset uses default fetch. Known typed invalid JS input shapes are excluded; opaque IDs/signatures reject unpaired units rather than replace them. Model display names, text/errors and arbitrary JSON are not covered by that exclusion. Native assistant key order, raw imported order, and JS-number serialization follow AGENTS.md. No telemetry/deferred/constrained contract is inferred from historical traces.

Gap: adversarial/exotic runtime inputs, numeric extremes and Node/V8 scheduling/runtime faults use the standing one-line runtime exclusion; existing lossless carriers remain intact.

## Final gate

Successor proof results and exact revisions are recorded in [AI core successor checkpoint](../ai-core-successor-proof.md). The audit alone does not replace persistence/HTTP proof or a passing current workspace check.
