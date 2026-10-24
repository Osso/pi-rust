# print-mode.ts

Frozen `4f9e6f8a4`, `packages/coding-agent/src/modes/print-mode.ts` (179 lines).

| Frozen surface | Native mapping | Proof |
|---|---|---|
| assistant-narration import12; calls38 and161–162 | existing get_assistant_content_text import and both call sites retained; signed/public thinking remains visible | existing public signed-narration process differential |
| findLastPrintableAssistant32–42 | scan final session messages; skip successful end_turn results and end_turn-only assistant messages; stop on other roles, failed results or empty assistant barriers | q15-print-final-state-red.log actual frozen process RED; four real-session initial-message fixtures; GREEN pending |
| text result153–164 | final-state printable prose, Error/Aborted stderr/exit1; no cached message or end-turn reason output | old end-turn-only fixture RED (source empty/native pi-monorepo), full process gate pending |
| JSON import16 / subscription129 | complete toJsonPrintModeEvent shaping only for print | common-pin JSON28-fixture differential and real process JSON/DoneReason proof |
| prompt sequencing, bind/rebind, disposal/flush | existing runtime + extension/context bindings | final-only, follow-up and error scenarios; signal-handler process branches remain unproven |

Source has no completed-assistant/reason caches, getEndTurnReason or findLastEndTurnReason. These old-only paths are deleted, not kept as fallbacks. Source helper usages remain literal; assistant_narration.rs is unchanged.

The harness compares complete stdout/stderr/exit after only approved id/timestamp/duration/path substitutions. It now gives both runtimes valid frozen builtin label=end_turn/executionMode=sequential; the earlier unknown/as-never undefined-label fixture was invalid typed input. No provider usage, date or text is normalized. Broader mode parity is not claimed until the focused gate is green.


## Queue9 revalidation (2026-10-24)

Whole frozen179-line scope. Existing process target8/8 GREEN atc0d8f420 (`print-mode-baseline.log`), closing the previously unrun basic final-state barrier gate, not yet signals/lifecycle. [Contract](../../specs/print-mode.md).

| Frozen branch | Classification / proof |
|---|---|
| `if(initialMessage)` :145 | **DISCREPANCY:** empty initialMessage with images is ignored by source. Actual-source/native process RED `print-mode-empty-images-red-final.log`: source no output, native prints the image-only response. Native now uses the same nonempty guard; no trim and no TS source edit. GREEN pending. |
| retained rebind closure after runPrintMode returns | Approved2026-10-22 owned-values rule; parent2026-10-24 confirms only builtin consumer is main's one-shot exit, so no post-return probe required. Native Weak upgrade failure is commented unreachable from built-in consumers, not treated as a silent generic fallback. |

No Source beforeInvalidate/reload/disposal/signal conclusions are inferred from the8 basic tests. Explicit process REDs remain the next step.


## Cleanup and catch carriers

`print-mode-dispose-red.log` runs real runtime beforeInvalidate rejection: source runPrintMode rejects to its caller (captured TypeError/code/raw UTF16), native exited1 through uncaught. `print-mode-signals-reject-red.log` proves both signals: source143/129, native1 with erroneous diagnostic. `print-mode-catch-finally-red.log` proves raw prompt rejection became blank native stderr, and covers catch String(object) throwing plus disposal override in text/JSON modes. Source fixtures use actual AgentSession cwd-recovery/runtime hooks; no print-mode method is replaced.

Native run_print_mode now returns Result<i32,CallbackError>; dispose preserves its original Result, body catch uses the shared Error.message/String(value) authority, and the body outcome is retained while finally runs. A cleanup rejection overrides the body outcome and skips flush, matching source. Signal's source finally exit deliberately ignores disposal rejection, without reporting it. Main and the process harness are mechanical Result adapters (parent-approved exact main range). No panic/catch_unwind control flow added. Registration/lifetime/repeated-signal and replacement-subscription audits remain open; this paragraph is not whole-mode acceptance.
