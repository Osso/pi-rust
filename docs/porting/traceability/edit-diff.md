# Edit-diff traceability

## Agent-core `edit_diff`

Pinned source: `packages/agent/src/harness/tools/edit-diff.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/tools/edit_diff.rs`. Behavioral proof: `tests/edit_diff_full.rs` (8 tests). Differential proof: edit fixtures in `tests/differential_harness_tools/` compare `diff`, `patch`, `firstChangedLine`, file bytes, and error messages with the real TS tool. Record-only slices remain documented in [edit](edit.md), [fuzzy match result](fuzzy-match-result.md), [applied edits result](applied-edits-result.md), and [edit line endings](edit-line-endings.md).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `edit-diff.ts:7-12` | Detect the first line-ending style | `detect_line_ending`; existing line-ending tests |
| `edit-diff.ts:14-16` | Normalize CRLF/CR to LF | `normalize_to_lf`; existing tests and cross-line-ending edit test |
| `edit-diff.ts:18-20` | Restore LF to CRLF | `restore_line_endings`; existing tests |
| `edit-diff.ts:28-48` | NFKC, per-line trailing whitespace, smart quotes, dashes, special spaces | `normalize_for_fuzzy_match`/`normalize_fuzzy_character` over `unicode::normalize_nfkc`; normalization test, differential fuzzy fixtures, and `differential_unicode_normalize` |
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
| `edit-diff.ts:357-363` | Unified patch via jsdiff `createTwoFilesPatch` with file headers only and context 4 | `generate_unified_patch` over `jsdiff::create_two_files_patch`; patch tests and differential edit fixtures |
| `edit-diff.ts:369-499` | Display diff via jsdiff `diffLines`, line-number width, context skipping, first changed line | `generate_diff_string`; diff tests and differential edit fixtures |
| jsdiff 8.0.4 `diff/base.js` | Myers edit-length search, path pruning, common extraction, component values | `jsdiff::diff_lines` (shared with coding-agent) |
| jsdiff 8.0.4 `diff/line.js` | Line tokenization incl. CRLF and trailing-newline handling | `jsdiff::tokenize` |
| jsdiff 8.0.4 `patch/create.js` | Hunk construction, `\ No newline at end of file`, zero-line start quirk | `jsdiff::structured_patch` |

## Rust boundary and discrepancies

- `FuzzyMatchResult.index`/`match_length` count UTF-16 code units, matching JavaScript string lengths. Internal matching uses byte offsets so Rust slicing stays correct.
- `normalize_for_fuzzy_match` trims the JavaScript `trimEnd` character set (including U+FEFF, excluding U+0085), not Rust `str::trim_end`; the coding-agent differential normalize cases cover NEL and BOM.
- `generate_unified_patch`/`generate_diff_string` take an explicit context-line count; the tool passes 4, matching the source default.
- Only the `diffLines`/`createTwoFilesPatch` paths used by the source are ported; callbacks, `maxEditLength`, timeout aborts, and other jsdiff diff variants are not.
- NFKC uses `pi_agent_core::unicode::normalize_nfkc` (`icu_normalizer = 2.3.0`); `differential_unicode_normalize` proves all four `String.prototype.normalize` forms against Node over Hangul, combining marks, compatibility characters, and emoji.

## Shared jsdiff module

