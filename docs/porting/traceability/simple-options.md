# simple-options traceability

Source: `packages/ai/src/api/simple-options.ts`.
Rust: `crates/pi-ai/src/api/simple_options.rs`.
Tests: module unit tests plus `streamSimple` paths in the Responses/Azure/Completions differential suites.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `CONTEXT_SAFETY_TOKENS`, `MIN_MAX_TOKENS` | constants | `context_clamp_reserves_safety_tokens` |
| `clampMaxTokensToContext` | `clamp_max_tokens_to_context` | same |
| `buildBaseOptions` | `build_base_options` | `base_options_merge_sampling_params`; streamSimple scenarios |
| `MIN_ANSWER_TOKENS` | `MIN_ANSWER_TOKENS` | `thinking_budget_leaves_answer_room` |
| `clampReasoning` | `clamp_reasoning` | `reasoning_clamps_match_standard_levels` |
| `clampStandardReasoningLevel` | `clamp_standard_reasoning_level` | same |
| `adjustMaxTokensForThinking` | `adjust_max_tokens_for_thinking` | `thinking_budget_leaves_answer_room` |
| `ThinkingLevel` → `ModelThinkingLevel` | `model_thinking_level` | streamSimple scenarios |

## Representation
- `SimpleStreamOptions` carries the base `StreamOptions`; `build_base_options` rebuilds a `StreamOptions` with only the TypeScript-listed fields (API-specific extras are dropped, matching the object literal).
- Sampling params merge preserves model key order and overrides values in place.

## Gaps
- `undefined` versus absent is represented as `None`; both are omitted from request bodies.
