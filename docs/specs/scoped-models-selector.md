# Scoped model selection

Authority: complete common-pin `modes/interactive/components/scoped-models-selector.ts` ([lock](../porting/source-lock.json)).

- Keep configured IDs as either all-enabled (`null`) or an explicit ordered UTF-16 list. Build visible rows only for IDs with a model; missing IDs still contribute to configured count. Duplicate model keys retain the last model record while the available-ID array retains duplicates.
- Preserve the literal toggle, enable-all, clear-all, move and sorted-ID operations. Enable-all collapses to `null` by resulting length alone, even with missing/duplicate IDs; this frozen discrepancy is not repaired.
- Search reads the actual shared Input, including changes made through its getter. Retain fuzzy search candidates, model order, eight-row scrolling, wraparound navigation, selected model name, enabled marks and dirty/save hints.
- Resolve every app/select action through shared keybindings. Ctrl+C independently clears a nonempty live query or cancels when empty. Forward all remaining input to the shared Input and refresh. Focus propagates to that same Input.
- Change/persist callbacks receive independent copies of configured IDs. Invoke synchronously at the frozen point without awaiting a callback result; asynchronous work belongs to the callback. Preserve synchronous failures, including persist failure before dirty-state reset. Frozen built-in callbacks change session/settings or close the selector; they do not mutate its model records.
- Inherit Container's live children, render, add/remove/clear and invalidation behavior. Keep typed handles to the same Input/list/footer children; no parallel cached search authority or silent downcast failures.
- A filtered reorder may decrement the visible selected index below zero. Retain that signed state and fail explicitly with `TypeError: Cannot read properties of undefined (reading 'model')` after the partial list update and before footer refresh/change notification. Do not clamp or repair it. Without a catch, the native process reports the same diagnostic and exits 1; JS stacks are omitted under the approved rule.
- The frozen component has no `updateModels` or refresh-status API. Catalog refresh/loading belongs before construction, not an alternate selector implementation.

[Traceability and proofs](../porting/traceability/interactive-scoped-models-selector.md).
