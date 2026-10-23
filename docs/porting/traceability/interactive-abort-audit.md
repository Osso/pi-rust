# Q13 tui/interactive AbortSignal audit

Frozen `4f9e6f8a4`; audited 2026-10-22. Shared carrier: `pi_js_compat::abort` only. No CancellationToken remains in these consumers.

| Frozen consumers/producers | Native behavior | Evidence |
|---|---|---|
| tui cancellable-loader.ts:14–35; interactive bordered-loader.ts:10–62 | default DOMException producer; retain signal identity, abort-before-callback, repeated callbacks without repeating abort events; plain loader never aborts | [loader traceability](cancellable-loader.md); bordered differential ten scenarios, including add/remove/clear while retaining private loader behavior |
| interactive login-dialog.ts:15,74–96 | stable controller signal; first cancel supplies default AbortError, repeated cancel leaves reason/event count unchanged | `differential_login_lifecycle`: four full-frame/lifetime scenarios now include sameSignal, exact reason and one-shot abort-listener snapshots; `/tmp/pi-rust-repin-lossless-interactive/login-abort-green.log` |
| tui editor.ts:2473–2505,2521–2546,2573–2589,2601–2608 | new controller per started request; abort on cancellation; previous task must finish before next provider starts; stale results ignored; completed controller detached | [editor RED→GREEN](editor.md#q13-abortsignal-request-ordering-2026-10-22); held provider future proves actual ordering, default reason and live signal propagation |
| tui autocomplete.ts:123–216,294–319,729–743 | aborted checks return no suggestions rather than throwing reason; fd abort listener kills process, removes listener at close; supplied false reason stays false | `autocomplete_close_abort`: real child exit before inherited stdout pipe closes; non-abort/default-abort/supplied-false abort all match TS; `/tmp/pi-rust-repin-lossless-interactive/autocomplete-reason-green.log` |
| interactive-mode.ts:1443,2232,7107 and share loader | timeout branded TimeoutError is retained even after fetch completion; extension context and login pass live signal; share cancels via bordered loader | existing [interactive-mode proof](interactive-mode.md); `install_telemetry_abort_timeout_matches_frozen` pending and immediate fetch branches already GREEN in prior Q13 ledger; do not rerun unchanged proof. `/login`/refresh/catalog caller sites remain Q15-owned and are not changed here. |

One observed native mismatch fixed: editor started the next provider before a cancelled previous provider settled. No semantic deviation or source discrepancy introduced. No claims about Q15 completion or agent-core provider internals.
