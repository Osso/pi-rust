# Coding-agent ComponentRef downcast audit

Parent-requested audit, 2026-10-22. Scope: every `downcast_mut`/`downcast_ref` in `crates/pi-coding-agent` source/examples/tests; distinguish Component trait objects from error/context `Any` payloads. This is a missing-hook audit, not a claim of whole-module parity for every caller.

## Reproduced silent sites

| Site | Before | Fix / behavioral proof |
|---|---|---|
| ExtensionEditor `editor_text` | `ComponentRef → Editor`; absent Editor hook yields `unwrap_or_default`, so child receives an empty file | Retain `Rc<RefCell<Editor>>`; actual frozen/native child reads Node-encoded prefill bytes in `differential_extension_editor_process` |
| ExtensionEditor `set_editor_text` | same missing hook; `if let` discards saved content | Same typed handle; real child writes saved content and subsequent Enter submits it, source/native equal |
| ExtensionEditor `set_focused` | same missing hook; inner focus update silently absent; outer component also lacked the focus capability hook | Typed editor focus and explicit `as_focusable_mut`; actual component capability, frozen focus property and full rendered cursor frames exercised in the real-process differential |

No fallback or added generic Editor downcast hook is needed. The typed handle and erased Container child refer to the same Editor allocation. Container operations remain exposed through the composed component.

## Other audited target groups

All targets below implement `as_any_mut -> Some(self)` at this checkpoint; no additional missing-hook failure was reproduced. Existing optional/capability checks are not changed merely because they use `if let`, `?`, or a reuse branch.

| Caller group | Downcast targets | Existing hook authority |
|---|---|---|
| core/tools: `mod::render_text_component`, `ask_questions::render_result`, edit call/result rendering | Text, BoxComponent | pi-tui components/text.rs, box.rs |
| bundled_extensions/pyrun `update_existing_text` | Text | pi-tui Text; optional last-component reuse also handles a live borrow |
| login_dialog, oauth_selector | Input | pi-tui components/input.rs |
| scoped_models_selector | Input, Text | same concrete hooks |
| extension_input, extension_selector | Input, Text | same concrete hooks |
| bordered_loader | CancellableLoader, Loader | pi-tui components/cancellable_loader.rs, loader.rs |
| bash_execution | Loader, Container | pi-tui loader.rs, tui.rs |
| tool_execution `add_render_child` | BoxComponent or Container | both concrete hooks present; the two branches discriminate native wrapper types |
| coordination_message | CustomMessageComponent, UserMessageComponent | their concrete message component hooks |
| tree_selector | Container, LabelInput | pi-tui Container; local LabelInput hook |
| interactive_mode loader/status/auth/busy paths | Loader, Text | pi-tui concrete hooks |
| interactive_mode tool, streaming, initial/tail/replay/user/bash paths | ToolExecutionComponent, AssistantMessageComponent, UserMessageComponent, BashExecutionComponent | each concrete component returns Some(self) |
| interactive_mode editor replacement + extension adapter | CustomEditor, ExtensionComponentAdapter | concrete hooks; composed example editors intentionally expose their CustomEditor base |
| custom_editor input continuation | CustomEditor | expected concrete/composed base is checked with `expect`, not silently ignored |
| example editor composition and test wiring | CustomEditor | native composed adapter exposes its base explicitly |

Error payload downcasts (custom-entry errors, uncaught errors) and typed event/context payload downcasts are not ComponentRef casts. Constructor graphs were checked for the loader/status/dialog/tree groups: Loader, Input, Text, LabelInput, BoxComponent and Container handles point at those concrete children; cancellable/plain loader and Box/Container pairs intentionally discriminate two supported types. The original Editor was the only referenced ComponentRef target without a concrete hook. No blanket `expect` conversion was applied to unrelated optional casts.
