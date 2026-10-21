# utils.ts frozen traceability

Reference: immutable `4f9e6f8a4`, `packages/tui/src/utils.ts` (1188 lines).
Rust: `crates/pi-tui/src/utils.rs` + generated `unicode_tables.rs`. [Q1 proof](../q1-tui-core-proof.md).

| Frozen export/function/branch | Rust mapping | Proof |
|---|---|---|
| getGraphemeSegmenter/getWordSegmenter | internal unicode-segmentation/ICU4X, not JS Intl objects | typed host-object adaptation; segmentation exercised through text APIs |
| cjkBreakRegex, PUNCTUATION_REGEX | generated CJK predicate and punctuation predicate | unit + character-class differential |
| couldBeEmoji/graphemeWidth/visibleWidth | frozen base code point, marks/emoji/regional-indicator/halfwidth/Thai-Lao rules; tabs are width 3 | corpus + approximately 11k code point/emoji/flag samples |
| normalizeTerminalOutput | Thai/Lao AM decomposition only; tabs preserved | unit + source normalization corpus |
| extractAnsiCode | CSI m/G/K/H/J and OSC/APC BEL/ST scan | complete extracted-sequence stream comparison |
| parse/format OSC8 hyperlink/close, AnsiCodeTracker reset/clear/process/active/line-end methods | same SGR attributes, foreground/background state, link terminator preservation | wrapping, slicing and segment source differential |
| splitIntoTokensWithAnsi/wrapTextWithAnsi/wrapSingleLine/breakLongWord/updateTrackerFromText | LF-only line split; style/link carry; CJK tokens; whitespace/long-token handling | unit + source wrap matrix |
| isWhitespaceChar/isPunctuationChar | JS whitespace/punctuation predicates | ECMAScript character-class source matrix |
| applyBackgroundToLine | visible-width padding then caller style function | unit + source callback-output comparison |
| truncateFragmentToWidth/finalizeTruncatedResult/truncateToWidth | empty/zero/ellipsis clipping, ASCII/Unicode/tab/ANSI paths, contiguous prefix, padding/reset placement; no added OSC8 close | unit + source truncation matrix |
| sliceByColumn/sliceWithWidth | strict/non-strict wide-grapheme boundaries, inherited ANSI and width | unit + source slice matrix |
| extractSegments | before/after extraction, inherited styling, strict after bound, zero-after-length early exit | unit + source segment matrix |

Removed later terminal-spacing-mark/conjunct corrections, tab normalization, CR splitting and truncation hyperlink closing. Those later improvements are not frozen behavior. Removed unused generated nonprinting/spacing-mark tables and their generator entries; remaining Unicode 17.0 properties and EAW/RGI data are unchanged.

Later-only stripTerminalSequences/getGraphemeCellRange/getOsc8LinkAtColumn are absent from frozen exports. Their internal Rust implementations remain for Q2/Q13 consumers with local unit tests only; impossible differential calls to nonexistent frozen exports were removed. The retained inventory is in the Q1 report.

Representation boundaries: positions/lengths are UTF-8 bytes rather than UTF-16 units; tests compare extracted ANSI streams at valid character boundaries. JS Intl.Segmenter/RegExp objects are represented by Rust operations/predicates, not returned JS host objects. Width caching and pooled tracker reuse are omitted because pure recomputation has the same outputs. Invalid JS runtime input types remain excluded by the typed API.
