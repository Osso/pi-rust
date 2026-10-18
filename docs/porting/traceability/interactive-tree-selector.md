# components/tree-selector.ts

Source: `coding-agent/src/modes/interactive/components/tree-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/tree_selector.rs`

Tests: `crates/pi-coding-agent/tests/interactive_components_c.rs` (tree_selector_*),
differential `treeSelector` cases in `differential_interactive_components.rs` (9 trees/input
sequences, exact ANSI lines, selected ids, copy payload).

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `renderHorizontalViewport` | `render_horizontal_viewport` | differential narrow widths |
| `FilterMode` | `FilterMode` | differential user-only/no-tools/all |
| `TreeList` constructor | `TreeList::new` | differential |
| `findNearestVisibleIndex` (parent walk) | `find_nearest_visible_index` | metadata-leaf tests |
| `buildActivePath` | `build_active_path` | path marker in render |
| `flattenTree` (indent/connector/gutter rules, active-first ordering) | `flatten_tree` | differential branching tree |
| tool call map | `tool_call_map` | toolResult `[read: ...]` test |
| `buildFilterCandidates` (user-only compaction context) | `build_filter_candidates` | compaction-view test |
| `shouldHideToolOnlyAssistant` | `should_hide_tool_only_assistant` | tool-only tests |
| `matchesFilterMode` | `matches_filter_mode` | differential filter cases |
| `matchesSearch` | `matches_search` | search differential |
| `applyFilter` (fold descendants, cursor preservation) | `apply_filter` | fold/search tests |
| `recalculateLegacyVisualStructure` | `recalculate_legacy_visual_structure` | differential branching tree |
| `recalculateCompactedUserStructure` | `recalculate_compacted_user_structure` | compaction view |
| `getSearchableText` (all entry types) | `get_searchable_text` | search cases |
| `getSelectedNode` / `getSearchQuery` | `get_selected_node` / `get_search_query` | differential |
| `copySelected` | `copy_selected` | copy differential |
| `updateNodeLabel` (timestamp default) | `update_node_label` | labels differential |
| `getStatusLabels` | `get_status_labels` | `[user]`/`[all]` assertions |
| `render` (cursor, gutters, fold marker, path marker, label/time, content) | `Component::render` | differential |
| `getEntryDisplayText` (all entry types) | `get_entry_display_text` | differential + behavioral |
| `formatLabelTimestamp` (local time) | `format_label_timestamp` (chrono Local) | labels differential |
| `extractContent` 200-unit cap | `slice_utf16` | render tests |
| `getEntryCopyText` | `get_entry_copy_text` | copy differential |
| `formatToolCall` (read/write/edit/bash/grep/find/ls/default) | `format_tool_call` | differential tool result |
| `handleInput` (up/down, fold/unfold, page, confirm, copy, cancel, filters, cycles, search, label edit/time) | `TreeList::handle_input` | differential input sequences + behavioral |
| `isFoldable` / `findBranchSegmentStart` | same-named methods | fold differential |
| `SearchLine` | `SearchLine` | differential |
| `TreeHelp` / `TREE_HELP_ITEMS` / `formatHelpKeys` / `compactRawKeys` | `TreeHelp`/`format_help_keys`/`compact_raw_keys` | narrow-width help test |
| `LabelInput` | `LabelInput` | label edit path |
| `TreeSelectorComponent` (layout, focus, deferred empty cancel) | `TreeSelectorComponent` | differential + poll test |

## Gaps

- `setTimeout(onCancel, 100)` for an empty tree is exposed as `poll(now)`; the caller drives it.
- Label timestamps use `chrono` local time; the differential runs both sides on the same host
  so the formatted time matches.

## Discrepancies

None found.
