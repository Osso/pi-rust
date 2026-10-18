# Coding-agent edit-diff traceability

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
