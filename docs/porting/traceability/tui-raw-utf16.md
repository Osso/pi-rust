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
