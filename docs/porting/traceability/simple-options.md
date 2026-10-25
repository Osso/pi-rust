# simple-options traceability

Source: `packages/ai/src/api/simple-options.ts`.
Rust: `crates/pi-ai/src/api/simple_options.rs`.
Tests: module unit tests plus `streamSimple` paths in the Responses/Azure/Completions differential suites.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `CONTEXT_SAFETY_TOKENS`, `MIN_MAX_TOKENS` | constants | `context_clamp_reserves_safety_tokens` |
| `clampMaxTokensToContext` | `clamp_max_tokens_to_context` | same |
| `buildBaseOptions` | `build_base_options` | `base_options_drop_old_sampling_params`; whole frozen field-selection/context/key-precedence differential; streamSimple scenarios |
| `MIN_ANSWER_TOKENS` | `MIN_ANSWER_TOKENS` | `thinking_budget_leaves_answer_room` |
| `clampReasoning` | `clamp_reasoning` | `reasoning_clamps_match_standard_levels` |
| `clampStandardReasoningLevel` | `clamp_standard_reasoning_level` | same |
| `adjustMaxTokensForThinking` | `adjust_max_tokens_for_thinking` | `thinking_budget_leaves_answer_room` |
| `ThinkingLevel` → `ModelThinkingLevel` | `model_thinking_level` | streamSimple scenarios |

## Representation
- `SimpleStreamOptions` carries the base `StreamOptions`; `build_base_options` rebuilds a `StreamOptions` with only the TypeScript-listed fields (API-specific extras are dropped, matching the object literal).
- Frozen 4f9e6f8a4 does not forward or merge `samplingParams`. The former builder merge is removed. `cwd` is forwarded, including when an explicit API key overrides the caller key; the final caller API key is kept even when empty.

## Common-pin audit (2026-10-25)

Whole module compared with unmodified 4f9e6f8a4: context-window disabled/enabled, safety reserve and minimum, default/explicit fractional ceiling, every reasoning level, default/custom/zero budgets, explicit/absent caller cap and answer reserve. `differential_simple_options` covers 45 base-option cases and 168 budget cases plus all clamp levels. Genuine RED: old samplingParams appeared in the base output. All listed pass-through fields match the source object literal; callback behavior is exercised by the provider streamSimple differential suites. No source algorithm delta.

## Gaps
- `undefined` versus absent is represented as `None`; both are omitted from request bodies.
