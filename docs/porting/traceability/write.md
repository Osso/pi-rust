# coding-agent write tool traceability

Pinned source: `packages/coding-agent/src/core/tools/write.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::write`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `write.ts:13-21` | Schema: `path` + `content` strings | `write_schema`; differential write case. |
| `write.ts:23-26` | Prompt snippet/guidelines | Not part of `AgentTool`; owned by the system-prompt slice. |
| `write.ts:31-45` | `WriteOperations` (`writeFile`, recursive `mkdir`) and local defaults | `WriteOperations` + `LocalWriteOperations`; `writes_file_contents_and_creates_parent_directories`. |
| `write.ts:47-101` | Highlight-cache and render helpers | `format_write_call`/`format_write_result`; the cache is not kept (same rendered text); renderer differential `write` cases. |
| `write.ts:202-239` | Resolve path, serialize on the file mutation queue, abort checks before/after each await, create parent directories, write UTF-8, report `content.length` bytes | `execute_write`; `writes_file_contents_and_creates_parent_directories`, `write_reports_utf16_code_unit_length`, differential case. `utf16_len` supplies JavaScript `.length`. |
| `write.ts:240-273` | `renderCall`/`renderResult`/`createWriteTool` | `render_call`, `render_result`, `create_write_tool_definition`, `create_write_tool`; renderer differential and `tool_definitions_expose_renderers`. |

Differential: `oracle/coding-agent-edit-write-tool-oracle.ts` (request `27300`, 1 write case) compares the result JSON and the resulting file contents byte-for-byte.

## Discrepancies

- None observed.
