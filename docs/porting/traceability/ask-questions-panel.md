# Ask questions panel

Frozen `packages/coding-agent/src/core/tools/ask-questions-panel.ts` at `4f9e6f8a4`; native `core/tools/ask_questions_panel.rs`.

| Frozen symbols / branches | Native | Proof |
|---|---|---|
| createAskQuestionsPanel / focused / finished-once | QuestionPanel, UI-local RefCell/Cell state | real custom-factory driver in ask_questions_tool |
| formatAnswer / hasAnswer / canSubmit | Answer::format/has_answer, can_submit | real frozen panel keystroke differential pending final pass |
| findFreeTextTabOffset / moveToTab / advanceAfterAnswer | edit_answer / move_to_tab / advance | app.questions.next/previous defaults added to core keybindings; focused differential pending |
| inputs submit/escape / activateRow / editAnswer / handleKey | corresponding private methods | free text, Other, single, multi, partial cancel fixtures |
| renderTabBar / renderOptionRows / renderSubmitSummary / renderBody / renderHelp | rendering methods | native render exercised, exact render differential not yet established |

Source `app.questions.next` defaults right/tab; previous left/shift-tab. Free text retains left/right editor navigation. Multiple questions or multi-select require a Submit tab; multi-select may submit an empty answer. Selected options serialize in source option order, not toggle order.
