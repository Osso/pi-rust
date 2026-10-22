# login-dialog.ts

Frozen: `4f9e6f8a4`, `coding-agent/src/modes/interactive/components/login-dialog.ts`.
Native: `src/modes/interactive/components/login_dialog.rs`.

| Branch | Native contract | Proof |
|---|---|---|
| constructor/title, device code | native JsString labels and content | login-dialog full-frame differential |
| `showInfo(lines)` :182–190 | clear prior content, spacer, raw lines, spacer, closing hint; request render once | info→info replacement, waiting/progress and prompt differential |
| showPrompt/manual input/submit/cancel | native answers, displayed submitted text, cancellation error | prompt-submit and Escape differential; raw error consumer |
| waiting/progress | exact dim text/hints | full-frame differential |

Removed old-only `show_details` and provider-message/links/optional-hint component API. Callers now supply explicit lines; component owns only frozen line-array presentation. The historical unsupported-action fixture now calls the actual frozen showInfo array API; complete frames/events remain exact assertions, not relaxed expected output. Former RED: oracle showDetails absent and native info layout mismatched frozen closing hint. AuthEvent message/link adaptation remains a caller concern, not a new LoginDialog API.

All prompt answers/labels/info remain JsString. Browser launch utility owns argv encoding. LoginError Display only encodes for Rust formatting. No fallback API retained.
