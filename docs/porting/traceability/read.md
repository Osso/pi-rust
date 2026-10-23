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

Numeric extension proof: `differential_read_numbers` runs 14 custom-operation cases against frozen, including fractional/NaN/infinite stat sizes, NaN/±Infinity options, absent range support and an empty range buffer. Observable operation order and non-finite runtime metadata are compared with explicit number tags before JSON wire encoding. Native totalBytes stores JsValue::Number directly so serde JSON projection cannot erase its NaN/Infinity before a built-in consumer reads it.

## Q5 native IO correction (in progress)

Frozen read.ts:95–99 uses R_OK, not stat-only existence. Local access now calls the native access syscall. Source readLocalRange (81–92) is one positioned read, not seek plus a fill loop, and explicitly closes the handle with finally precedence. Native range reads run on the blocking IO pool and retain open/read/close error origins. Removed old-only constrainedSampling from the native read tool.

`differential_read_filesystem` reproduces write-only-file access EACCES and directory-sniff EISDIR/read without a path against the frozen tool. Shared MIME async-file detection now carries the actual NativeNodeError record, rather than having read relabel every failure as open. Resulting code uses shared fd-read/close/open error records, not an EISDIR guard. RED log: read-red-filesystem.log.

## Q5 empty MIME truthiness (2026-10-23)

The custom detector may return an empty string. Frozen `if (mimeType)` treats it as text; native previously attempted image conversion. `differential_read_numbers` now compares 15 cases including that result and the complete operation trace. RED/GREEN: `read-empty-mime-{red,green}.log`.

## Q5 optional range operation (2026-10-23)

Frozen `readRange?:` presence is represented by `ReadOperations::read_range() -> Option<ReadRange<'_>>`. Removed the independent supports flag and default failing implementation. Local/sandbox providers and custom-operation fixtures supply the callback itself. Absent callbacks select readFile; present callbacks select the bounded range only when frozen numeric/stat conditions hold.

`read_tool::avoids_full_reads_for_oversized_files` failed before the change when an implemented callback lacked the unrelated flag (read-optional-range-red.log). GREEN: read_tool 12/12 and differential_read_numbers (14 frozen custom-operation cases, including absent range and nonfinite sizes); read-optional-range-green.log. Whole-module audit remains open.

## Q5 renderer branch corrections (2026-10-23)

`differential_tool_renderers::read_renderer_branch_matrix_matches_frozen` compares 17 frozen render branches: zero/fractional line ends, eight resource basenames, skill classification, old-only AGENTS.override.md, highlighted errors, collapsed error expansion spacing, and three truncation warning branches with missing maxBytes/maxLines. RED found seven divergent outputs; GREEN matches all rendered lines (read-renderer-{red,green}.log). Numeric zero/NaN end lines use frozen truthiness. Removed old-only resource classification and error-only highlighting suppression. Render truncation reads only displayed fields instead of deserializing unused metadata, so absent optional maxima receive frozen defaults.

## Q5 model image note (2026-10-23)

Frozen getNonVisionImageNote/execute reads the live optional context model after MIME detection. Definition execution now forwards that context; the plain AgentTool API remains context-free. `differential_read_model_note` compares eight processed/omitted-image cases with no context, text-only, image-only and mixed models. Both text-only branches failed RED and now append the exact frozen note; produced image content remains present. Logs read-model-note-{red-2,green}.log. No new model/provider execution API was introduced.

## Q5 agent-config definition (2026-10-23)

Added frozen createAgentConfigReadToolDefinition plus private config-root resolution. Canonical requested path/root are checked before R_OK, MIME and read. `differential_read_config_root` compares ten real-filesystem cases: normal/inside-symlink/root-alias reads, symlink/sibling/parent escapes, missing path/root realpath error origin, empty root and NUL validation. RED compared the prior unrestricted native definition and reproduced three unauthorized reads plus two incorrect error boundaries; the new factory matches all frozen results (read-config-root-{red,green}.log). No source consumer calls this factory at the pin; native export is module-local like frozen.

## Q5 abort settlement and background lifetime (2026-10-23)

Frozen Promise/onAbort settlement is separate from its async IIFE. Native execution with a signal now retains background work while a one-shot synchronous abort listener settles the returned result immediately. The same completion slot arbitrates success/error/abort; subscription removal precedes background settlement. The generic read abort message intentionally ignores the reason, matching frozen. An abort after path resolution prevents access; after access it prevents MIME; after MIME, subsequent stat/full/range/image work continues until frozen's final check. Late failures cannot overwrite an aborted promise.

`differential_read_abort` compares 12 gated frozen/native operation sequences (access/MIME/stat/full/range, successful and failing underlying IO, plus image-read/MIME continuations). It compares prompt rejection before gate release, eventual result and complete operation traces. All 12 failed RED (pending instead of immediate rejection; five late failures overwrote abort), now GREEN: read-abort-{red-2,green}.log. Existing pre-aborted read fixture remains the before-work proof.

## Q5 compact classification and resolution errors (2026-10-23)

`differential_read_classification` compares ten isolated-process branches with a shared explicit package root: README/docs/examples, literal POSIX backslashes, docs/resource/skill precedence, root/outside-root and deleted-CWD path resolution. Removed backslash-to-separator rewriting, silent `.ok()` classification suppression and resource-label fallback. Shared utils/paths process_cwd now uses the existing Node uv_cwd formatter instead of OS display text. RED found two changed labels and a swallowed deleted-CWD error; after propagation it exposed the wrong shared message. GREEN matches all ten (read-classification-{red-2,green,green-2}.log).
