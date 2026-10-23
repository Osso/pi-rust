# components/tree-selector.ts

Source: `coding-agent/src/modes/interactive/components/tree-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/tree_selector.rs`

Tests: `crates/pi-coding-agent/tests/interactive_components_c.rs` (tree_selector_*),
differential `treeSelector` cases in `differential_interactive_components.rs` (9 trees/input
sequences, exact ANSI lines and selected ids).

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
| `updateNodeLabel` (timestamp default) | `update_node_label` | labels differential |
| `getStatusLabels` | `get_status_labels` | `[user]`/`[all]` assertions |
| `render` (cursor, gutters, fold marker, path marker, label/time, content) | `Component::render` | differential |
| `getEntryDisplayText` (all entry types) | `get_entry_display_text` | differential + behavioral |
| `formatLabelTimestamp` (local time) | `format_label_timestamp` (chrono Local) | labels differential |
| `extractContent` 200-unit cap | `slice_utf16` | render tests |
| `formatToolCall` (read/write/edit/bash/grep/find/ls/default) | `format_tool_call` | differential tool result |
| `handleInput` (up/down, fold/unfold, page, confirm, cancel, filters, cycles, search, label edit/time) | `TreeList::handle_input` | differential input sequences + behavioral |
| `isFoldable` / `findBranchSegmentStart` | same-named methods | fold differential |
| `SearchLine` | `SearchLine` | differential |
| `TreeHelp` / `TREE_HELP_ITEMS` / `formatHelpKeys` / `compactRawKeys` | `TreeHelp`/`format_help_keys`/`compact_raw_keys` | narrow-width help test |
| `LabelInput` | `LabelInput` | label edit path |
| `TreeSelectorComponent` (layout, focus, deferred empty cancel) | `TreeSelectorComponent` | differential + poll test |

## Q13 common-pin audit (in progress, 2026-10-22)

Actual frozen tree-selector.ts contains no copy API, callback or help item. Removed old-only CopyHandler/on_copy/copy_selected/set_on_copy/get_entry_copy_text and interactive_mode copy_tree_selection integration. Deleted the old-only copy behavior test and nonexistent source-property injection; all remaining exact frame/selection assertions stay intact. Existing Ctrl+X scenario now proves the source's ignored control input. `interactive-components-current.log` established RED on the old copy help row; new `tree-selector-red.log` additionally reproduces Container, label focus/hide/show, numeric height, argument coercion, entry truthiness and large-token formatting differences. The whole module remains open.

Q13 label/container GREEN `/tmp/pi-rust-repin-lossless-interactive/tree-selector-label-green.log`: actual Container inheritance and typed TreeList/LabelInput/child containers replace erased downcasts. Show-label hides the tree, captures current focus, submit updates label then calls the callback before restoring the tree, and cancel restores it. Full UTF-16 component/child/list frames, raw label callbacks and widths2/8/30/90 match. Default `...` truncation is restored for list status, SearchLine and LabelInput; horizontal body clipping retains its explicit empty suffix.

Empty-tree constructor now schedules the actual 100ms local-runtime callback, retaining it independently of the component. The old undriven poll field/API is deleted. Real130ms tests cover live and drop-before-timeout cases; cancellation still fires after owner drop. Native drop releases the local UI callback cycle, not the independent timer. Frozen getTreeList has no built-in consumers; typed shared children stay live within the component under the general owned-value approval. No source gap is claimed for this timer.

Numeric height/index GREEN `tree-selector-height-green.log`: terminalHeight, maxVisibleLines and selection state use f64. Shared JS Math.min/max retain NaN/Infinity; NaN viewport loops emit no rows and NaN selection emits no select callback. Negative/NaN array indices are projected only at the final indexed lookup. Fractional height keeps frozen floor(height/2). Rust callers widen actual integer terminal rows at the sole boundary.

## Gaps

- `setTimeout(onCancel, 100)` for an empty tree is exposed as `poll(now)`; the caller drives it.
- Label timestamps use `chrono` local time; the differential runs both sides on the same host
  so the formatted time matches.

## Discrepancies

None found.

## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).

## Q18 native UTF-16 integration (2026-10-22)

Tree identity/label/search state and render frames use JsString. Native truncation, styling and helper key rendering no longer repair text before terminal composition. `lossless_interactive_selectors::tree_label_editor_keeps_units_through_input_and_render` is authored, unexecuted. No primary diagnostics at interactive6.
