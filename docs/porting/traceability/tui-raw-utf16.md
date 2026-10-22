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

## Autocomplete (staged)

AutocompleteItem, SlashCommand, argument-completion callbacks, suggestion prefixes,
CompletionResult lines, provider lines/trigger strings, combined base/fd paths,
command descriptions and path parsing/manipulation now retain native strings.
Cursor columns count UTF-16 units; no scalar boundary snapping. File paths and fd
argv encode Node UTF-8 only at actual fs/process calls; external directory/process
bytes decode at their actual text-ingress boundary. Sorting uses native ICU4C UTF-16
collation, not scalar conversion.

`raw_autocomplete::completion_applies_at_a_native_cursor_inside_a_surrogate_pair`
behavioral RED observed baseline X😀tail/cursor1 versus frozen high-X-low-tail/cursor2;
actual native implementation GREEN. Whole module source representation migrated;
expanded command/path/filesystem/callback differential proof pending. Existing typed
numeric gaps are not broadened or silently accepted as source behavior.

Autocomplete follow-up proof covers 168 native apply cases: four raw strings, every
unit cursor (including inside pairs), empty/attachment/overlong prefixes, file/directory
items with trailing quotes. Source behavioral RED exposed negative internal
`cursorCol - prefix.length`: native JS slice counts from the line end, whereas old
Rust saturated to zero. The raw implementation now retains that exact source behavior,
including duplicated-prefix text; no TS fix/delta or new numeric gap. Matrix GREEN.

Autocomplete command proof compares raw descriptions/hints, partial surrogate query
prefixes, slash command results, argument callback inputs/results, and file-trigger
predicates: four corpora × three queries. Filesystem proof creates isolated real
replacement-encoded OS directories, then compares frozen suggestions for native high
surrogate path prefixes and repeated parent segments. Native prefixes/candidate values
retain the original high unit across actual FS calls. Source differential RED exposed
old Rust path normalization collapsing repeated `..`; native normalization now retains
each parent segment like Node POSIX path.join, and the actual FS fixture is GREEN.
This repairs a Rust discrepancy, not source code. fd argv/cancellation raw proof pending.

Word-navigation follow-up `raw_word_navigation` compares native segments, every unit
cursor, and custom segment/atomic callback values and invocation ordering for four
concrete corpora. Behavioral RED caught a new raw-boundary mistake: skipping an
unpaired first unit during scalar mark classification merged `abc` + high-surrogate +
combining-mark into one word. Classification now inspects the actual first decoded
unit (an unpaired unit is not a mark); native segments, inside-pair positions and raw
callbacks match frozen source GREEN. Dictionary segmentation gap unchanged.

## Editor / EditorComponent (staged native representation)

Editor state lines/cursors, history/search drafts, paste registry/buffers, undo/yank,
working/prompt strings, autocomplete snapshots, TextSegment/TextChunk values and all
render/layout lines now retain native units. Public textual callbacks and EditorComponent
inputs/outputs are native; EditorFixedCell::update receives native content. Whole editor
regex matching/replacement uses the single shared JsRegex authority. No scalar render
cache, constructor encoding, scalar-boundary snap, or compatibility rendering path.
Native helpers implement repeat, literal marker removal and linear-time reverse unit
search; Input shares the marker-removal helper (126 source cases still GREEN).

Before production changes, an isolated read-only Rust `797efcee` snapshot ran the new
`raw_editor` test against frozen TS: setText😀 returned byte cursor4 versus source unit
cursor2 (behavioral RED). The actual current Editor/EditorComponent plus production
renderer/dependencies compile in the module-isolated harness, and native cursor/render
source JSON is GREEN. Expanded raw high/low/history/paste/inside-pair/editor narrow-scroll
proof remains pending; this is not whole-module acceptance. No TS fixes/deltas applied.

Editor expanded native source proof is now GREEN: 96 scenarios / 576 intermediate
state snapshots (4 raw corpora × 6 action histories × widths 1/2/4/9). Covers raw
high/low/pair/combining strings, multiline edits, undo, word/line kill+yank, callbacks,
raw bracketed paste and CSI-u control cleanup, 1000+ unit paste registry/expanded text,
raw history/search with length-changing lowercase, submission, and narrow scroll/page
navigation. Public jump-to-low-surrogate reaches an inside-pair cursor, then insertion,
backspace, undo, forward delete and raw insert match frozen JS exactly. Render snapshots
retain the frozen narrow-scroll/overflow lines; no width sanitization or TS delta.
Working prompt/fixed-cell and custom autocomplete-provider raw editor proof still pending.

## Image / terminal-image (staged)

Image base64/mime/filename, fallback styling, cached lines and rendered sequences now
retain native strings. Kitty chunks slice exactly4096 UTF16 units, even through a
pair; iTerm2 width/height text and name remain native until source-defined protocol
construction. `Iterm2Size::Text` holds JsString and uses native `to_js_string`, not a
scalar Display adapter. Hyperlink and image-fallback construction concatenate raw
units. Dimension helpers consume native Buffer base64 via the shared primitive.
The source's iTerm2 name Buffer.from(name).toString('base64') is preserved as its own
UTF8 binary-metadata boundary, not generic component sanitation.

`raw_images::kitty_chunking_keeps_native_units_when_a_pair_crosses_the_source_boundary`
RED reproduced old Rust's byte4096 panic for4095A+😀Z; actual source splits a pair across
native chunks. Current original native sequence JSON matches frozen source GREEN.
Raw constructor/cache/protocol metadata/dimensions corpus is next; not acceptance.

Expanded image proof GREEN:45 frozen scenarios (5 native high/low/pair/mixed/control
corpora × none/Kitty/iTerm2 × widths1/3/8). Compares raw Kitty/iTerm2 parameters and
payloads, native width/height strings, source UTF8/base64 name metadata, raw hyperlinks,
MIME/filename/fallback styling callback inputs, delete sequences, renderImage's actual
source fields sequence/rows/imageId, original/cache/invalidated Image lines and IDs.
A real PNG header represented entirely by high-surrogate base64 units decodes to18x36
through source Buffer semantics while Image render retains the ORIGINAL raw base64
payload. Together with the4096-unit split-pair case this proves raw construction/cache/
render state, not merely removal of a valid-Unicode panic. Numeric algorithms/gaps unchanged.
