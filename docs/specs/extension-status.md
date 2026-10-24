# Extension footer status

Authority: frozen `core/footer-data-provider.ts:25,175–183,208–214`, extension UI `setStatus`, RPC `rpc-mode.ts:174–181`.

- ExtensionStatus is text or a callback receiving the displayed FooterSessionOverride (undefined when absent). A callback returns text/undefined or throws. Native callbacks return Result<Option<JsString>,CallbackError>; the argument is an owned optional override under the owned-values approval.
- FooterDataProvider stores the text/callback without evaluation. Every getExtensionStatuses read evaluates current callbacks with the current provider override, preserving map order and original errors. Undefined results are omitted from that read, not deleted from storage. Replacement/removal/clear retain frozen Map behavior for pure status callbacks.
- Interactive UI stores the carrier; footer rendering reads statuses on every render, including cached-session-data renders. RPC evaluates the callback immediately with undefined and emits the resulting optional statusText. The no-op UI never evaluates callbacks. Setter failure returns to the extension handler; no mode-local logging or error rewriting.
- Checked footer render methods return original callback errors to synchronous native callers. Component adapters feed only otherwise-unobserved render errors into the shared uncaught authority. Frozen TUI nextTick/timer render scheduling (`tui.ts:962–1013`) and its render try/finally (`1897–1902`) do not catch these errors. InteractiveMode's existing global uncaughtCrash handler (`5210–5223,5279–5281`) restores the terminal and exits; its native shared authority remains responsible.
- Bundled/default footer and Pyrun capability snapshot consumers evaluate exactly at their frozen read point and propagate original failures. Existing text callers only wrap Text and handle the new Result; no detached-runtime or other UI behavior is changed.

The documented pure-callback boundary and its snapshot-on-read gap are recorded once in [footer-data-provider traceability](../porting/traceability/footer-data-provider.md).

The existing crash policy reads original `.code` on native and callback error carriers, including raw object reasons; a typed carrier must not hide a dead-terminal code. This changes no cleanup or shutdown policy.
