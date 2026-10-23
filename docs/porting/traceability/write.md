# coding-agent write tool traceability

Current source: `packages/coding-agent/src/core/tools/write.ts` at immutable `4f9e6f8a4`. Whole-module re-pin in progress; historical evidence below is not closure.

Rust: `pi_coding_agent::core::tools::write`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `write.ts:13-21` | Schema: `path` + `content` strings | `write_schema`; differential write case. |
| `write.ts:23-26` | Prompt snippet/guidelines | Not part of `AgentTool`; owned by the system-prompt slice. |
| `write.ts:31-45` | `WriteOperations` (`writeFile`, recursive `mkdir`) and local defaults | `WriteOperations` + `LocalWriteOperations`; `writes_file_contents_and_creates_parent_directories`. |
| `write.ts:43–126` | Per-component full/incremental highlight cache | Native WriteCallRenderComponent retains raw path/language/content and line state; full rebuild, append and first50 prefix refresh. New actual-frozen sequence RED disproves the historical same-text-without-cache claim at sequence0/step1, an open comment beyond50lines. GREEN at3543237e; inherited-language Result cascade also GREEN atc70f430b. |
| `write.ts:202-239` | Resolve path, serialize on the file mutation queue, abort checks before/after each await, create parent directories, write UTF-8, report `content.length` bytes | `execute_write`; `writes_file_contents_and_creates_parent_directories`, `write_reports_utf16_code_unit_length`, differential case. `utf16_len` supplies JavaScript `.length`. |
| `write.ts:240-273` | `renderCall`/`renderResult`/`createWriteTool` | `render_call`, `render_result`, `create_write_tool_definition`, `create_write_tool`; renderer differential and `tool_definitions_expose_renderers`. |

`constrainedSampling` mirrors `getExperimentalToolSampling()` (`differential_tools_index::tool_definitions_expose_experimental_constrained_sampling`).

Differential: `oracle/coding-agent-edit-write-tool-oracle.ts` (request `27300`, 1 write case) compares the result JSON and the resulting file contents byte-for-byte.

## Current cycle

`differential_write_render_sequence` uses actual frozen components across partial appends, completion, path/language changes, replacement, invalid content and empty/plain text. RED: native full highlighting differs from frozen incremental colors after line50 (`write-render-sequence-red-2.log`). The cache is observable state, not an omitted performance optimization. Native now follows the frozen transition rules; streaming and inherited-language/catch differentials are GREEN (language-prototype-green-2.log). `differential_write_queue` adds eight actual-frozen gated-operation scenarios with real file effects: pre-abort, abort during mkdir/write, raw mkdir/write failure winning over abort, normal serialized success, independent-file progress and a raw surrogate filename. Native explicitly polls the blocked first/queued second futures before releasing the gate, avoiding an unpolled-second false positive. GREEN1/0 (`write-queue-green.log`). Sixteen-case actual-frozen default filesystem matrix now matches final contents, modes, root entries and exact failures. NUL/ELOOP RED at queue key realpath is fixed through the shared Node authority, not a write guard. Shared native realpath identity, local-path validation, queue, read config-root and write regressions:10/0 (`write-realpath-green-4.log`). Queue waiter proof now drives a later independent registration, ensuring the second call passed async realpath before releasing the first operation. Whole-write renderer/result reuse audit remains pending.

## Source quirks / classifications

- The success message labels `content.length` as bytes although it counts UTF-16 units. Preserve the frozen message/count; do not silently fix it to UTF-8 bytes.
- Frozen incremental highlighting independently colors the modified tail and refreshes only the first50-line prefix. Tail colors can differ from a full render; preserve this visible behavior, including complete-args rebuilds.

[Write contract](../../specs/write-tool.md).
