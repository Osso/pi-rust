# Core UI-local forms and overlays

Source pin: `532f7d12`; working base: `04f51ba`. Native contract: [spec](../../specs/core-ui-overlay.md). Historical API RED evidence: [batch M](extension-examples-m.md). Tests run only through Cargo's mandatory sandbox; no live state, providers, model CLIs, new dependencies, unsafe thread traits, or new TLS registry.

## API / ownership coverage

| Exact source contract | Rust representation | Behavioral proof |
|---|---|---|
| `question.ts:73–89,115,193`; `questionnaire.ts:110–130,199,342`: factory creates/owns real Editor | `ExtensionUiComponent` has no Send/Sync bound; adapter owns `Rc<dyn ExtensionUiComponent>`. Factory closures remain Send/Sync; outputs do not cross the request channel. | `extension_ui_local_editor_nested_overlay_and_handle_callback` owns `RefCell<Editor>`, types Unicode through the real interactive adapter and receives the submitted value. Ownership mutation fails compilation. |
| `interactive-mode.ts:3471–3479`: save editor text before factory construction; restore on non-overlay completion only | `show_extension_custom` captures text before invoking the factory; Done restores editor then saved text. Synchronous factory failure still preserves its existing rejection ordering. | `extension_ui_custom_restores_text_saved_before_construction_only_without_overlay`: real capture-terminal host, factory writes `factory`, UI request writes `during`, Enter completion restores `before`; overlay retains `during`. |
| `interactive-mode.ts:3486–3505`: done closes/resolves/disposes once | `ExtensionUiDone` callback is UI-local; its weak component slot uses `Rc`/`RefCell`. Existing `with_mode` dispatch is reused, not replaced by a component registry. | Existing `extension_ui_overlay_uses_component_width_and_closes_once`, explicit-options/synchronous-close and focus-restoration tests remain passing. |
| `tui.ts:439,770–862`: `showOverlay(component, options)`; QA calls `957,1057,1278` | `ExtensionUiTui::show_overlay`, matching required `ExtensionUiTuiBackend::show_overlay`; native handle adapter delegates directly to pi-tui. | `extension_ui_nested_overlay_options_input_and_passive_focus`: width 23, keyboard delivery, non-capturing overlay, explicit focus/unfocus, hidden/visible focus, removal of only the addressed entry. Dropped-options mutation killed. |
| `tui.ts:861`: `isFocused`; QA `1107,1419` | `ExtensionUiOverlayHandle::is_focused` queries native handle. Handle is UI-local, not Send/Sync. | Both nested-overlay tests; constant-false mutation killed. |
| `interactive-mode.ts:3518–3520`; QA `246–260`: expose handle immediately after mount | `custom` invokes existing options `on_handle` after mounting. | `extension_ui_local_editor_nested_overlay_and_handle_callback` observes focused handle and hides/restores it; omitted-callback mutation killed. |
| Same onHandle timing, but callback must retain UI-local handle | Approved native bridge: default `ExtensionUiComponent::on_overlay_handle`. Called once by `custom` immediately after mount, before optional shared options callback; raw `show_overlay` does not invoke this custom-only hook. | `extension_ui_local_on_handle_retains_handle_for_later_timer`: `Rc<RefCell>` retains handle; later local timer hides/restores it, checks focus and ordered `hook → option → hidden → shown → disposed`. Omitted-hook mutation killed. |
| Render-only timers may request frames across threads | `ExtensionUiTuiBackend` and render request sender retain Send/Sync. | Existing snake, rainbow, border-status and other timer examples pass unchanged behavioral assertions. |

No new overlay positioning rules or focus algorithms were introduced: native pi-tui owns those contracts. Both regular/fullscreen Editor fixed-cell tests remain passing.

Local callbacks capturing Done now use the existing UI LocalSet (`llama/ui.rs`, `qna.rs`, `handoff.rs`). Fixtures that previously stored a boxed component in Send UI state now keep it on an owned thread and communicate through messages. Paused fixture runtimes start paused at construction, preserving the timer wheel origin; pausing after construction produced a one-tick drift, recorded in RED logs 21–24. No assertion was weakened.

## `question.ts` whole-module map

Native: `examples/extensions/question.rs`, shared `forms.rs`; opt-in `question_extension(&ExtensionApi)`. No default registration.

