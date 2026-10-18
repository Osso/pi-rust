# coding-agent read tool traceability

Pinned source: `packages/coding-agent/src/core/tools/read.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::read`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `read.ts:12-27` | Schema: `path` required; `offset`/`limit` optional numbers | `read_schema`; differential requests exercise both. |
| `read.ts:29-36` | Prompt snippet/guidelines | Not part of `AgentTool`; owned by the system-prompt slice. |
| `read.ts:47-58` | Compact read classification for `SKILL.md`, pi docs, and `AGENTS.md`/`CLAUDE.md` resources | **Gap: renderers pending theme.ts** (`get_compact_read_classification`). |
| `read.ts:60-170` | `ReadOperations` with `readFile`, optional `readRange`/`stat`/`detectImageMimeType`, `access` | `ReadOperations` + `LocalReadOperations`; `avoids_full_reads_for_oversized_files` covers the optional range path. |
| `read.ts:96-104` | `readLocalRange` reads `[start, end)` and returns the bytes actually read | `LocalReadOperations::read_range`. |
| `read.ts:112-118` | Options: `autoResizeImages` (default true), custom operations | `ReadToolOptions`; BMP conversion test exercises auto-resize. |
| `read.ts:124-157` | Render-only helpers (`formatReadLineRange`, `formatReadCall`, `formatReadResult`, `getNonVisionImageNote`) | **Gap: renderers pending theme.ts**. The non-vision note needs `ctx.model`, absent from `AgentToolExecutionContext`. |
| `read.ts:159-164` | `trimTrailingEmptyLines` | **Gap: renderers pending theme.ts**. |
| `read.ts:180-211` | Oversized files (`size > 10KB`, offset ≤ 1, no small limit) use a bounded head range read and are marked `rangeLimited` | `can_use_head_range_read`; `avoids_full_reads_for_oversized_files`. |
| `read.ts:213-224` | Resolve path, `access` (R_OK), then detect image MIME | `execute_read`; `missing_files_report_enoent`. |
| `read.ts:305-318` | Aborted signal rejects with `Operation aborted` before work starts | `check_aborted`; `aborted_reads_return_the_source_error`. |
| `read.ts:322-361` | Image branch: process the image, emit a text note plus image block, or a text note with the omission message | `read_image`; `detects_images_from_file_magic`, `converts_bmp_files_to_png_attachments`. |
| `read.ts:364-373` | Text branch: decode UTF-8, split on `\n`, offset is 1-indexed and clamped at 0, out-of-range offset throws | `read_text`; `offset_beyond_end_reports_the_total_line_count` and differential cases. |
| `read.ts:374-414` | Range-limited, first-line-too-long, truncated, user-limited, and untruncated output notices | `render_read_output`; `truncates_files_exceeding_the_line_limit`, `truncates_when_the_byte_limit_is_exceeded`, `caps_oversized_user_limits_before_returning_content`, `offset_and_limit_select_lines_and_report_remaining`. |
| `read.ts:416-430` | Abort listeners removed; the promise rejects with the original error | Rust checks cancellation before and after each await. |
| `read.ts:431-465` | `renderCall`/`renderResult`/`createReadTool` | **Gap: renderers pending theme.ts**; `ReadTool` is the `AgentTool` equivalent of `createReadTool`. |

Differential: `oracle/coding-agent-read-tool-oracle.ts` executes the real pinned tool over shared temp fixtures (request `27200`, 11 cases) and compares content blocks, details, and error messages.

## Discrepancies

- `LocalReadOperations::access` uses `metadata` instead of `access(2)`. Both fail for missing and unsearchable paths; the difference is that a write-only file is treated as readable. Node's `fs.access` also only checks the real UID's permission bits, so no additional check is meaningful here.
- `offset`/`limit` are converted with JavaScript `Number` array-index semantics (truncate toward zero, non-finite as 0); non-number JSON values are ignored, which the typed API excludes.
