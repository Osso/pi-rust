# UI-local extension forms and overlays

Native factories cross the extension/UI request boundary; their returned components remain on the owning UI thread. Source contracts: pinned `packages/tui/src/tui.ts:266–286,770–862`, `packages/coding-agent/src/modes/interactive/interactive-mode.ts:3457–3535`, and `examples/extensions/{question,questionnaire}.ts`. [Traceability](../porting/traceability/core-ui-overlay.md) records implementation and proof.

## What it must do

- [x] Custom factories return a UI-local future through one completion/mount path. Capture editor text before invoking the factory; mount only after fulfillment. Pending factories leave editor focus/content visible. Async rejection restores saved editor text/focus only for non-overlays, then rejects with its message.
- [x] Rejection settles `custom()` but does not mark the component closed: a retained first Done still closes/restores the editor or hides the top overlay; later Done calls do nothing.
- [x] Done before future settlement wins over late fulfillment/rejection: never mount, deliver a handle, dispose an unmounted component, or restore editor text twice. The factory future still runs to settlement.
- [x] Overlay unfocus distinguishes omitted target (restore prior eligible focus), explicit native `ComponentRef` target, and explicit null (clear focus); unfocused handles do not alter focus.
- [x] A synchronous custom-factory error rejects `custom()` unchanged, without mounting, focus/editor restoration, or disposal. If the factory calls `done` before failing, that first result wins.
- [x] Factory-created components may own the existing `Rc`-based Editor without unsafe thread traits or a new thread-local registry. Render-only TUI requests remain usable by Send timers.
- [x] Nested overlays preserve options, keyboard routing, passive focus, hide/restore, and native handle focus queries.
- [x] Overlay `custom()` delivers its handle immediately after mounting, once per invocation; non-overlay and synchronously completed factories do not deliver one.
- [x] A UI-local component callback can retain the handle and hide/restore it from a later local timer. The local callback precedes the optional shared options callback when both are supplied.
- [x] Question forms preserve selection bounds, inline Editor input, blank/back/cancel behavior, result details, renderers, wrapped ANSI output, and source cache invalidation.
- [x] Questionnaire forms preserve single/multiple navigation, defaults, insertion-ordered answer replacement, partial cancellation, custom/blank answers, summaries, and renderers on tested normal paths.
- [x] Existing String boundaries project unmatched UTF-16 surrogates to U+FFFD before Editor input.
- [x] User-approved post-pin optionless questionnaires keep index 0 on Down, ignore Enter without changing frames or answers, and still cancel on Escape.

## How it works

- [Ownership, API/source map, and proof matrix](../porting/traceability/core-ui-overlay.md).

## Implementation inventory

- `crates/pi-coding-agent/src/core/extensions/types.rs`: UI-local component/Done/handle contracts; nested overlay method and local handle callback.
- `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs`: native TUI adapter and custom lifecycle.
- `crates/pi-coding-agent/examples/extensions/forms.rs`: existing Editor integration and shared form wrapping.
- `crates/pi-coding-agent/examples/extensions/question.rs`: opt-in question registration, form, and renderers.
- `crates/pi-coding-agent/examples/extensions/questionnaire.rs`: opt-in questionnaire registration, form, and renderers.

## Tests asserting this spec

- `crates/pi-coding-agent/src/modes/interactive/interactive_mode.rs` (`ui_tests::extension_ui_*`).
- `crates/pi-coding-agent/tests/example_question.rs`.
- `crates/pi-coding-agent/tests/example_questionnaire.rs`.
- `oracle/example-forms-oracle.ts` executes the pinned examples and real TS Editor/TUI.

## Known gaps (current cycle)

- [ ] Shared existing Editor recursion for width-2 graphemes at layout width 1 remains a confirmed defect assigned to a separate corrective slice, not an approved gap.

## Out of scope

- Porting the thirteen `overlay-qa-tests.ts` commands; this slice supplies their previously missing core operations.
- Dynamic TypeScript extension loading, model/provider calls, deployment, or live Pi configuration.
- Invalid JavaScript runtime input types beyond the approved typed-native boundary.
