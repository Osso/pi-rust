# Native editor and overlay examples

Pinned source: `packages/coding-agent/examples/extensions/{border-status-editor,modal-editor,rainbow-editor,overlay-test}.ts`.
Opt-in functions in `crates/pi-coding-agent/examples/extensions/`; never discovered automatically.

| Source behavior | Rust | Proof |
|---|---|---|
| Three session-start editor factories, CustomEditor inheritance | `editor_composition::ComposedEditor` delegates base editor capabilities and exposes its CustomEditor for host callbacks | core factory integration; `extension_examples_editors` differential (pending GREEN) |
| Modal escape/insert/normal, hjkl/0/$/x/i/a, printable filtering and mode label | `modal_editor::Modal` | differential key sequences, widths 1/8/20/70 |
| Rainbow case-insensitive word replacement, RGB interpolation, 60ms timer, 20-frame cycle, stop/restart | `rainbow_editor::Rainbow` | differential animation clock and input sequences |
| Border fit/truncation, live model/thinking/context/cwd/branch; footer suppression; 80ms spinner events | `border_status_editor::BorderStatus`, `State`, `fit_border` | differential widths 1/8/20/70/100 and lifecycle |
| Overlay fixed width 70, styled/wide/emoji rows, focus-conditioned cursor marker, navigation/input/select/cancel | `overlay_test::OverlayTest` | differential focus, key sequences, result, notify, overlay flag |

Oracle imports actual pinned modules and actual CustomEditor/Editor/TUI. Public package import redirects only to pinned CustomEditor (avoids loading unrelated agent entrypoints); deterministic timer callbacks substitute wall time. Rust tests run native factories through ExtensionRunner, real renderer fixed cells and paused Tokio time. No module bytes patched.

## Source discrepancies

- Rainbow has no dispose hook: its interval survives replacement while the word remains present. Native timer likewise detaches until explicitly stopped or its runtime ends; preserved source lifetime bug, not silently repaired.
- Border git failures are deliberately swallowed by the source; native branch refresh mirrors this and renders no branch.

## Remaining proof

GREEN, per-example mutation checks and expanded boundary cases pending.
