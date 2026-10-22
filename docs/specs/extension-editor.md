# Extension editor

Authority: frozen `packages/coding-agent/src/modes/interactive/components/extension-editor.ts` at the [common pin](../porting/source-lock.json).

- Compose the inherited Container and Editor. Preserve title, prefill truthiness, editor options, Enter submission, cancel priority, external-editor keybinding and propagated focus.
- Resolve the external editor when invoked: explicit nonempty command, live VISUAL, live EDITOR, then platform default. Split on literal spaces, preserving empty arguments.
- Write native text through Node UTF-8 to `pi-extension-editor-<Date.now()>.md` in the platform temporary directory before stopping TUI. Spawn asynchronously with inherited stdio. Input returns while the child is running; do not block TUI's event loop waiting for exit.
- Only exit code zero reads the saved file. Decode as Node UTF-8 and remove exactly one final LF before updating the same Editor. Spawn error, signal termination or nonzero exit leaves original text unchanged.
- Empty executable and embedded NUL in executable/argv reject with the frozen `TypeError [ERR_INVALID_ARG_VALUE]` message. Use the shared Node inspection/truncation authority. Do not misclassify validation rejection as an ordinary spawn error.
- Pre-await write/stop errors execute finally before input returns, but their unhandled rejection is reported later. Spawn-argument rejection awaits the rejected promise, so finally occurs after input returns.
- Finally attempt unlink (ignore cleanup errors), restart TUI, then request forced render, including failure paths. Restart errors must propagate rather than disappear. An uncaught read/write/argument failure reaches the process error boundary after finally; it must not silently become a successful edit.

[Traceability](../porting/traceability/interactive-extension-editor.md) records current proof and open branches.
