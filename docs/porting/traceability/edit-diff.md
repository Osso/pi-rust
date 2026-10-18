# Edit-diff traceability

## Agent-core `edit_diff`

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

## Coding-agent `edit_diff`

Pinned source: `packages/coding-agent/src/core/tools/edit-diff.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::edit_diff`. Line-ending, BOM, `Edit`, `AppliedEditsResult`, and `FuzzyMatchResult` values are reused from the already-ported `pi_agent_core::harness::tools::edit_diff` because those declarations are textually identical. Everything else is ported separately from the coding-agent source, including the duplicate-context argument the coding-agent version adds to `getDuplicateError`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `edit-diff.ts:7-13` | `detectLineEnding` | Re-export of `detect_line_ending`; agent-core [edit-line-endings](edit-line-endings.md) proof plus differential request `27005` preview fixtures (CRLF file) |
| `edit-diff.ts:15-21` | `normalizeToLF`, `restoreLineEndings` | Re-exports; same proof |
| `edit-diff.ts:23-44` | `normalizeForFuzzyMatch`: NFKC, per-line `trimEnd`, quotes, dashes, spaces | `normalize_for_fuzzy_match` with `icu_normalizer` NFKC and the shared JavaScript trim set; focused test and differential normalize cases (ligature, fullwidth, combining mark, NEL, BOM) |
| `edit-diff.ts:46-48` | `splitLinesWithEndings` (`/[^\n]*\n|[^\n]+/g`) | `split_lines_with_endings`; exercised by every fuzzy replacement case |
| `edit-diff.ts:50-72` | `LineSpan`, `MatchedEdit`, `TextReplacement`, duplicate-context constants | `LineSpan`, private `MatchedEdit`, public `TextReplacement` (byte offsets), constants |
| `edit-diff.ts:74-116` | Line spans and the replacement line range, including both out-of-range errors | `get_line_spans`, `replacement_line_range`; fuzzy differential cases cover grouped replacements |
| `edit-diff.ts:118-131` | Reverse-order replacement application with an offset | `apply_replacements` |
| `edit-diff.ts:133-193` | `applyReplacementsPreservingUnchangedLines`: line-count guard, grouping by sorted match index, original lines preserved | `apply_replacements_preserving_unchanged_lines`, `group_replacements`; focused `fuzzy_edits_preserve_unchanged_lines_from_the_original` and differential fuzzy apply case |
| `edit-diff.ts:195-232` | `FuzzyMatchResult` and `fuzzyFindText` with exact-first and normalized fallback | `fuzzy_find_text`; offsets reported in UTF-16 units; focused astral-index test and differential fuzzyFind cases |
| `edit-diff.ts:234-246` | `Edit`, `AppliedEditsResult`, `stripBom` | Re-exports; focused `strip_bom_is_reexported_for_the_coding_agent_module` |
| `edit-diff.ts:248-267` | Non-overlapping occurrence scan and fuzzy occurrence count | `find_text_occurrences`, `count_occurrences` |
| `edit-diff.ts:269-283` | Line number at offset; context line truncation to 160 units with `…` | `line_number_at_offset` (LF byte counting), `truncate_context_line` |
| `edit-diff.ts:285-322` | Duplicate context blocks: up to 5 matches, 2 context lines, `>` markers, width padding, omission note | `format_duplicate_match_contexts`; ported `edit-tool-duplicate-context.test.ts` cases |
| `edit-diff.ts:324-354` | Not-found, duplicate, empty, and no-change messages, single vs multi variants | `not_found_error`, `duplicate_error`, `empty_old_text_error`, `no_change_error`; focused message tests and differential apply cases |
| `edit-diff.ts:356-418` | `applyEditsToNormalizedContent`: LF normalization, empty check, fuzzy base selection, duplicate check, overlap check, fuzzy preservation, no-change check | `apply_edits_to_normalized_content`; focused and differential cases |
| `edit-diff.ts:420-427` | `generateUnifiedPatch` via `diff@8.0.4` `createTwoFilesPatch` with `FILE_HEADERS_ONLY` | `generate_unified_patch` over the private `jsdiff` port; focused patch tests and differential patch corpus |
| `edit-diff.ts:429-521` | `generateDiffString` line-numbered output with context trimming and skip markers | `generate_diff_string`; focused tests and differential diff corpus |
| `edit-diff.ts:523-645` | `EditDiffResult`, `EditDiffError`, `computeEditsDiff`, `computeEditDiff`: resolve, readability check, BOM strip, LF normalize, apply, diff, error mapping | `EditDiffResult`, `EditDiffOutcome`, `compute_edits_diff`, `compute_edit_diff`; focused ENOENT/EACCES/EISDIR tests and differential preview cases |
| `src/index.ts:293` | Package exports `EditDiffResult`, `generateDiffString`, `generateUnifiedPatch` | Crate-root re-exports; focused test imports through the crate root |

## jsdiff port

`src/core/tools/jsdiff.rs` ports `diffLines` (tokenize on `\n`/`\r\n`, empty-token removal, Myers with the source's diagonal pruning and tie-breaking, component flattening) and `structuredPatch`/`formatPatch` for the two-file, headers-only form. Differential coverage: 700 seeded diff/patch cases over a duplication-heavy line alphabet plus targeted empty, append-only, deletion-only, CRLF, and no-newline cases.

## Deviations and gaps

- `TextReplacement` uses byte offsets; the source uses UTF-16 indices. `fuzzy_find_text` still reports UTF-16 units, matching the source record.
- `truncate_context_line` keeps whole scalars where the source can split a surrogate pair; the same approved class of deviation as `truncate_line`.
- Error codes are mapped from `io::ErrorKind`; only `ENOENT`, `EACCES`, `EISDIR`, `ENOTDIR`, `EEXIST`, and `EINVAL` are named, other read failures fall back to the Rust error text.
- `compute_edits_diff` is async (Tokio file I/O) and returns `Result<EditDiffOutcome, PathError>`; the path-resolution failure is the only `Err`, mirroring the source's uncaught throw from `resolveToCwd`.
