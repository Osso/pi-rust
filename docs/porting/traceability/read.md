# coding-agent read tool traceability

Pinned source: `packages/coding-agent/src/core/tools/read.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::read`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `read.ts:12-27` | Schema: `path` required; `offset`/`limit` optional numbers | `read_schema`; differential requests exercise both. |
| `read.ts:29-36` | Prompt snippet/guidelines | Not part of `AgentTool`; owned by the system-prompt slice. |
| `read.ts:47-58` | Compact read classification for `SKILL.md`, pi docs, and `AGENTS.md`/`CLAUDE.md` resources | `get_compact_read_classification`, `get_pi_docs_classification`, `format_compact_read_call`; `getNonVisionImageNote` remains a gap because `ctx.model` is absent from `AgentToolExecutionContext`. |
| `read.ts:60-170` | `ReadOperations` with `readFile`, optional `readRange`/`stat`/`detectImageMimeType`, `access` | `ReadOperations` + `LocalReadOperations`; `avoids_full_reads_for_oversized_files` covers the optional range path. |
| `read.ts:96-104` | `readLocalRange` reads `[start, end)` and returns the bytes actually read | `LocalReadOperations::read_range`. |
| `read.ts:112-118` | Options: `autoResizeImages` (default true), custom operations | `ReadToolOptions`; BMP conversion test exercises auto-resize. |
| `read.ts:124-157` | `formatReadLineRange`, `formatReadCall`, `formatReadResult` | `format_read_line_range`, `format_read_call`, `format_read_result`; renderer differential `read` cases. |
| `read.ts:159-164` | `trimTrailingEmptyLines` | `trim_trailing_empty_lines`. |
| `read.ts:180-211` | Oversized files (`size > 10KB`, offset ≤ 1, no small limit) use a bounded head range read and are marked `rangeLimited` | `can_use_head_range_read`; `avoids_full_reads_for_oversized_files`. |
| `read.ts:213-224` | Resolve path, `access` (R_OK), then detect image MIME | `execute_read`; `missing_files_report_enoent`. |
| `read.ts:305-318` | Aborted signal rejects with `Operation aborted` before work starts | `check_aborted`; `aborted_reads_return_the_source_error`. |
| `read.ts:322-361` | Image branch: process the image, emit a text note plus image block, or a text note with the omission message | `read_image`; `detects_images_from_file_magic`, `converts_bmp_files_to_png_attachments`. |
| `read.ts:364-373` | Text branch: decode UTF-8, split on `\n`, offset is 1-indexed and clamped at 0, out-of-range offset throws | `read_text`; `offset_beyond_end_reports_the_total_line_count` and differential cases. |
| `read.ts:374-414` | Range-limited, first-line-too-long, truncated, user-limited, and untruncated output notices | `render_read_output`; `truncates_files_exceeding_the_line_limit`, `truncates_when_the_byte_limit_is_exceeded`, `caps_oversized_user_limits_before_returning_content`, `offset_and_limit_select_lines_and_report_remaining`. |
| `read.ts:416-430` | Abort listeners removed; the promise rejects with the original error | Rust checks cancellation before and after each await. |
| `read.ts:431-465` | `renderCall`/`renderResult`/`createReadTool` | `render_call`, `render_result`, `create_read_tool_definition`, `create_read_tool`; renderer differential and `tool_definitions_expose_renderers`. |

`constrainedSampling` mirrors `getExperimentalToolSampling()` (`differential_tools_index::tool_definitions_expose_experimental_constrained_sampling`).

Differential: `oracle/coding-agent-read-tool-oracle.ts` executes the real pinned tool over shared temp fixtures (request `27200`, 11 cases) and compares content blocks, details, and error messages.

## Discrepancies

- `LocalReadOperations::access` uses `metadata` instead of `access(2)`. Both fail for missing and unsearchable paths; the difference is that a write-only file is treated as readable. Node's `fs.access` also only checks the real UID's permission bits, so no additional check is meaningful here.
- `offset`/`limit` are converted with JavaScript `Number` array-index semantics (truncate toward zero, non-finite as 0); non-number JSON values are ignored, which the typed API excludes.

## Q5 current-pin numeric slice (2026-10-23, module audit in progress)

Frozen 4f9e6f8a4 `read.ts:403–415,421–463`: offsets/limits keep their f64 values through Math.max/Math.min and continuation arithmetic. Array.slice alone coerces indices; negative end indices are relative to the array end. `differential_read_tool` now compares 18 cases, including fractional offsets/limits, negative limit, zero limit, huge exponential offset and the oversized-line fractional-index failure. ReadOperations stat/range values use f64; native local stat sizes convert at the OS boundary, and the sandbox adapter preserves numeric values.

DISCREPANCY (literal frozen behavior, not corrected): fractional/negative slice operands can produce fractional or counterintuitive remaining-line notices because notice arithmetic uses uncoerced values. At line436, a fractional startLine reads undefined from allLines even though slice truncated it; Buffer.byteLength(undefined) throws `The "string" argument must be of type string or an instance of Buffer or ArrayBuffer. Received undefined`. The native result reproduces that error rather than flooring the property index.

RED/GREEN: `/tmp/pi-rust-repin-core-tools/read-{red,green}-numbers.log`. Remaining current-pin read work includes optional model image note, agentConfigRoot factory, cancellation lifecycle, native readability/access and renderer branches. Legacy table above is not new-pin parity evidence.
