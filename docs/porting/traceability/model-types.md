# model-types traceability

Source: `packages/ai/src/model-types.ts`.
Rust: `crates/pi-ai/src/types.rs` (no new module; the file duplicates types already ported).
Tests: `crates/pi-ai/tests/types.rs`, `crates/pi-ai/tests/model_types.rs` (4 focused tests).

## Mapping
`model-types.ts` contains only interfaces, all already represented in `types.rs`:

| TS interface | Rust item |
|---|---|
| `OpenAICompletionsCompat` | `types::OpenAICompletionsCompat` |
| `OpenAIResponsesCompat` | `types::OpenAIResponsesCompat` |
| `AnthropicMessagesCompat` | `types::AnthropicMessagesCompat` |
| `OpenRouterRouting` | `types::OpenRouterRouting` |
| `VercelGatewayRouting` | `types::VercelGatewayRouting` |
| `ModelCostRates` | `types::ModelCostRates` |
| `ModelCostTier` | `types::ModelCostTier` |
| `ModelCost` | `types::ModelCost` |
| `Model<TApi>` | `types::Model` (api-keyed `ModelCompat`) |
| `ImagesModel<TApi>` | `types::ImagesModel` |

No code was added for this module. `Model` deserialization selects `compat` by `api` via `ModelCompat::parse`.

## Proof
- `model_compat_is_typed_by_api`, `custom_api_model_parses_without_compat`, `context_round_trips_tools_with_json_schema_parameters` (`tests/types.rs`).
- `openai_responses_compat_maps_to_the_types_module`, `openrouter_routing_supports_string_and_object_sort_variants`, `vercel_gateway_routing_maps_to_the_types_module`, `images_model_maps_to_the_types_module` (`tests/model_types.rs`).

## Gaps
- TypeScript's conditional `compat` type (`TApi extends … ? … : never`) is represented as a runtime enum parsed by `api`; unknown APIs drop `compat`. Same observable wire behavior as the typed TS contract.

## Discrepancies
- `types::OpenAIResponsesCompat` carries an extra `supportsReasoningEffort`
  field. The TypeScript interface does not declare it, but the pinned
  `opencode` catalog attaches it to an `openai-responses` model and the TS
  runtime preserves unknown properties; Rust needs the field to keep generated
  catalog objects byte-identical (see `providers-models.md`).