| Source symbols / branches | Native items | Proof |
|---|---|---|
| Default export, Option/Question schemas, sequential execution metadata | `question_extension` | Full metadata/schema differential, including exact descriptions and execution mode. |
| Execute: no TUI, empty options, cancel, selection/index, custom answer/details | `execute`, `result` | `question_all_branches_keys_editor_renderers_and_widths_match`: exact content/details in all outcomes. |
| Factory: bounds, select Other, editor typing/submit, blank returns to list, editor Escape returns, list Escape cancels | `QuestionForm`, `State`, `FormEditor` | Up/down boundaries, repeated Down, Enter, blank, discarded text/back, custom Unicode and cancellation key scripts. |
| Refresh/caching and wrapped render, prefix wider than viewport, descriptions, editor/footer text | `refresh`, `render`, `wrap_prefixed`, `option_lines`, `editor_lines` | Full ANSI frames and render-request counts at widths 1/2/7/24/80. Widths 1/2 use ASCII editor input because of the separately recorded core defect below. |
| `renderCall`, `renderResult`: cancelled/custom/index/not-found and missing-details text/non-text/empty content | `render_call`, `render_result` | Full Text-component output at narrow/wide widths; explicit not-found fixture plus all fallback variants. |

35 complete pinned-TS differential requests plus one source-only RangeError characterization. TS oracle loads the actual pinned example and Editor/TUI, not a reimplementation. Selection-index and blank-refresh mutations are killed. A new request-count assertion first exposed native `4` versus TS `5` requests on blank submit (log 46); native now preserves both source refresh calls.

## `questionnaire.ts` whole-module map

Native: `examples/extensions/questionnaire.rs`, shared `forms.rs`; opt-in `questionnaire_extension(&ExtensionApi)`. No default registration.

| Source symbols / branches | Native items | Proof |
|---|---|---|
| Default export, schemas, private `errorResult`, no UI/no questions | `questionnaire_extension`, `execute`, `error`, `result` | Full metadata/schema/error content/details differential. |
| Normalize label/allowOther defaults; single/multi layout, Submit tab, incomplete/complete submission | `execute`, `Questionnaire::{options,all_answered,render_tabs,render_submit,submit}` | Single/multiple questions; missing labels, explicit false; Tab/right/left/Shift+Tab wrapping, incomplete Enter and cancellation, completed summary/submit. |
| Option bounds, save/advance, insertion-ordered replacement, partial cancellation | `handle_input`, `advance`, `State.answers: IndexMap` | Revisit/replace first answer after answering second; returned answer order remains first insertion order. Partial answers survive cancel. |
| Custom editor, blank `(no response)`, back/discard, input id, refresh/cache | `input_editor`, `FormEditor`, `refresh` | Key scripts compare complete details, frames and render-request counts. Blank-answer mutation killed. |
| Wrapped form, tabs, answered markers, summaries, descriptions and helpers | `render`, shared wrapping helpers | Full ANSI frames at widths 1/2/7/24/80, including narrow-prefix branches and cache reuse. |
| Both renderers; cancelled/custom/index/no-index and missing-details variants | `render_call`, `render_result` | Full Text output including manually supplied no-index answer and text/image/empty fallback content. |

33 complete pinned-TS differential requests plus one source-only RangeError characterization. The approved post-pin case below has a separate native behavioral test, not a pinned comparison.

### Approved post-pin optionless correction

User chose TS root fix on 2026-10-20. TS commit `9f1ac334512d9670dbd9b171e1ee8bf2948800d4` changes questionnaire Down to clamp at zero and makes Enter return when no option exists. Rust does exactly that; the draft pinned-TypeError branch was removed before committing questionnaire. [Compatibility decision](../../specs/compatibility.md).

`empty_options_down_enter_are_no_ops_and_escape_cancels` drives Down → Enter → Escape through the real form with `options: []`, `allowOther: false`: four identical rendered snapshots, empty answers, cancelled result. RED log 41 throws at Enter; GREEN log 47 passes. Reintroducing the TypeError kills this test (48). This corrected case never runs against the pinned oracle.

## String boundary / source defects

