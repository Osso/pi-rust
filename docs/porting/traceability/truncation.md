# Agent-core truncation traceability

Pinned source: `packages/agent/src/harness/utils/truncate.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Focused Rust proof covers the shared valid-string and non-negative-integer domain. Direct TypeScript differential proof is pending for this production commit.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `truncate.ts:11-13` | Defaults are 2,000 lines, 10 KiB, and 500 grep-line characters | Focused constants and `TruncationOptions::default` test |
| `truncate.ts:15-45` | Results expose content, cause, full counts, partial flags, and applied limits | Representative full-struct equality across no-truncation, head, and tail branches |
| `truncate.ts:54-80` | Count UTF-8 bytes, including two-, three-, and four-byte characters | Focused Unicode, combining-mark, NUL, supplementary, CRLF, and byte-boundary tests; lone surrogates excluded |
| `truncate.ts:82-87` | Split only on LF; empty content has zero lines; terminal LF adds no line | Focused empty, CRLF, and trailing-LF metadata tests |
| `truncate.ts:89-110` | Replace selected unpaired surrogates in tail output | Approved exclusion: Rust strings cannot contain those source inputs |
| `truncate.ts:115-123` | Format bytes below 1 KiB as B, below 1 MiB as one-decimal KB, otherwise one-decimal MB | Focused threshold, exact-boundary, and half-rounding fixtures |
| `truncate.ts:132-154` | Resolve limits and return the original content unchanged when both fit | Focused defaults/zero options plus full metadata with Unicode, CRLF, and trailing LF |
| `truncate.ts:157-172` | First head line above the byte limit returns empty byte-truncated output and sets its flag | Focused multibyte first-line fixture |
| `truncate.ts:175-196` | Head keeps complete lines, accounts for inserted LF bytes, and classifies the first reached limit | Focused byte, line, exact-tie, and zero-line fixtures |
| `truncate.ts:198-214` | Truncated head output is joined without a trailing LF and reports exact output metadata | Full-struct focused assertions |
| `truncate.ts:222-244` | Tail limit resolution and no-truncation behavior match head | Focused no-truncation metadata fixture |
| `truncate.ts:247-277` | Tail walks backward, accounts for LF bytes, permits one partial final line, and reclassifies line-limit ties | Focused line, byte, multi-line, empty-partial, and zero-byte fixtures |
| `truncate.ts:279-295` | Truncated tail output is joined without a trailing LF and reports exact metadata | Full-struct focused assertions |
| `truncate.ts:301-336` | Tail suffix respects valid UTF-8 character boundaries and can return empty when no character fits | Focused two-/four-byte suffix and oversized-scalar fixtures; surrogate inputs excluded |
| `truncate.ts:342-350` | Single-line truncation uses a default limit and exact marker | Focused default, exact, over-limit, zero, and custom-limit fixtures; Rust scalar counting is an approved deviation from UTF-16 units |

Approved deviations, August 12, 2026:

- Rust limits are `usize`, excluding negative, fractional, NaN, and infinite JavaScript values. The pinned source accepts those values and can return odd results or throw for empty input; those are classified source bugs outside the typed Rust API.
- Rust `truncate_line` counts Unicode scalar values. The source counts UTF-16 code units and can emit a lone surrogate when slicing a supplementary character. Rust preserves the complete scalar instead.

Preserved source quirks:

- No-truncation paths preserve an original trailing LF; truncated paths drop it.
- An empty tail suffix created by byte truncation still reports one output line.
- Tail truncation may report `Lines` after a byte-triggered empty partial suffix when `max_lines` is also reached.
