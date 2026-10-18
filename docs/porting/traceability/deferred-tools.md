# deferred-tools traceability

Source: `packages/ai/src/utils/deferred-tools.ts`.
Rust: `crates/pi-ai/src/utils/deferred_tools.rs`.
Tests: `crates/pi-ai/tests/deferred_tools.rs` (5 behavioral), `crates/pi-ai/tests/differential_deferred_tools.rs` (6 requests vs pinned TS).

## Representation
- `{ immediate, deferred }` → `SplitDeferredTools { immediate: Vec<Tool>, deferred: Vec<(String, Tool)> }`.
- TS `Map` insertion order is preserved with a `Vec` plus a `BTreeMap<String, usize>` position index. Re-inserting a duplicate name keeps the first position and replaces the value, like `Map.set`.
- `normalizeName` defaults to identity (`split_deferred_tools`) and is injectable (`split_deferred_tools_with`).

## Symbols → proof
| TS | Rust | Test |
|---|---|---|
| `splitDeferredTools` | `split_deferred_tools` / `split_deferred_tools_with` | all behavioral tests + differential |
| disabled path returns all unique tools immediate | `split_deferred_tools_with` | `disabled_splitting_keeps_every_tool_immediate`, `duplicate_tool_names_keep_first_position_and_last_value` |
| assistant tool-call names mark tools as used | `transcript_loaded_names` | `tools_already_used_before_the_result_stay_immediate` |
| `addedToolNames` on tool results defer unused tools | `transcript_loaded_names` | `transcript_loaded_tools_are_deferred` |
| custom normalizer | `normalize_name` parameter | `custom_normalizer_applies_to_tools_and_transcript_names` |

## Branch notes
- Names are resolved in message order; `usedNames` only contains assistant calls seen before the tool result.
- Only tools present in `context.tools` can be deferred; unknown added names are ignored.
- `context.tools` absent → empty split.

## Gaps
- None.

## Discrepancies
- None.
