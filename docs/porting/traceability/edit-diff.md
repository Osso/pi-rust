# Edit-diff traceability

Pinned source: `packages/agent/src/harness/tools/edit-diff.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/tools/edit_diff.rs`. Behavioral proof: `tests/edit_diff_full.rs` (8 tests). Differential proof: edit fixtures in `tests/differential_harness_tools/` compare `diff`, `patch`, `firstChangedLine`, file bytes, and error messages with the real TS tool. Record-only slices remain documented in [edit](edit.md), [fuzzy match result](fuzzy-match-result.md), [applied edits result](applied-edits-result.md), and [edit line endings](edit-line-endings.md).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `edit-diff.ts:7-12` | Detect the first line-ending style | `detect_line_ending`; existing line-ending tests |
| `edit-diff.ts:14-16` | Normalize CRLF/CR to LF | `normalize_to_lf`; existing tests and cross-line-ending edit test |
| `edit-diff.ts:18-20` | Restore LF to CRLF | `restore_line_endings`; existing tests |
| `edit-diff.ts:28-48` | NFKC, per-line trailing whitespace, smart quotes, dashes, special spaces | `normalize_for_fuzzy_match`/`normalize_fuzzy_character`; normalization test |
| `edit-diff.ts:50-52` | Split lines keeping newline endings | `split_lines_with_endings`; exercised by fuzzy preservation |
| `edit-diff.ts:64-72` | Line spans and offsets | `get_line_spans` |
| `edit-diff.ts:74-97` | Widen a replacement to touched lines and reject out-of-range spans | `get_replacement_line_range`; fuzzy preservation test |
| `edit-diff.ts:99-108` | Reverse-order replacements with an offset | `apply_replacements`; exact and fuzzy edits |
| `edit-diff.ts:120-167` | Preserve unchanged original line blocks, merging overlapping groups | `apply_replacements_preserving_unchanged_lines`; fuzzy preservation test and differential fuzzy fixture |
| `edit-diff.ts:187-195` | `Edit`/`AppliedEditsResult` records | Existing records; see record traceability docs |
| `edit-diff.ts:205-242` | Exact-first, then fuzzy match with UTF-16 offsets and replacement content | `fuzzy_find_text`/`find_text`; UTF-16 offset test and fuzzy differential fixture |
| `edit-diff.ts:244-247` | Strip one BOM | `strip_bom`; existing tests |
| `edit-diff.ts:249-253` | Occurrence counting in fuzzy space | `count_occurrences`; duplicate tests |
| `edit-diff.ts:255-290` | Exact not-found/duplicate/empty/no-change messages for single vs multiple edits | `not_found_error`/`duplicate_error`/`empty_old_text_error`/`no_change_error`; error tests and differential fixtures |
| `edit-diff.ts:300-354` | Normalize edits, detect fuzzy mode, match all against the same base, reject overlaps, apply, reject no-op | `apply_edits_to_normalized_content`/`match_edits`; disjoint, overlap, duplicate, missing, no-change, empty tests and differential fixtures |
| `edit-diff.ts:357-363` | Unified patch via jsdiff `createTwoFilesPatch` with file headers only and context 4 | `generate_unified_patch`/`structured_patch`; patch tests and differential edit fixtures |
| `edit-diff.ts:369-499` | Display diff via jsdiff `diffLines`, line-number width, context skipping, first changed line | `generate_diff_string`; diff tests and differential edit fixtures |
| jsdiff 8.0.4 `diff/base.js` | Myers edit-length search, path pruning, common extraction, component values | `diff_tokens`/`add_to_path`/`extract_common`/`build_values` |
| jsdiff 8.0.4 `diff/line.js` | Line tokenization incl. CRLF and trailing-newline handling | `tokenize_lines` |
| jsdiff 8.0.4 `patch/create.js` | Hunk construction, `\ No newline at end of file`, zero-line start quirk | `structured_patch`/`split_lines_for_patch` |

## Rust boundary and discrepancies

- `FuzzyMatchResult.index`/`match_length` count UTF-16 code units, matching JavaScript string lengths. Internal matching uses byte offsets so Rust slicing stays correct.
- `generate_unified_patch`/`generate_diff_string` take an explicit context-line count; the tool passes 4, matching the source default.
- Only the `diffLines`/`createTwoFilesPatch` paths used by the source are ported; callbacks, `maxEditLength`, timeout aborts, and other jsdiff diff variants are not.
- NFKC uses `unicode-normalization` (`=0.1.26`); outputs match JavaScript for representable scalar values.
- `apply_replacements_preserving_unchanged_lines` is private; the source exports it but no other pinned module imports it.
