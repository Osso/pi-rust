# Renderer owner completions

Native scheduling boundary for frozen edit previews. [Edit proof map](../porting/traceability/edit.md).

## Contract
- [x] Completing preview IO must not mutate renderer state or invoke invalidate during the current synchronous render sequence.
- [x] Bind an optional completion executor to ToolRenderState. Interactive tool rows dispatch through the existing UiRequest owner channel; its drain polls completions on the UI owner loop.
- [x] Check the request key when the owner applies the completion, then update preview and invalidate in one task. Disposed UI owners use the existing no-op request boundary.
- [x] Tests that await previews bind a test owner and drain only on yield; the fixture asserts completion runs on its creator thread.
- [x] Public reusable HTML rendering accepts the same optional executor; callers observing after await bind one. Production one-shot export supplies None.

## Frozen owner-less export
`export-html/index.ts:184-230` renders calls/results in a synchronous loop. `tool-renderer.ts:86` supplies a no-op invalidate, and113-115 convert component lines to a string immediately. Later preview resolution cannot change those snapshots. None therefore has no async rerender target; no thread-timing fallback is installed.

The reusable HTML API's observable-after-await case is distinct: its owner executor applies preview state before the next observation, preserving matching-error suppression. `differential_edit_html_owner` compares both boundaries against the actual frozen renderer.

## Proof
`edit-render-lifecycle-red.log` proves native early invalidation. `edit-render-owner-green-2.log`:12/0 across direct lifecycle, reusable/snapshot HTML, six frozen on-disk preview fixtures and existing component behavior tests. The test-only transport is not a production queue.

## Scope
No new production queue, dependency or JavaScript scheduler implementation. Existing background IO remains; only completion application crosses the owner boundary. Whole edit module closure and async failure-path audit remain separate.
