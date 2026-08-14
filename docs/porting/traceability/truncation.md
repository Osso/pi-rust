# Agent-core truncation traceability

Pinned source: `packages/agent/src/harness/utils/truncate.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Direct differential proof invokes the real pinned module through one persistent `tsx` process. Thirty-four oracle requests cover 3,040 behavioral cases: eight size formats (`10000..=10007`), eighteen targeted head/tail results (`10008..=10025`), five shared line results (`10026..=10030`), one approved UTF-16 discrepancy (`10031`), one Buffer-disabled fallback case (`10032`), and a batch of 3,007 deterministic valid-Unicode tail boundaries (`10033`).

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `truncate.ts:11-13` | Defaults are 2,000 lines, 10 KiB, and 500 grep-line characters | Focused constants/default test plus direct default-option head/tail fixtures |
| `truncate.ts:15-45` | Results expose content, cause, full counts, partial flags, and applied limits | Focused full-struct equality plus direct full-object comparison for all targeted and corpus cases |
| `truncate.ts:54-80` | Use `Buffer.byteLength` when available; otherwise count UTF-8 bytes across ASCII, two-, three-, and four-byte characters | Focused Unicode tests, direct normal-runtime cases, one exact-source VM execution with `Buffer` absent, and 3,007 valid-Unicode boundary cases; lone surrogates excluded |
| `truncate.ts:82-87` | Split only on LF; empty content has zero lines; terminal LF adds no line | Focused and direct empty, consecutive-empty, CRLF, and trailing-LF metadata cases |
| `truncate.ts:89-110` | Replace selected unpaired surrogates in tail output | Approved exclusion: Rust strings cannot contain those source inputs |
| `truncate.ts:115-123` | Format bytes below 1 KiB as B, below 1 MiB as one-decimal KB, otherwise one-decimal MB | Focused and direct eight-case threshold, exact-boundary, and half-rounding matrix |
| `truncate.ts:132-154` | Resolve limits and return the original content unchanged when both fit | Focused and direct default/zero options plus Unicode, CRLF, empty, and trailing-LF full metadata |
| `truncate.ts:157-172` | First head line above the byte limit returns empty byte-truncated output and sets its flag | Focused and direct multibyte first-line fixture |
| `truncate.ts:175-196` | Head keeps complete lines, accounts for inserted LF bytes, and classifies the first reached limit | Focused and direct byte, line, exact-tie, zero-line, empty-line, NUL, and supplementary fixtures |
| `truncate.ts:198-214` | Truncated head output is joined without a trailing LF and reports exact output metadata | Focused full structs and direct normalized object equality |
| `truncate.ts:222-244` | Tail limit resolution and no-truncation behavior match head | Focused and direct default/no-truncation metadata fixtures |
| `truncate.ts:247-277` | Tail walks backward, accounts for LF bytes, permits one partial final line, and reclassifies line-limit ties | Focused and direct line, byte, multi-line, empty-partial, zero-byte, CRLF, trailing-LF, and deterministic corpus cases |
| `truncate.ts:279-295` | Truncated tail output is joined without a trailing LF and reports exact metadata | Focused full structs plus direct full-result equality across targeted and corpus cases |
| `truncate.ts:301-336` | Tail suffix respects valid UTF-8 character boundaries and can return empty when no character fits | Focused suffix/oversized-scalar cases plus 3,007 direct valid-Unicode byte-limit boundaries; surrogate inputs excluded |
| `truncate.ts:342-350` | Single-line truncation uses a default limit and exact marker | Focused and direct shared default/exact/over/zero/custom cases; source UTF-16 units are returned separately for the pinned supplementary-character discrepancy |

Separate coding-agent boundary:

- `packages/coding-agent/src/core/tools/truncate.ts:61-69` `formatSize` is covered separately by request `13185`. Its Rust wrapper delegates to the identical approved agent-core `usize` formatter; this does not extend coverage to other coding-agent truncation APIs.

Approved deviations, August 12, 2026:

- Rust limits are `usize`, excluding negative, fractional, NaN, and infinite JavaScript values. The pinned source accepts those values and can return odd results or throw for empty input; those are classified source bugs outside the typed Rust API.
- Rust `truncate_line` counts Unicode scalar values. The source counts UTF-16 code units and can emit a lone surrogate when slicing a supplementary character. Rust preserves the complete scalar instead.

Preserved source quirks:

- No-truncation paths preserve an original trailing LF; truncated paths drop it.
- An empty tail suffix created by byte truncation still reports one output line.
- Tail truncation may report `Lines` after a byte-triggered empty partial suffix when `max_lines` is also reached.
