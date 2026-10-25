# tree-selector.ts

Frozen: `coding-agent/src/modes/interactive/components/tree-selector.ts` at `4f9e6f8a4`.
Native: `crates/pi-coding-agent-ui/src/modes/interactive/components/tree_selector.rs`.

Baseline fixture isolation (2026-10-25): all case batches install process-global keybindings, including a remapped binding case. A shared async test lock now holds across each batch's exact native/oracle comparisons so parallel height/filter tests cannot render another test's key hints. Expanded mismatch diagnostics retain exact differing fields and frames.

## Symbol and branch map

| Frozen symbols / lines | Native representation | Proof |
|---|---|---|
| renderHorizontalViewport (:62–94) | fixed gutter, selected-anchor scroll, body-only clipping and ANSI reset | widths0/1/2/8/30/90/240; existing branching/fold sequences |
| FilterMode/TreeList constructor/findNearestVisibleIndex/buildActivePath (:97–198) | exact five modes; f64 selection; nearest visible ancestor; falsey empty leaf | numeric, metadata-leaf and empty-ID source sequences |
| flattenTree (:200–328), toolCallMap | iterative traversal, per-node identity cache, shared JS array sort, active-first roots/children and literal gutters | distinct roots sharing an ID, parent/branch corpus, tool result previews |
| buildFilterCandidates/shouldHideToolOnlyAssistant/matchesFilterMode/matchesSearch (:330–390) | frozen compaction-context source ordering, text/stop filtering, raw substring token matching | all five filters, actual user-only compaction context, entry-tag/long-search corpus |
| applyFilter/recalculateVisualStructure/recalculateLegacyVisualStructure/recalculateCompactedUserStructure (:392–594) | retained flat-node authority and filtered indices; fold reset and cursor/ancestor retention | filter/cycle/fold/search sequences, more than two visible pages |
| getSearchableText/extractContent/hasTextContent (:596–651,910–937) |200-unit UTF-16 cap on message/array searches, full custom-message string search, all-content text-presence check | text only beyond200 must not match capped searches; raw split surrogate and custom string/array contrast |
| getSearchQuery/getSelectedNode/updateNodeLabel/getStatusLabels (:655–693) | typed list handle/accessors, unit-native labels/timestamps, literal status suffixes | labels, clear/empty-label truthiness, all filters/time-toggle sequences |
| render/getEntryDisplayText/formatLabelTimestamp (:695–908) | all entry/role tags; truthy errors/titles/labels; shared Date parse/local fields; f64 JS round/number formatting | every entry tag including unknown; invalid/empty timestamp, raw content, negative ties/large tokens, exact ANSI frames |
| formatToolCall (:939–995) | shared JS truthiness/String/number parsing/formatting; frozen read-range addition/subtraction | JSON objects/arrays/numbers/bools/null/string values; zero end; non-callable toString TypeErrors; empty call ID; isolated HOME/USERPROFILE branch |
| handleInput/isFoldable/findBranchSegmentStart (:997–1153) | exact key priority, wrap/page/fold/filter/cycle/search/label routing and f64 NaN/Infinity propagation | numeric/nonfinite height, branch navigation, direct list input, control-input and empty-result sequences |
| SearchLine/TreeHelp/TREE_HELP_ITEMS/formatHelpKeys/compactRawKeys (:1156–1266) | raw query, source default ellipsis, semantic help chunks, shared-prefix and key-name compaction | custom/empty help bindings and narrow wrapping; no old-only copy item |
| LabelInput (:1269–1321) | typed live Input, focused cursor, trim/undefined submit, cancel and exact default truncation | full label lifecycle at widths0/1/2/8/30/90; raw submit/clear/cancel |
| TreeSelectorComponent constructor/focus/showLabelInput/hideLabelInput/handleInput/getTreeList (:1326–1423) | inherited Container, typed retained children, hide/show ordering, live focus state and100ms local-runtime timer | component and inherited-child frames; add/remove/clear/invalidate; live and drop-before-timeout source sequences |

## Proof ledger (2026-10-22)

Logs live in `/tmp/pi-rust-repin-lossless-interactive/`.

- `tree-selector-red.log`: full-source RED for Container/label focus/hide-show, heights, arbitrary argument JSON, falsey entry fields and large compaction counts.
- `tree-selector-label-green.log`: label/container lifecycle and default truncation; `tree-selector-height-green.log`: fractional/nonfinite height and selection.
- `tree-selector-arguments-green-attempt1.log`: JSON argument coercion, falsey error/title, f64 compaction text. Expanded exact TypeError envelopes cover own non-callable toString shadows; ls never coerces an unused pattern.
- `tree-selector-expanded-red.log` → `tree-selector-expanded-green.log`:200-unit search caps, falsey labels/timestamps and empty-leaf marker. `tree-selector-alias-red2.log` → `tree-selector-alias-green.log`: node identity, not entry ID, keys the source active-subtree cache.
- `tree-selector-home-red.log`, `tree-selector-empty-call-id-red.log` → `tree-selector-full-green.log` (+ `tree-selector-raw-green.log`, `tree-selector-active-focus-green.log`): shared source truthiness for HOME/USERPROFILE and tool-call lookup. Final dedicated corpus:53 source scenarios across five tests, including the environment child probe, full UTF-16 component/list/child frames, callback events, selected IDs, labels/search and exact thrown-error envelopes.

Source contains no copy feature. Deleted old-only CopyHandler/on_copy/copy_selected/set_on_copy/get_entry_copy_text, the interactive-mode callback/helper, the copy-only behavior test and the oracle's nonexistent property injection. Remaining frame/selection assertions were not relaxed. Existing Ctrl+X case now exercises ignored control input.

## Runtime/ownership boundary

Empty-tree construction schedules the source100ms callback on the UI Tokio LocalSet. The independent callback survives component drop and an earlier Escape; the old undriven poll API is deleted. Real130ms source/native tests cover both cases. Native drop releases its local UI callback cycle, not the independent timer.

Frozen getTreeList has no built-in consumers. Native returns its retained typed list; external mutation-through-returned-reference is not required beyond the approved owned-value boundary. Internal label updates/rendering retain one flat-node authority. The active-subtree cache uses borrowed node addresses only within flatten_tree; it never escapes those borrows.

Human fields and arbitrary argument JSON retain UTF-16. A non-scalar numeric string yields NaN rather than replacement or a scalar error. Typed FilterMode excludes invalid JS runtime tags per the standing typed-API approval; no valid numeric height is narrowed to usize.

`tree-selector-existing-green.log`:10 current behavioral tests GREEN. `tree-selector-existing-oracle-green.log`:10 existing source scenarios GREEN. Raw argument strings (path/pattern/numeric offset) retain unpaired units, with an invalid numeric offset producing NaN without replacement; active label focus toggles also match the source. `tree-selector-date-green.log` additionally proves valid same-day, same-year and other-year label timestamps against Node, alongside the invalid/empty timestamp cases. The original exact frame/selection assertions remain intact.

No source discrepancy identified. No async timer, chrono timestamp or inherited-container gap remains. Parent owns final readability/clippy/workspace gates; local all-target checkpoint `tree-selector-check-final.log` at dd37452e passed with0 warnings; subsequent coverage/trace changes are documentation-only.

Shared numeric authority (2026-10-24,43adc225): argument_number delegates to pi_js_compat::js_json::to_js_number, preserving shared TypeError escalation and exact UTF-16/Number semantics; private recursive converter deleted. Existing full frozen tree-selector differential GREEN: print-mode-tree-shared-green.log. Behavior-preserving consolidation, not a new semantic slice.
