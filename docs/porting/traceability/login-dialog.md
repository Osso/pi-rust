# login-dialog.ts

Frozen: `4f9e6f8a4`, `coding-agent/src/modes/interactive/components/login-dialog.ts`.
Native: `src/modes/interactive/components/login_dialog.rs`.

| Branch | Native contract | Proof |
|---|---|---|
| constructor/title, device code | native JsString labels and content | login-dialog full-frame differential |
| `showInfo(lines)` :182–190 | clear prior content, spacer, raw lines, spacer, closing hint; request render once | info→info replacement, waiting/progress and prompt differential |
| showPrompt/manual input/submit/cancel, :58–67,81–92,144–177 | native answers, displayed submitted text, cancellation error; overwritten resolver leaves older promise pending | `differential_login_lifecycle`, prompt-submit/Escape differential and raw error consumer |
| waiting/progress | exact dim text/hints | full-frame differential |

Removed old-only `show_details` and provider-message/links/optional-hint component API. Callers now supply explicit lines; component owns only frozen line-array presentation. The historical unsupported-action fixture now calls the actual frozen showInfo array API; complete frames/events remain exact assertions, not relaxed expected output. Former RED: oracle showDetails absent and native info layout mismatched frozen closing hint. AuthEvent message/link adaptation remains a caller concern, not a new LoginDialog API.

All prompt answers/labels/info remain JsString. Browser launch utility owns argv encoding. LoginError Display only encodes for Rust formatting. No fallback API retained.

## Abort reason audit (2026-10-22)

`differential_login_lifecycle` additionally compares stable signal identity, default DOMException AbortError/message/code20 and abort listener snapshots on every step. Repeated Escape keeps the original reason and emits only one abort event. GREEN `/tmp/pi-rust-repin-lossless-interactive/login-abort-green.log`; no production change needed. See [Q13 audit](interactive-abort-audit.md).

## Whole common-pin comparison / Q13 proof

Compared all 222 frozen lines, including focus propagation (:21–28), provider-title truthiness/nullish title (:41–43), input submission only with live resolver (:58–67), replacing every matching input reference (:81–85), repeated cancellation (:88–96), auth hyperlinks/browser launch (:102–119), device-code URL/code/no browser (:125–139), append-vs-clear behavior and exact hints (:144–211), global cancel routing (:214–221).

Q13 RED `test-login-lifecycle-red.log` reproduces empty-placeholder rows and superseded promise receiving synthetic channel-closed output instead of remaining pending. Native prompt future now maps only explicit submit/cancel; dropping/replacing its sender never invents cancellation. Empty instructions and placeholders are skipped as frozen truthiness requires.

GREEN `/tmp/pi-rust-repin-lossless-interactive/test-login-lifecycle-green-3.log`: 1/1 wrapper (+1 child probe), four complete-frame/event/promise-state scenarios: overlapping prompts, repeated cancel, empty optionals, manual input, focus, info replacement, waiting/progress, auth and device code. A private Python xdg-open stub receives four exact literal URL argv values (TS + Rust); no browser/network runs. `test-login-dialog-q13-green.log`: existing full-frame/title differential1/1; `test-login-dialog-raw-q13-green.log`: unpaired-unit prompt/display/answer1/1. The caller no longer translates channel closure into a synthetic Login cancelled result.
