# `components/extension-editor.ts`

Authority: whole [common-pin](../source-lock.json) module. [Contract](../../specs/extension-editor.md).
Rust: `crates/pi-coding-agent/src/modes/interactive/components/extension_editor.rs`.

## Current process/representation checkpoint

The prior ComponentRef→Editor downcasts always failed because Editor has no `as_any_mut` hook. This silently emptied child input, ignored saved output and dropped inner focus. One typed `Rc<RefCell<Editor>>` now supplies both native methods and the erased Container child. [Crate-wide audit](component-downcast-audit.md) found no second missing-hook target; all three silent Editor sites are removed, not replaced by fallback behavior.

The component now owns its frozen extension-editor lifecycle instead of invoking the old main-editor helper: literal filename, live command resolution, write-before-stop, asynchronous child wait, one-final-LF removal, finally unlink/restart/forced render. Host restart returns typed `TerminalRawModeError`; concrete TUI adapters no longer ignore it. Container APIs are exposed through native composition and the focus capability is explicit.

| Capability | Current proof |
|---|---|
| title/prefill/options/hints, Enter/cancel and delegated rendering | existing interactive component differential and native tests; final checkpoint rerun pending |
| raw native prefill/submission | existing `raw_extension_dialogs`; final checkpoint rerun pending |
| actual process, exact prefill bytes including lone unit/pair, literal empty argv, filename, saved text submission | `differential_extension_editor_process::extension_editor_real_process_order_focus_and_live_command_match_frozen`, 2 scenarios |
| explicit command wins vs live VISUAL changed after construction | same actual frozen/native process differential |
| input returns before restart/forced-render; component exposes focus capability | same differential; no timing-only assertion |
| saved file removed by child → read error; finally before uncaught process error | actual source and Rust subprocesses with installed production crash hook, exit1, equal stop/input-return/start/render trace, ENOENT read diagnostic |
| write failure, spawn/exit/signal failure, argument validation, restart failure, dynamic EDITOR/default resolution and focus cursor propagation | OPEN: source branches not yet fully proved; not approved deviations |

## Proof ledger

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:

- `test-extension-editor-process-red-2.log`: real child receives empty file; wrong basename; original text submitted after edit; absent focus hook; input blocks; changed VISUAL ignored.
- `test-extension-editor-finally-red.log`: actual read-error native subprocess exits with only stop recorded; frozen restores/render first.
- `test-extension-editor-process-green-1.log`: 2 tests pass after typed handle/lifecycle port (normal explicit/live-command cases plus real crash boundary).

This checkpoint is not whole-module acceptance. Pending branches above remain active; generic ExtensionUiTuiBackend and parent-owned crates are unchanged.