`pi_agent_core::jsdiff` is the single port of `diff@8.0.4`: `tokenize` (line tokenization including CRLF), `diff_lines` (empty-token removal, Myers edit-length search with the source's diagonal pruning and tie-breaking, component flattening), `structured_patch` (hunk construction, context, `\ No newline at end of file`, zero-line start quirk), and `create_two_files_patch` (file headers only). Both edit-diff modules call it; neither keeps a second copy. Differential coverage: 700 seeded diff/patch cases over a duplication-heavy line alphabet plus targeted empty, append-only, deletion-only, CRLF, and no-newline cases (coding-agent), and the harness-tools edit fixtures (agent-core).

## Coding-agent `edit_diff`

Pinned source: `packages/coding-agent/src/core/tools/edit-diff.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::edit_diff`. Functions whose TypeScript body is identical to `packages/agent/src/harness/tools/edit-diff.ts` call the agent-core implementation directly: `detectLineEnding`, `normalizeToLF`, `restoreLineEndings`, `normalizeForFuzzyMatch`, `splitLinesWithEndings`, `getLineSpans`, `getReplacementLineRange`, `applyReplacements`, `applyReplacementsPreservingUnchangedLines`, `fuzzyFindText`, `stripBom`, `getNotFoundError`, `getEmptyOldTextError`, `getNoChangeError`, `generateUnifiedPatch`, `generateDiffString`, and the `Edit`/`AppliedEditsResult`/`FuzzyMatchResult`/`TextReplacement` records. `EditDiffResult` and the preview helpers are coding-agent-only; the remaining functions differ as follows.

| TypeScript function | Differing lines (agent source -> coding-agent source) | Rust |
|---|---|---|
| `countOccurrences` | `return fuzzyContent.split(fuzzyOldText).length - 1;` -> `return findTextOccurrences(fuzzyContent, fuzzyOldText).length;` plus the new non-overlapping `findTextOccurrences` scan | `find_text_occurrences`/`count_occurrences` |
| `getDuplicateError` | signature gains `content: string, oldText: string`; both messages append `${context}`; new constants `DEFAULT_DUPLICATE_CONTEXT_MATCHES`/`DEFAULT_DUPLICATE_CONTEXT_LINES`/`DEFAULT_CONTEXT_LINE_MAX_LENGTH`, `TextOccurrence`, `getLineNumberAtOffset`, `truncateContextLine`, `formatDuplicateMatchContexts` | `duplicate_error`/`format_duplicate_match_contexts`/`line_number_at_offset`/`truncate_context_line` |
| `applyEditsToNormalizedContent` | duplicate call passes the extra arguments: `getDuplicateError(path, i, normalizedEdits.length, occurrences)` -> `getDuplicateError(path, i, normalizedEdits.length, occurrences, replacementBaseContent, edit.oldText)` | `apply_edits_to_normalized_content`; all other steps call the shared agent-core helpers |
| `computeEditsDiff`, `computeEditDiff`, `EditDiffResult`, `EditDiffError` | coding-agent-only preview API appended after `generateDiffString`; reads the file, strips the BOM, normalizes, applies, and maps errors | `compute_edits_diff`/`compute_edit_diff`/`EditDiffResult`/`EditDiffOutcome` |

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `edit-diff.ts:7-48` | Line endings, LF normalization, fuzzy normalization | Shared agent-core functions; focused `fuzzy_normalization_applies_nfkc_trim_end_quotes_dashes_and_spaces` and differential normalize cases |
| `edit-diff.ts:50-232` | Line spans, replacement ranges, reverse replacement, fuzzy preservation, `fuzzyFindText` | Shared agent-core functions; focused `fuzzy_edits_preserve_unchanged_lines_from_the_original`, `fuzzy_find_prefers_exact_matches_and_reports_utf16_offsets`, and differential fuzzy apply/find cases |
| `edit-diff.ts:234-267` | `Edit`, `AppliedEditsResult`, `stripBom`, occurrence scan | Shared records/strip; coding-agent `find_text_occurrences`/`count_occurrences` |
| `edit-diff.ts:269-354` | Duplicate context, line numbers, truncation, single vs multi messages | `format_duplicate_match_contexts`/`line_number_at_offset`/`truncate_context_line`/`duplicate_error`; ported `edit-tool-duplicate-context.test.ts` cases and differential duplicate fixtures |
| `edit-diff.ts:356-418` | `applyEditsToNormalizedContent` | `apply_edits_to_normalized_content`; focused and differential cases |
| `edit-diff.ts:420-521` | Unified patch and display diff | Shared `generate_unified_patch` and `generate_diff_string` wrappers; focused patch/diff tests and differential corpus |
| `edit-diff.ts:523-645` | Preview API | `EditDiffResult`/`EditDiffOutcome`/`compute_edits_diff`/`compute_edit_diff`; focused ENOENT/EACCES/EISDIR tests and differential preview cases |
| `src/index.ts:293` | Package exports `EditDiffResult`, `generateDiffString`, `generateUnifiedPatch` | Crate-root re-exports; focused test imports through the crate root |

## Deviations and gaps

- `TextReplacement` uses byte offsets; the source uses UTF-16 indices. `fuzzy_find_text` still reports UTF-16 units, matching the source record; the coding-agent match path converts those offsets to bytes.
- `truncate_context_line` keeps whole scalars where the source can split a surrogate pair; the same approved class of deviation as `truncate_line`.
- Error codes are mapped from `io::ErrorKind`; only `ENOENT`, `EACCES`, `EISDIR`, `ENOTDIR`, `EEXIST`, and `EINVAL` are named, other read failures fall back to the Rust error text.
- `compute_edits_diff` is async (Tokio file I/O) and returns `Result<EditDiffOutcome, PathError>`; the path-resolution failure is the only `Err`, mirroring the source's uncaught throw from `resolveToCwd`.
