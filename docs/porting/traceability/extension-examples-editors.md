# Native editor and overlay examples

Pinned source: `packages/coding-agent/examples/extensions/{border-status-editor,modal-editor,rainbow-editor,overlay-test}.ts`.
Opt-in functions in `crates/pi-coding-agent/examples/extensions/`; never discovered automatically.

| Source behavior | Rust | Proof |
|---|---|---|
| Three session-start editor factories, CustomEditor inheritance | `editor_composition::ComposedEditor` delegates base editor capabilities and exposes its CustomEditor for host callbacks | core factory integration; `extension_examples_editors` differential GREEN |
| Modal escape/insert/normal, hjkl/0/$/x/i/a, printable filtering and mode label | `modal_editor::Modal` | differential key sequences, widths 1/8/20/70 |
| Rainbow case-insensitive word replacement, RGB interpolation, 60ms timer, 20-frame cycle, stop/restart | `rainbow_editor::Rainbow` | differential animation clock and input sequences |
| Border fit/truncation, live model/thinking/context/cwd/branch; footer suppression; 80ms spinner events | `border_status_editor::BorderStatus`, `State`, `fit_border` | differential widths 1/8/20/70/100 and lifecycle |
| Overlay fixed width 70, styled/wide/emoji rows, focus-conditioned cursor marker, navigation/input/select/cancel | `overlay_test::OverlayTest` | differential focus, key sequences, result, notify, overlay flag |

Oracle imports actual pinned modules and actual CustomEditor/Editor/TUI. Public package import redirects only to pinned CustomEditor (avoids loading unrelated agent entrypoints); deterministic timer callbacks substitute wall time. Rust tests run native factories through ExtensionRunner, real renderer fixed cells and paused Tokio time. No module bytes patched.

## Source discrepancies

- Rainbow has no dispose hook: its interval survives replacement while the word remains present. Native timer likewise detaches until explicitly stopped or its runtime ends; preserved source lifetime bug, not silently repaired.
- Border git failures are deliberately swallowed by the source; native branch refresh mirrors this and renders no branch.

## Proof

- Five differential tests: modal, rainbow, border spinner, border live model/context/thinking/home/no-branch, overlay (three select/cancel scenarios). Every snapshot compares exact ANSI strings at multiple widths and text/focus state.
- Oracle uses real pinned TUI focus routing. Core tests additionally mount real CustomEditor, dispatch input through regular/fullscreen interactive TUI, and prove overlay width/options, close-on-Enter, single disposal, synchronous completion, cursor-marker consumption and hardware cursor column.
- Mutation proofs: modal NORMAL label → BROKEN; rainbow red channel +1; spinner advances two frames; overlay omits cursor marker. Each produced a behavioral differential assertion failure; each file restored byte-identically. Logs: `/tmp/pi-rust-ext-ui-editor-focus/15-*` through `18-*`, ledger beside them.
- Rust clock tests service interval deadlines with one timer-wheel tick of scheduling allowance; TS interval callbacks are deterministically advanced. Branch fixtures wait for the actual git completion's render request, not a guessed sleep.
- All four examples remain opt-in. `editor_composition.rs` is a companion to the three editor examples, not a built-in registration.

## Host scope

Overlay-test needs the component's numeric width field honored and its done callback to close/dispose. These missing host semantics were exposed during the port and implemented from TS interactive-mode.ts:3485–3528. Explicit overlay sizing options also override component width. Existing custom UI `onHandle` callback delivery remains outside this slice; none of these four examples uses it.

## Discrepancy corrected

Rust Editor computed a negative cursor offset for wrapped chunks that did not contain the cursor. TS only subtracts inside the cursor-containing branch (editor.ts:1285–1293). The narrow-width modal sequence reproduced the Rust overflow; the branch now matches TS.

