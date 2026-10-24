# print-mode.ts

Frozen `4f9e6f8a4`, `packages/coding-agent/src/modes/print-mode.ts` (179 lines).

| Frozen surface | Native mapping | Proof |
|---|---|---|
| assistant-narration import12; calls38 and161–162 | existing get_assistant_content_text import and both call sites retained; signed/public thinking remains visible | existing public signed-narration process differential |
| findLastPrintableAssistant32–42 | scan final session messages; skip successful end_turn results and end_turn-only assistant messages; stop on other roles, failed results or empty assistant barriers | actual pinned-process final-state/barrier fixtures GREEN in print-mode-whole-green.log |
| text result153–164 | final-state printable prose, Error/Aborted stderr/exit1; no cached message or end-turn reason output | old end-turn-only fixture RED (source empty/native pi-monorepo); process final-text/error/narration GREEN |
| JSON import16 / subscription129 | complete toJsonPrintModeEvent shaping only for print | common-pin JSON28-fixture differential and real process JSON/DoneReason proof |
| prompt sequencing, bind/rebind, disposal/flush | runtime + extension/context bindings, owner-tagged subscription | process sequencing, catch/finally, cleanup rejection, reload and signal matrix; replacement JSON key-order pending below |

Source has no completed-assistant/reason caches, getEndTurnReason or findLastEndTurnReason. These old-only paths are deleted, not kept as fallbacks. Source helper usages remain literal; assistant_narration.rs is unchanged.

The harness compares complete stdout/stderr/exit after only approved id/timestamp/duration/path substitutions. It now gives both runtimes valid frozen builtin label=end_turn/executionMode=sequential; the earlier unknown/as-never undefined-label fixture was invalid typed input. No provider usage, date or text is normalized. Whole-module acceptance includes the explicit2026-10-24 native AssistantMessage order approval below; no generic JSON normalization is applied.


## Queue9 revalidation (2026-10-24)

Whole frozen179-line scope. Existing process target8/8 GREEN atc0d8f420 (`print-mode-baseline.log`), closing the previously unrun basic final-state barrier gate, not yet signals/lifecycle. [Contract](../../specs/print-mode.md).

| Frozen branch | Classification / proof |
|---|---|
| `if(initialMessage)` :145 | **DISCREPANCY:** empty initialMessage with images is ignored by source. Actual-source/native process RED `print-mode-empty-images-red-final.log`: source no output, native prints the image-only response. Native now uses the same nonempty guard; no trim and no TS source edit. Process GREEN. |
| retained rebind closure after runPrintMode returns | Approved2026-10-22 owned-values rule; parent2026-10-24 confirms only builtin consumer is main's one-shot exit, so no post-return probe required. Native Weak upgrade failure is commented unreachable from built-in consumers, not treated as a silent generic fallback. |

No Source beforeInvalidate/reload/disposal/signal conclusions are inferred from the8 basic tests. Explicit process REDs remain the next step.


## Cleanup and catch carriers

`print-mode-dispose-red.log` runs real runtime beforeInvalidate rejection: source runPrintMode rejects to its caller (captured TypeError/code/raw UTF16), native exited1 through uncaught. `print-mode-signals-reject-red.log` proves both signals: source143/129, native1 with erroneous diagnostic. `print-mode-catch-finally-red.log` proves raw prompt rejection became blank native stderr, and covers catch String(object) throwing plus disposal override in text/JSON modes. Source fixtures use actual AgentSession cwd-recovery/runtime hooks; no print-mode method is replaced.

Native run_print_mode now returns Result<i32,CallbackError>; dispose preserves its original Result, body catch uses the shared Error.message/String(value) authority, and the body outcome is retained while finally runs. A cleanup rejection overrides the body outcome and skips flush, matching source. Signal's source finally exit deliberately ignores disposal rejection, without reporting it. Main and the process harness are mechanical Result adapters (parent-approved exact main range). No panic/catch_unwind control flow added. Registration/lifetime/repeated-signal and replacement-subscription proofs are recorded below; this paragraph alone is not whole-mode acceptance.

## Reload callbacks and old-context lifetime

Actual inline extension commands use real print bindings and private gated ResourceLoaders, not replacements for print-mode methods. `print-mode-reload-action-red.log`: source returns the original ordinary Error with raw UTF16 message to the caller; native swallowed it and printed stderr. `print-mode-reload-fifo-red.log`: source's queued reload rejects originally; native's prematurely invalidated context silently resolved. `print-mode-reload-success-context-red.log`: source old-context `isIdle()` returns true after successful reload; native rejects it as stale. These logs are distinct REDs, not GREEN evidence.

