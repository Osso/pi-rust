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
