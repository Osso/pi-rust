# word-navigation.ts traceability

Source: `packages/tui/src/word-navigation.ts` (117 lines). Rust: `crates/pi-tui/src/word_navigation.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `interface WordNavigationOptions { segment?, isAtomicSegment? }` | `struct WordNavigationOptions<'a> { segment, is_atomic_segment }` | atomic-segment test, differential (default options) |
| `findWordBackward(text, cursor, options?)` | `find_word_backward(&str, usize, Option<&WordNavigationOptions>) -> usize` | `tests/word_navigation.rs` (17 tests); `tests/differential_word_navigation.rs` |
| `findWordForward(text, cursor, options?)` | `find_word_forward(&str, usize, Option<&WordNavigationOptions>) -> usize` | same |
| `Intl.SegmentData` (structural input) | `struct WordSegment { segment, is_word_like }` | atomic-segment test |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| `cursor <= 0` / `cursor >= length` short circuits | `cursor == 0` / `cursor >= text.len()` | `backward_cursor_at_zero...`, `forward_cursor_at_end...`, `cursor_past_end_is_clamped_to_end` |
| default `Intl.Segmenter` word segmentation | `icu_segments` (ICU4X `WordSegmenter::new_auto`) | all tests; differential |
| custom `options.segment` | `segment_text` callback | atomic-segment test |
| skip trailing/leading whitespace unless atomic | whitespace loops in both functions | `backward_skips_whitespace...`, `forward_skips_whitespace...` |
| atomic segment skipped as one unit | `is_atomic_segment` branches | `atomic_segments_are_skipped_as_one_unit` |
| word-like segment stops at last/first `PUNCTUATION_REGEX` match | `word_backward_offset`, `word_forward_offset` | dotted/colon/path/punctuation tests |
| punctuation run consumed until word/whitespace/atomic | run loops | `foo...bar`, CJK |
| multi-byte cursors and astral characters | byte-offset slicing | CJK/emoji differential |

## Behavioral notes

- Cursor positions are UTF-8 byte offsets; the TypeScript API uses UTF-16 code units. The differential oracle converts byte cursors to UTF-16 indices before calling TypeScript and converts results back, so both sides compare the same boundary.
- A cursor that is not on a character boundary snaps down to the preceding boundary. JavaScript callers can address half a surrogate pair; the typed Rust API excludes that input.
- Default segmentation uses ICU4X 2.3.0 (`new_auto`: dictionary + LSTM) because it is pure Rust and closely tracks `Intl.Segmenter`. Two compatibility adjustments make ICU4X match ICU4C/Node where their metadata differs:
  - ICU4X splits a combining mark that follows a Han ideograph; the port merges mark-only segments into the preceding segment (except after a line break), reproducing UAX #29 WB4 as ICU4C applies it.
  - ICU4X tags `ExtendNumLet`/`Extend` runs (for example a lone `_`) as `WordType::Letter`; the port treats a segment as word-like only when it is `WordType::Number` or contains an alphabetic character, matching ICU4C's `WORD_NONE` for those runs.
- The `segment` callback returns only `{ segment, is_word_like }`; the source's `index`/`input` fields are not consumed by either algorithm.

## Approved deviation: ICU4X vs ICU4C dictionary segmentation

ICU4X's CJK/Thai dictionary and LSTM models differ from ICU4C 78.3 (the version bundled with Node 26). These boundaries are pinned by `known_icu4x_dictionary_divergences_are_pinned`. Rows where Rust and TypeScript differ are the only tolerated mismatches in the differential test:

| Text | Cursor (byte) | Rust (ICU4X) forward | TypeScript (ICU4C) forward | Rust backward | TypeScript backward |
|---|---:|---:|---:|---:|---:|
| `好好好` | 0 | 6 (`好好\|好`) | 3 (`好\|好好`) | 0 | 0 |
| `好好好` | 6 | 9 | 9 | 0 | 0 |
| `中国人民` | 0 | 9 (`中国人\|民`) | 6 (`中国\|人民`) | 0 | 0 |
| `中国人民` | 12 | 12 | 12 | 9 | 6 |
| `สวัสดีชาวโลก` | 3 | 18 | 12 | 0 | 0 |

The differential corpus contains these strings plus generated Han/Kana/Hangul/Thai combinations. A mismatch is accepted only when the text contains a dictionary-sensitive script (Han, Hiragana, Katakana, Hangul, Thai, Lao, Khmer, Myanmar); mismatches outside those scripts fail the test. Common cases from the TypeScript test suite (for example `你好世界 test` → `你好|世界| |test`, `日本語テスト` → `日本語|テスト`) match exactly and are asserted equal.

## Gaps / exclusions

- Dictionary-sensitive segmentation differs as documented above (approved deviation, decision 2026-10-18).
- `segment` callbacks returning `Intl.SegmentData` with non-boolean `isWordLike` or missing `segment` are excluded by the typed API (decision 2026-08-12).
- The source's `options` object is optional; Rust callers pass `None` or a `WordNavigationOptions` value.

## Discrepancies

No source bugs found. The ICU4C/ICU4X dictionary difference is an engine-version discrepancy, not a source defect, and is recorded as the approved deviation above.