- Direct existing String APIs project lone UTF-16 surrogates to U+FFFD per [compatibility](../../specs/compatibility.md#out-of-scope). Both form tests submit input units `[55296]` and assert answer `�`. Oracle applies `toWellFormed()` before the corresponding Editor call, at the same native String boundary; ordinary frames/results are not normalized beyond that boundary.
- Both pinned forms cache render output without a width key; tests compare consecutive width renders with that cache intact. Preserve-cache classification was explicitly approved; `invalidate` clears it. This is not silently repaired.
- **Confirmed existing shared Editor defect, not an approved gap:** a width-2 indivisible grapheme (e.g. `🚀`) at layout width 1 recursively re-wraps itself forever. TS `packages/tui/src/components/editor.ts:165–171` produces `RangeError: Maximum call stack size exceeded`; source-only oracle cases prove it for both forms. Rust `crates/pi-tui/src/components/editor.rs:2765–2766` aborts its stack, captured in logs 13–15/18. No Editor code changes in this slice. Main assigned the TS/Rust correction to a separate slice. ASCII Editor input is tested at widths 1/2; wide graphemes at width ≥7.

## Later `overlay-qa-tests.ts` batch

The module is still **todo**, not partially ported. Static API audit found no remaining missing operation among its thirteen commands after this slice; this is an API-availability claim, not behavioral proof of those commands.

| Commands / TS registration lines | Newly available or existing required operations | Remaining work / proof |
|---|---|---|
| animation 32; anchors 43; margins 81 | UI-local component state/timers; options/anchors/margins and render requests | Entire examples/timers/key behavior remain unported and untested. |
| stack 115; overflow 159; edge 170 | Concurrent `custom`; options; local Done; native process/timer disposal | Entire scenarios remain unported and untested; no streaming subprocess spawned here. |
| percent 181; maxheight 217; sidepanel 228 | Percentage sizing, maxHeight, visible predicate, existing render/input | Entire scenarios remain unported and untested. |
| toggle 246; passive 264 | Shared/local onHandle; retained local handles; nested `show_overlay`, non-capturing option | API behavior tested in core; scenario counters/timer/controller cleanup remain unported. |
| focus 276; streaming 288 | Nested handles, `focus`, `unfocus`, `is_focused`; local controller/input state | API behavior tested in core; multi-panel rendering/focus loop/streaming remain unported. |

## Proof / verification exceptions

Full commands, revisions and outcomes: `/tmp/pi-rust-core-ui-overlay/ledger.md`; numbered logs beside it. Core API compile RED is log 01; form module/API RED is 09; local handle bridge RED is 35; optionless and request-count behavioral RED are 41/46. Nine isolated mutation checks killed their mutants and restored original bytes.

- All eight `extension_ui_*` tests emitted GREEN, including the three new real-adapter tests.
- All 50 existing `example_*`/editor targets constructing these traits pass, using non-redundant combined evidence from logs 21 and 25. Footer/llama targeted regression targets pass (07/08).
- All 83 tests in changed `interactive_mode.rs` were attempted (40). Eighty emitted `ok`; three hung, also individually at a hard 60 seconds (42–44): `command_restart_always_restarts_the_process_and_propagates_errors`, `entry_appended_renders_registered_custom_entries`, `render_session_entries_renders_persisted_custom_entries`.
- Main reports these three were reproduced at base `04f51ba` by the core-ui-input slice and assigned to a separate fixer. They are not passing evidence. No restore/bisect or unrelated fix was performed here. The hung process tree was cancelled and all owned processes verified exited. No broad rerun.
- No Cargo check, Clippy, workspace suite, deployment or live smoke test was run, per this task's focused-test-only instruction. Independent/final integration verification belongs to main; this slice does not claim a globally clean milestone.

## UI-local ownership / Clippy follow-up

`ExtensionUiDone` and overlay handles use `Rc`; local callback slots use
`Rc<RefCell<_>>`. `ExtensionUiOnHandle` still crosses threads as a Send/Sync
`Arc` callback, but receives an `Rc` handle only on the UI thread. Public custom
factories and `ExtensionUiTuiBackend` retain Send/Sync `Arc` ownership; direct
UI-thread factories use `LocalExtensionUiCustomFactory`.
`extension_ui_done_clones_share_local_callback_state` checks retained callback
state and calls after dropping the original wrapper. Existing overlay tests
cover focus, nested handles, disposal, and first settlement without changed
assertions. Follow-up proof: `/tmp/pi-rust-clippy-final/`.

## Async/target fidelity proof

`/tmp/pi-rust-ui-fidelity/ledger.md` retains command, exact revision/diff and
RED/GREEN/mutation logs. API RED (`async-unfocus-api-red.log`) proves the old
signature cannot express future factories or explicit unfocus targets; it is a
compile rejection, not a runtime failure. `retained-done-red.log` is behavioral
RED for rejection/closure separation. `restored-core-green.log`: **17 passed**.
`async-unfocus-mutations.log`, `async-rejection-null-mutations.log` and
`retained-done-mutation.log` reject missing early-Done guard, ignored explicit
target, null-as-omitted, lost rejection text restoration and response-as-closed.
All mutations were restored byte-for-byte.

Fifteen example factories and the bundled llama factory use
`ready_custom_component`; fixture hosts consume that same future. Immediate-only
fixture seams explicitly require readiness; the production host awaits arbitrary
UI-local futures. After merging master, `migrated-targets-{1,2,3}.log` cover all
**30 affected targets, 77 tests**, with unchanged behavior assertions. Expected
invalid-regex/missing-path stderr belongs to `example_truncated_tool`'s passing
error fixtures. `package-clippy-fixed.log` passes package `--all-targets -- -D
warnings`; subsequent changes are oracle readability/docs only. No workspace
suite, new dependency, spawned agent, model CLI or unsandboxed oracle ran.

## Synchronous custom-factory rejection (batch Q prerequisite)

Pinned `interactive-mode.ts:3483–3505` evaluates `factory(...)` inside the
Promise executor. A synchronous throw rejects directly: the later chained
`.catch` at `:3529–3533` is never attached. No overlay hide, editor/focus
restoration, component mount, options/handle callback, or disposal occurs.
`close` at `:3487–3503` settles first when called before that throw.

`ExtensionUiCustomFactory` returns `ExtensionUiCustomResult`: an outer `Result`
for synchronous throws, containing a UI-local future whose `Result` models
fulfillment/rejection. `ready_custom_component` migrates synchronous callers
onto that same future path. Factories remain Send/Sync; futures and components
stay on the owning LocalSet. `show_extension_custom` saves text before calling
the factory and awaits settlement before the common mount helper.

`extension_ui_async_factory_settlement_orders_mount_restore_and_early_done`
drives actual UI requests through the capture-terminal mode for eight cases:
fulfill/reject/Done-before-fulfill/Done-before-reject, each with/without overlay.
Pending factories keep the editor visible/focused; non-overlay rejection and
Done restore saved text, overlay paths retain edits; late settlement cannot
mount, invoke onHandle or restore twice. A fulfilled component closes through
real terminal Enter input; captures prove pending editor visibility and that only mounted components receive dispose. Existing synchronous-throw tests retain their ordering.

Pinned `.catch` rejects without setting `closed`. `extension_ui_retained_done_after_rejection_still_closes_once` retains Done across both synchronous/asynchronous rejection, then checks non-overlay saved-text restoration or overlay hide, and guards the second close. Separate `Cell<bool>` tracks Done closure state; consuming the response sender is not treated as closing the component.
No loadable whole-interactive-mode TS oracle exists; these are native host
proofs against exact pinned `interactive-mode.ts:3471–3534` statements.

`ExtensionUiOverlayHandle::unfocus_to(Option<pi_tui::tui::ComponentRef>)`
bridges directly to native pi-tui, without a new component registry.
`extension_ui_unfocus_distinguishes_omitted_explicit_target_and_null` checks
previous-overlay restoration, explicit editor target, null focus, and no-op on
an unfocused handle (`tui.ts:257–276,828–858`).

Behavioral proof through the real `InteractiveUiContext` request adapter:
`extension_ui_factory_error_rejects_custom_without_changing_focus_or_overlay`
and `extension_ui_factory_done_before_error_keeps_first_settlement`.
RED: `/tmp/pi-rust-ext-examples-q/01-core-red.log` rejects both new `Err` factories
at compile time against baseline `7769ff0c`; subsequent commands/results live
in that directory's `ledger.md`.
GREEN at `8712f86a`: `extension_ui_` 12/12, full `interactive_mode` 87/87,
and all 28 affected example/fixture targets pass (logs 03–05).
