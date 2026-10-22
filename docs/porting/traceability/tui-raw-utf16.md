# Native UTF-16 TUI representation (Q18, staged)

Frozen source: [`source-lock.json`](../source-lock.json), common pin `4f9e6f8a4`.
No later source deltas, deleted Q2 features, sanitization, or new semantic gaps authorized.

## Utilities and segmentation

`utils.rs` text arguments accept `Into<JsString>`; text results, ANSI payloads and lengths,
OSC8 URLs, wrapping tokens, tracker state, overlay segments, and background callbacks retain
native UTF-16 units. `text_units.rs` segments original units with ICU4X's UTF-16 grapheme
iterator. Scalar-property checks do not become rendered text authority. Lone surrogates have
source `Surrogate` zero-width/leading-nonprinting classification, not replacement width.

| Source exports / branches | Behavioral proof |
| --- | --- |
| `extractAnsiCode`, UTF-16 positions | `raw_utf16::ansi_offsets_are_native_utf16_units` RED at baseline |
| `visibleWidth`, `wrapTextWithAnsi`, `truncateToWidth`, `sliceByColumn`, `extractSegments`, `normalizeTerminalOutput` | `raw_utils` differential: high, low, pair, mixed units, combining mark, Thai, SGR, raw OSC8 URL; widths 0–5, starts 0–3 (192 cases) |
| `getGraphemeCellRange`, `getOsc8LinkAtColumn`, `strip_terminal_sequences`, background callback | migrated representation; dedicated raw assertions pending |
| ANSI tracker, hyperlink reopening/closing | wrapping/extraction differential raw hyperlink corpus |
| printable/nonprinting classification | width differential high/low/paired/combining cases |

## Proof boundary

Crate consumers still require migration. Utility differential GREEN currently uses an isolated
Cargo harness importing the actual production utility/segmentation files; mandatory repository
runner, jobs=4, same pinned source oracle. It is **not** whole-crate compilation or integration
acceptance. `/tmp/pi-rust-repin-lossless-tui/REPORT.md` records commands and revisions.

Input baseline reproduces `handleInput('a'); setValue('😀'); handleInput('x')` panic with cursor
1. Frozen JS retains cursor 1 and inserts between pair units. No inside-pair gap is approved.

## Input / kill ring / word navigation (staged)

`Input` value, paste buffer, undo values, masking, cursor, edit slices, kill ring and
render lines now use `JsString` / UTF-16 units. Native `handle_input(Into<JsString>)`
is the sole input algorithm; the decoded terminal `Component::handle_input(&str)`
entrypoint lifts into it. ASCII keyboard-protocol parsing is a transient recognition
projection only, never text state or output. Submit callbacks receive `&JsString`.

`raw_input` compares every intermediate value/cursor/render and submitted values over
six concrete edit histories × mask on/off × widths 1–9 (108 scenarios): inside-pair
insert/backspace/delete/undo, lone high/low keys, split bracketed paste with CRLF/tab,
word kill/yank, home/end, submit. Native source output uses `JSON.stringify`.
Word navigation now segments UTF-16 directly and does not snap inside-pair cursors.
Its callbacks and custom segments are native strings. ICU dictionary gap unchanged.

`Component::render` now declares `Vec<JsString>`. Remaining component/frame
implementations are compile-blocked, not adapted through scalar encoding.
Input isolated proof uses production Input/kill/word/keybinding/keys modules and the
literal source trait interface with placeholder unused Container; no renderer proof.

## Text

`Text::{new,with_background,set_text}` accept `Into<JsString>`; `text()` borrows
`JsString`. `StyleFn = Rc<dyn Fn(&JsString) -> JsString>`. Text, cached text,
wrapped/padded lines and cached rendered lines are native, with no scalar copies.
`raw_text` compares 256 frozen cases (raw high/low/pair/mixed, widths 1–8,
horizontal/vertical padding, background off/on); each compares two cached renders.
Representation RED was the pre-migration scalar Text failing the native trait/test
interface; GREEN uses actual Text in the same isolated harness as Input.
