# `components/custom-editor.ts`

Authority: whole 138-line [common-pin](../source-lock.json) module; [contract](../../specs/custom-editor.md).
Rust: `crates/pi-coding-agent-ui/src/modes/interactive/components/custom_editor.rs`.

| Export / branch | Rust representation | Proof |
|---|---|---|
| `CustomEditorOptions`; constructor DB truthiness | native options; empty DB path normalized to absent | `differential_custom_editor::empty_control_db_option_does_not_open_a_database` reproduces unwanted legacy import RED→GREEN |
| `CustomEditor extends Editor` | owned Editor plus native delegating `Component`, `Focusable`, `EditorComponent`, and `editor[_mut]` access | existing editor examples 7/7 compare actual frozen Editor/CustomEditor/TUI, render/input/focus; raw input/history/shortcut 1/1 |
| `onAction`, Map insertion order/replacement; remaining handler `!== false` | `IndexMap<JsString, Rc<dyn Fn() -> bool>>`; bool normalizes the source handled predicate | dedicated frozen action differential; four false-return cases RED→GREEN, replacement retains its insertion slot |
| extension shortcut before all app checks | deferred callback; resume rechecks actual editor state | mounted `AnyEditorHandle` callback re-entry RED→GREEN; shortcut consumes before paste/action, or changes text before empty-exit check |
| paste consumes with/without callback | `dispatch_consumed` | frozen cases with callback and without; callback re-entry |
| interrupt dynamic/registered/no handler; autocomplete | `dispatch_app_binding`; registered return ignored | frozen routing; actual combined autocomplete opens, first Escape cancels without callback, second invokes dynamic escape; native existing interrupt test |
| exit empty/nonempty; dynamic/registered/no callback | same dispatch branch | frozen false-return exit/dynamic exit; existing nonempty suppression + empty exit |
| remaining handlers fall through to next match/base Editor | `dispatch_action` plus `EditorInput::Callback`/`Resume` | 14 frozen routing scenarios include ordered text mutations, raw units, two false handlers then base insertion, and later true consumption |
| `addToHistory`, `recordNewestHistoryEntry`, `sameHistory` | inherited memory update, native history comparison, typed DB error | real frozen DB differential: trim, duplicate/empty no-op, cap, filtered migration, normal write, reopen; failed-write memory mutation and duplicate retry match frozen |
| `addToHistoryWithoutPersistence` | inherited memory-only update | memory-only entry absent after DB reopen in frozen differential |
| `loadPromptHistory`, legacy read/filter/slice, constructor filter | real file + real control DB, shared lossless JSON parse | raw legacy high unit plus 100-entry cap/filter differential; 8 legacy cases: absent/missing/directory/malformed/non-array/empty/non-string/invalid UTF-8 |
| history SQLite projection | existing `JsString` `ToSql`/`FromSql`; no new codec | exact stored hex and reopened history match frozen: JSON migration stores ED A0 80; direct lone-unit binding stores EF BF BD; malformed TEXT reads through shared Node decoder |
| inherited method failure propagates | `EditorComponent::add_to_history -> Result<(), EditorHistoryError>` | SQL-abort trigger: frozen inherited-method differential; native real message render throws, selected-agent steering catches/reports/restores submitted text (2 tests) |

## Native callback protocol

`EditorInput` is Complete, Callback, or Resume. `AnyEditorHandle` runs callbacks outside its `RefCell` borrow, then resumes under a fresh borrow; direct Component input drives the same protocol. Modal, rainbow and border-status composed editors retain their base CustomEditor downcast surface. Rainbow's post-input work follows the complete continuation chain, not merely its first callback. The old `Vec<Fn()>` path is replaced, not retained as a fallback.

`actionHandlers` consumers were audited in the frozen coding-agent tree. Only `CustomEditor.onAction` inserts, and interactive-mode copies handlers during editor replacement; no built-in handler deletes/reinserts entries during iteration or relies on shared map identity. Owned callback values follow the general approval. Registration replaces in place. Public Rust handlers represent the literal-false predicate as `bool`, not arbitrary JS return objects.

## Retained frozen behavior / scope

A failed history write leaves memory changed; retrying the same newest text skips persistence. Tests retain that ordering. Generic `ExtensionUiTuiBackend` is unchanged. Parent-owned final independent/readability gates remain separate from targeted lane evidence. Rendering/inherited Editor internals and interactive-mode as a whole retain their own traceability and approved proof boundaries.

## Proof ledger

Artifacts: `/tmp/pi-rust-repin-lossless-interactive/`.

- RED: `test-custom-editor-red-2.log` (lost false fallthrough, mounted shortcut borrow panic); `test-custom-editor-empty-path-red.log` (empty path migration); `test-custom-editor-history-persistence-red.log` (ignored inherited failure); `test-custom-editor-green-2.log` (raw migrated SQL TEXT rejected on reopen).
- GREEN: `test-custom-editor-green-4.log` (7 dedicated tests); `test-custom-editor-history-boundaries-green-3.log` (2 real host failure boundaries); `test-custom-editor-examples-green-1.log` (7 examples); `test-custom-editor-existing-green-1.log` (4 native history/routing); `test-custom-editor-raw-green-1.log` (1 raw native differential).
- These are scoped implementation proofs. Formatting changes do not invalidate them. Later relevant production changes require new scoped proof; final all-target check recorded separately by checkpoint SHA.