Shared print/RPC reload actions now await `runtime.reload`, preserving FIFO and CallbackError through HandlerError and the native command wrapper. Defaults return Ok; inactive-context reads return their real error. The reload example and test/UI adapters are mechanical Result consumers. ResourceLoader's production trait is unchanged. Native AgentSession.reload's extra invalidation is deleted: frozen invalidates at dispose (:2271), not reload (:5617–5635) or _buildRuntime (:5551–5611). **DISCREPANCY:** the stale-context diagnostic advertises reload invalidation that the frozen implementation does not perform. Parent approved literal deletion on2026-10-24. Pending/failure and success contexts are separate process probes; all three GREEN in print-mode-reload-green.log and print-mode-whole-green.log.


## Queue9 final module matrix (2026-10-24)

`print-mode-whole-green.log` at d5055540: **18/19 tests GREEN** before the key-order ruling; the remaining rebind test is GREEN in `print-mode-approved-order-green.log` after the explicit2026-10-24 approval. Composite **19/19 GREEN**, without rerunning unrelated passing cases. The 19 tests run actual pinned TS and native private processes, without live providers. Earlier behavioral REDs: `print-mode-rebind-behavior-red.log`, `print-mode-early-signals-behavior-red.log`, `print-mode-repeated-signal-bounded-red.log`.

| Frozen lines / feature | Native behavior / proof |
|---|---|
| 49–64 state/dispose once | cleanup original errors propagate; body/coercion rejection overridden by cleanup; text+JSON rejection capture and beforeInvalidate traces GREEN |
| 66–85 signals before binding | streams constructed synchronously, registration failure returned explicitly; real session_start raises TERM/HUP before yielding; text+JSON exits143/129 GREEN |
| 70–78 persistent signal callback | same and mixed second signals during real pending session_shutdown exit with second signal code; first shutdown task is independent of listener removal; four private-process cases GREEN |
| 87–132 rebind and command actions | owner-tagged `(AgentSession,id)` unsubscribes original owner, not replacement ID; text-mode replacement session+agent observer marker proof GREEN; old-only JSON AgentEvent/backpressure observer deleted (parent cleared); JSON replacement and both observer markers GREEN under the approved native AssistantMessage wire-order contract below |
| 135–143 JSON header/bind | header before extension-start signal, complete JSON event stream ordering GREEN |
| 145–151 prompts/images | empty initial message+image discrepancy reproduced; follow-up order and last-result-only output GREEN |
| 153–172 text/catch | barriers, end_turn-only messages, signed/public narration, provider errors, raw UTF-16 and throwing String coercion GREEN |
| 173–179 finally/flush | signal listeners removed before dispose; original cleanup error overrides pending return; complete large JSON output (>100KB), content and every ordered line match source; no separate controlled-backpressure barrier probe claimed |
| retained rebind closure | approved owned-values exclusion; Weak branch comment retained; only built-in consumer exits after one-shot mode |

**Approved deviation2026-10-24 (AGENTS.md, parent00ae05f6):** native AssistantMessages use one fixed key order in print-mode JSON and RPC event output, extending the prior JSONL approval. Frozen exhausted faux messages put `errorMessage` before `timestamp` (`faux.ts:277–288,450–462`); native puts timestamp first. The rebind JSON test compares exact parsed event values, asserts the native fixed AssistantMessage key sequence, and compares every field's serialized value to retain nested key order. Event envelope order and every non-AssistantMessage line stay exact. The matcher is enabled only for the rebind fixture's newly constructed assistant message_start/message_end events; historical records, arbitrary JSON and other test comparisons are not normalized. Both replacement session and agent observers now prove survival in text and JSON modes. No faux/types production edit. The earlier dedicated pi-ai source-order RED is deleted as moot under this ruling (parent authorized); its diagnostic artifact remains `print-mode-faux-order-red.log`. Audit: fauxAssistantMessage:82–93 declares errorMessage/responseId before timestamp; cloneMessage:265–274 and aborted:291–297 retain source insertion slots, now explicitly covered by the approved fixed native order.

Proof artifacts and exact revision/invalidation ledger: `/tmp/pi-rust-repin-lossless-interactive/print-mode-proof-ledger.md`. Module status is **done** with that explicit approved boundary. Controlled-backpressure flush timing is not separately proven; full large-output ordered bytes and finally/disposal branches are proven.
