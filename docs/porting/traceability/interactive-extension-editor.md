# `components/extension-editor.ts`

Authority: whole 167-line [common-pin](../source-lock.json) module. [Contract](../../specs/extension-editor.md).
Rust: `crates/pi-coding-agent-ui/src/modes/interactive/components/extension_editor.rs`.

## Representation and dispatch

One typed `Rc<RefCell<Editor>>` supplies native methods and the erased Container child. The old ComponentRef→Editor downcasts always failed because Editor has no `as_any_mut`: child input became empty, saved output disappeared, and inner focus was never updated. [Crate-wide audit](component-downcast-audit.md) lists all three fixed sites and the other supported target groups; no generic downcast hook or fallback is added.

`Deref`/`DerefMut`, delegated rendering/invalidation and `as_container` expose inherited Container operations. `as_focusable_mut` and the typed editor propagate source focus. The constructor preserves native title/prefill/options, submission and cancel callbacks. Existing constructor-render cases now run in a dedicated test, rather than also remaining in the broad leaf-helper list; their requests/assertions are unchanged and theme initialization still precedes fixture construction.

## Whole-module capability matrix

| Export / source branch | Rust item / behavior | Proof |
|---|---|---|
| `ExtensionEditorComponent extends Container implements Focusable` | composed Container and typed Editor; focus capability and setter delegation | complete before/after rendered UTF-16 frames, including cursor marker; constructor rendering differential; raw prefill/submission |
| constructor title, spacing, theme, hints, prefill truthiness, EditorOptions; Enter callback | `new`; inherited Editor delegates options; source default command always makes external-editor hint present | dedicated frozen constructor differential 1 test / 2 cases, existing native submit and raw tests, process frames |
| global cancel binding before app external binding | `handle_input` | real frozen scenario maps external action to Escape but receives cancel only; native cancel test |
| remaining keys forwarded to Editor | same | Enter/submission and complete frame comparisons |
| explicit nonempty command → live VISUAL → live EDITOR → platform default | `get_external_editor_command` at invocation | explicit vs changed VISUAL, empty explicit/VISUAL with EDITOR, controlled default executable on PATH |
| `getExternalEditorCommand` undefined guard | source cannot reach it: platform default is nonempty | literal source audit; controlled default launch confirms behavior |
| `currentText`, `Date.now` filename, `os.tmpdir`, write before stop | typed editor read; `extension_editor_temp_file`; Node UTF-8 at file boundary | real child compares exact prefill bytes incl lone high and astral pair, normalized timestamp basename, actual parent directory, literal empty argv |
| live TMPDIR → TMP → TEMP → Unix /tmp | shared process.env overlay, existing Node POSIX join | four real source/native directory scenarios RED→GREEN; no host-only std::env lookup as an alternate authority |
| asynchronous spawn with inherited stdio and literal-space split | synchronous prelude/spawn, Tokio-local async completion | stop/input-return/start/render event ordering; exact child argv and process environment |
| child error / nonzero exit / signal | ordinary failure leaves Editor untouched | missing executable, actual exit17, actual SIGTERM; unchanged full rendered state and raw submitted units |
| exit0 read, Node UTF-8 decode, remove exactly one final LF | `await_external_editor` | two-LF and no-final-LF saved files; invalid UTF-8 + CRLF; full rendered state before Enter so submission trimming cannot hide newline differences |
| read/write/stop errors and restart error overriding body failure | captured prelude, async rejection, finally result precedence | 7 actual frozen/native subprocess scenarios, installed production uncaught hook, exit1, exact error line and event order |
| empty executable, NUL executable / argv | `validate_external_editor_arguments`; shared Node ERR_INVALID_ARG_VALUE inspection/truncation | actual process comparison of precise TypeError code/message; ordinary spawn error remains distinct |
| finally unlink errors ignored; start then force full render | `finish_external_editor -> Result`; typed host restart | edited files no longer exist; child-deleted file still restores; restart failure suppresses forced render and overrides read error |

## Runtime and boundary details

The source's own extension-file/process path replaces the old main-editor helper. The UI runs the native completion on its Tokio LocalSet, matching the source event loop; no synchronous alternate path is retained. Native host restart returns `TerminalRawModeError` instead of silently discarding it.

Pre-await write/stop failure runs finally before input returns, but reports rejection later. Argument validation rejects the awaited child promise, so finally follows input-return. A finally failure overrides the body failure. Source/native subprocess traces prove these distinct orders, not elapsed-time estimates.

The inspector is extracted from the existing shared Node filesystem formatter; its existing 13 Node filesystem tests remain GREEN. Text stays `JsString` until actual file/stdout/process boundaries. Temporary filenames normalize only independent Date.now digits in the oracle; complete bytes, parent directory, arguments, frames and diagnostics remain assertions. Runtime process proof is on the current Linux host; the platform command/temp-directory branches remain explicit in code.

## Proof ledger

Artifacts `/tmp/pi-rust-repin-lossless-interactive/`:

- RED: `test-extension-editor-process-red-2.log` (empty file, lost update/focus, wrong name, blocking input, stale VISUAL); `test-extension-editor-finally-red.log` (read failure skips restore); `test-extension-editor-failures-red-1.log` (stop/write timing and three validation errors); `test-extension-editor-temp-env-red.log` (four wrong temporary directories).
- GREEN: `test-extension-editor-branches-green-2.log`: 2 tests, 14 handled scenarios plus 7 uncaught subprocess scenarios. Before/after full frames prove focus and final-LF semantics.
- Constructor rendering: `test-extension-editor-constructor-green-2.log` 1/1 (2 source cases). Native submit/cancel: `test-extension-editor-native-green.log` 2/2. Raw prefill/submission: `test-extension-editor-raw-green.log` 1/1.
- Shared Node formatting regression: `test-extension-editor-shared-node-fs-green.log` 13/13.
- Historical interim checkpoints: 549fca1c, 46db7864; each required coding-agent all-target check passed with zero warnings. Final checkpoint check recorded by SHA separately. Parent owns final independent/readability/clippy gates.

No new semantic deviation is claimed. Inherited Editor internals retain their own traceability; this does not claim whole interactive-mode or Q13 completion. Generic ExtensionUiTuiBackend and parent-owned ai/agent-core are unchanged.
