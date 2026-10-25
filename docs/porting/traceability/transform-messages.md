# transform-messages — common pin

Complete for realistic inputs at frozen 4f9e6f8a4. Current native 10 and actual-source 18-scenario differential (1 test) pass at ef3448a48: `/tmp/ai-transform-suite_core_{1,2}.log`. No production change was needed for this closure.

Source: `packages/ai/src/api/transform-messages.ts`.
Rust: `crates/pi-ai/src/api/transform_messages.rs`.
Tests: `crates/pi-ai/tests/transform_messages.rs` (10 behavioral), `crates/pi-ai/tests/differential_transform_messages.rs` (18 scenarios vs pinned TS).

## Representation
- `Message`/`AssistantContent`/`Model` reuse `types.rs`. `model.input` is `Vec<Modality>`.
- The injected clock is an explicit `now: f64` parameter because TypeScript uses `Date.now()` for synthetic results. The differential oracle rewrites only synthetic-result timestamps to the request clock.
- `normalizeToolCallId` is `Option<&N>` with `N: Fn(&str, &Model, &AssistantMessage) -> String`.

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `transformMessages` | `transform_messages` / `transform_messages_with` | behavioral + differential |
| `replaceImagesWithPlaceholder` | `replace_images_with_placeholder` | `downgrades_images_for_text_only_models_and_collapses_consecutive_images` |
| `downgradeUnsupportedImages` | `downgrade_unsupported_images` | same + `keeps_images_for_vision_models` |
| `toolCallIdMap` first-pass normalization | `transform_tool_call` + `BTreeMap` | `normalizes_invalid_tool_ids_even_when_replaying_the_same_model` |
| same-model check (`provider`/`api`/`model`) | `transform_assistant` | differential scenarios 0,2,3,16 |
| thinking: redacted / signature / empty / text conversion | `transform_thinking` | `converts_thinking_blocks...`, `keeps_redacted_thinking...`, differential 8-10 |
| cross-model `thoughtSignature` removal | `transform_tool_call` | `removes_thought_signatures_when_migrating_between_models`; empty signature kept (differential 14) |
| cross-model text drops `textSignature` | `transform_assistant` | differential 0/10/16 |
| second pass pending tool calls | `PendingToolCalls` | `adds_synthetic_tool_results...`, `adds_synthetic_results_only...` |
| skip `error`/`aborted` assistant messages | `insert_synthetic_tool_results` | `skips_errored_and_aborted_assistant_messages_after_flushing_pending_calls`; differential 11-12 |
| user interrupts tool flow | `insert_synthetic_tool_results` | `user_messages_interrupt_tool_flow_with_synthetic_results`; differential 13 |
| synthetic result shape (`No result provided`, `isError`, `now`) | `PendingToolCalls::insert_synthetic` | `adds_synthetic_tool_results_for_trailing_orphaned_tool_calls` |

## Boundary

GAP (2026-10-25): intra-call live-clock movement uses the approved single native `now` snapshot and existing synthetic-time normalization; undeclared roles/content and out-of-contract callback outputs follow the typed/pathological input exclusion, without new carriers or APIs.

Owned return values follow the general 2026-10-22 approval. The six frozen conversion consumers (Anthropic, Completions, Responses-shared, Google-shared, Bedrock and Mistral, covering nine APIs) read the transformed sequence into new wire-request structures; no built-in consumer relies on mutation through a returned message reference. Private maps/sets are used for lookup only, so native ordered-tree representation does not change output order.

## Discrepancies
- None. Source ordering is preserved: ID mappings are built during the first pass, stray tool results are kept, and only pending tool calls receive synthetic results.
