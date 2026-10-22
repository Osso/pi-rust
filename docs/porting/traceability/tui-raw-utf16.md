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

## Renderer / Terminal write boundary

All `tui.rs` line/frame/cache/overlay/region/fixed-cell buffers use native strings.
`FixedCell::update` accepts native input. `Terminal::write(&JsString)` receives the
original assembled buffer. `ProcessTerminal::write(Into<JsString>)` encodes Node UTF-8
at `output.write_all`; panic-restore buffers likewise remain raw until stdout write.
Diagnostic JSON serializes native units; crash logs encode only at actual fs write.
`terminal_image::is_image_line` recognizes native string sequences without conversion.

`raw_frame` compares original native mock write arrays through initial full frame,
line diff, overlay composition, overlay removal, and fixed-cell update, including
unpaired high/low and pairs. Stop compares concatenated native content: pre-existing
Rust stop groups ASCII move+CRLF into one write where frozen source emits two. This
write-call grouping discrepancy is recorded, not fixed/approved by this slice.
A separate test compares actual Node `process.stdout.write` bytes with ProcessTerminal
for high, low, pair, mixed raw strings. Mock captures are never terminal-encoded.

Proof now imports the actual production renderer and ProcessTerminal into the
module-isolated harness; the earlier unused-interface shim is no longer needed.
No whole-crate compilation acceptance: remaining component/editor consumers pending.

`Terminal::set_title(&JsString)` appends the unsanitized native title inside OSC 0.
`raw_frame::raw_terminal_title_reaches_osc_output_without_sanitation` compares a
high/low/pair/ESC/BEL title against frozen ProcessTerminal's intercepted original
stdout write and actual Node stdout bytes. Native interface RED then GREEN.

## Simple composition/components

RootCompositor rendered entries and gaps, Box child/cache/background samples,
TruncatedText storage, DynamicBorder formatter, Spacer lines, and Loader message,
frames/styles/render state now retain `JsString`. CancellableLoader message/frame
wrapper signatures follow Loader. No alternate scalar paths.

`raw_components` source differential: four raw corpora × widths 0–7 × padding 0/1
(64 scenarios), compares Box repeated cached lines, TruncatedText, DynamicBorder
callback output, Spacer, flow/bottom RootCompositor, Loader before/after raw message
update. Native interface RED -> source differential GREEN. CancellableLoader's
wrapper compiles only when remaining crate integration is unblocked; dedicated raw
abort/cancellation proof still pending.

## SelectList

Item value/label/description, filters, normalized descriptions, primary truncation
context/callback results, styling, and rendered lines are native strings. `set_filter`
accepts `Into<JsString>`. Native shared JsRegex performs newline normalization.
`raw_select` compares 24 frozen scenarios (4 raw corpora × widths 0,1,4,12,41,72),
including scroll selection, selected item values, actual `onSelect` raw callback values,
raw prefix filters, no-match paths, and wide description columns. Interface RED then
source differential GREEN. Custom primary callback raw signature migrated; dedicated
custom callback corpus pending.

## Fuzzy matcher

Queries, lowercase input, swap-alphanumeric fragments, slash/whitespace tokens and
candidate strings are native UTF-16. Generic query/candidate inputs lift once into
`JsString`; matching/scoring has no scalar alternate path. `raw_fuzzy` compares
7 raw queries against 6 concrete candidates, matching scores and filter identity:
high, low, half-pair, pair/slash, digit/letter swaps, Greek combining/lowercase tokens.
Native API RED then frozen source differential GREEN.

## Keyboard text decoding

`keys::{matches_key,parse_key,decode_kitty_printable,decode_printable_key,
is_key_release,is_key_repeat}` accept native inputs. Printable decoder outputs and
Key identifier templates retain native units, including numeric surrogate codepoints.
ASCII keyboard grammar recognition is a parser-only transient projection; raw input
is never component text/output state or a compatibility fallback. Key IDs lowercase
natively before grammar recognition, preserving e.g. Kelvin-sign -> ASCII k matching.
Out-of-range numeric gaps are unchanged.

`raw_keys`: six frozen decoder cases (high/low/pair/shifted/modifyOtherKeys), four
native key/identifier/event cases. Behavioral RED caught source CSI-u 55296 decoding
to a high surrogate while baseline returned None. GREEN retains raw output. Input
source corpus expanded from 108 to 126 scenarios with Kitty high+low insertion and
submission; previous 108 histories remain included and pass after decoder migration.

## Terminal color response parsing

OSC11 payload/frame/trim/RGB slices and OSC997 matching accept native strings.
Only validated ASCII hex digits are projected for existing numeric parseInt semantics;
raw framing payloads remain native and invalid RGB payloads are not sanitized.
`raw_terminal_colors` compares frozen final-newline framing, Unicode trim, uppercase
RGB, and high/low/pair payload framing (6 cases). Raw-interface RED then GREEN.
Existing nonfinite typed RGB-channel gap unchanged.

## Native component input and listener routing

`Component::handle_input(&JsString)`, `TerminalEvent::Input(JsString)`,
`TuiInputListener = Rc<dyn Fn(&JsString) -> TuiInputListenerResult>`, and listener
rewrite `data: Option<JsString>` now retain native units. Terminal byte decoding lifts
once into native event data; TUI focus/OSC/cell-size recognition and focused-component
routing never scalarize listener output. Source-backed ASCII numeric parsing is not
component text authority. Input's native algorithm is shared by its trait entrypoint.

`raw_frame::native_input_listener_rewrites_and_terminal_events_reach_focused_input_losslessly`
compares frozen actual TUI listener rewrites to high/low units, direct raw events,
focused Input values, original listener data and submit callbacks. Interface RED then
actual production module differential GREEN. Remaining component implementations and
legacy test helpers still require native signature migration; no scalar fallback.

## SettingsList

Item IDs, labels, descriptions, values, submenu input/done, change callbacks, theme
callbacks/cursor, search/filter data and rendering now use native strings.
`update_value` accepts native IDs/values; component input is `&JsString` throughout.
`raw_settings` compares 24 frozen scenarios (4 raw corpora × widths 1/10/50 × search
off/on), intermediate render lines, raw update IDs, value cycling/change callbacks,
raw search keys, selection/cancel state. Interface RED then source differential GREEN.
Submenu raw signatures migrated; dedicated submenu callback proof pending.

Observed frozen boundary preserved: SettingsList render(1) calls wrapping at width -3,
which emits an extra blank before a zero-width surrogate description. Earlier Rust
saturating subtraction silently clamped this internal source computation. Behavioral
source differential reproduced the missing line; native wrapping now accepts signed
internal width while public usize/numeric gaps remain unchanged. Settings source
behavior is preserved, not repaired; source/delta files remain untouched. Existing
192-case raw utility matrix remains GREEN after this internal width change.
