# coding-agent ls tool traceability

## Current-pin cycle (open)

Frozen `coding-agent/src/core/tools/ls.ts` at immutable4f9e6f8a4. [Current contract](../../specs/ls-tool.md); historical rows below are not new-pin closure.

Numeric REDs (`ls-numbers-red.log`, `ls-rtk-numbers-red.log`) prove Option<usize> silently defaulted valid fractional/negative/subnormal/infinite limits and erased RTK Array.slice behavior. Native now retains f64, uses JS number spelling for notices and the shared [slice index](../../specs/js-slice-index.md) for RTK output.17 injected +17 actual executable cases compare exact calls/content/details, including skipped stats and enough remaining entries for paging. GREEN8/0 with read numeric and ordinary ls regressions (`ls-numbers-green-1.log`). Abort continuation RED (`ls-abort-producer-red.log`) proves eight active-operation cases dropped their future after outer abort. Native read/ls now share the [first-winner retained-producer authority](../../specs/tool-promise-observer.md); no post-readdir abort check absent from frozen. Nine actual-frozen scenarios plus existing read abort differential GREEN2/0 (`ls-read-abort-green-1.log`). Renderer RED (`ls-render-sequence-red.log`) proves Text replacement, missing literal hint space, u64 warning coercion and zero treated as truthy. Native now reuses Text and preserves source numeric truthiness/spelling;16 actual-frozen call/result frames (including raw text, collapsed/expanded, both warning types and images) GREEN1/0 (`ls-render-sequence-green-3.log`). Nonfinite direct callback fixtures explicitly retain native numbers rather than JSON-wire null projection. RTK IO RED (`ls-rtk-io-red.log`) proves ordinary BOM/whitespace/quiet-exit message drift and a UTF-8 boundary panic after unbounded reads. RTK now extends the existing [shared child capture](../../specs/child-process-capture.md): independent stream caps, early termination/reaping, safe decode; ordinary outputs and labels stay frozen-exact. Seven actual frozen cases +native overflow safety and six shared child controls GREEN9/0 (`ls-rtk-io-green-2.log`). RTK validation RED (`ls-rtk-validation-red-2.log`) proves native NUL failures lost exact user-visible messages. Shared Node argv/cwd validation now preserves diagnostics and precedence; ten actual frozen ordinary/validation cases GREEN (`ls-rtk-validation-green.log`). This preserves error contracts, not env-name NUL truncation or another runtime oddity. Filesystem RED (`ls-filesystem-red.log`) reproduces EACCES and deterministic post-stat ENOENT/ENOTDIR message loss through actual local operations. Readdir now carries shared native `scandir` errors from both open/read phases until the frozen outer prefix; eight real-filesystem frozen cases GREEN (`ls-filesystem-green.log`), including post-exists disappearance, normal entries and broken/cyclic/invalid-UTF-8-name stat skips. Final symbol/branch closure pending.

Pinned source: `packages/coding-agent/src/core/tools/ls.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::ls`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `ls.ts:14-30` | Schema and prompt contribution | `ls_schema`; differential requests. |
| `ls.ts:32-64` | `LsOperations` (`exists`, `stat.isDirectory`, `readdir`) and local defaults | `LsOperations` + `LocalLsOperations`. |
| `ls.ts:128-168,225-227` | Fallible exists rejects unchanged before stat/readdir; abort wins over pending/rejecting exists | [Operation-error differential and mutation proof](core-ssh-errors.md#fallible-lsfind-operations). Historical cancel-on-abort mapping is superseded for current-pin ls by the retained-producer proof above. |
| `ls.ts:66-101` | Render helpers | `format_ls_call`, `format_ls_result`; renderer differential `ls` cases. |
| `ls.ts:103-121` | `canUseRtk` only for default operations; description limits | `can_use_rtk`; differential uses the filesystem path because `rtk` is absent. |
| `ls.ts:123-199` | `rtk ls` fast path, missing-binary fallback, stderr/stdout error text, entry limit, byte truncation | `execute_rtk_ls`, `run_rtk_ls`, `format_rtk_ls_output`; `ls_rtk.rs` exercises success, limit, failure, and PATH-without-rtk fallback through a child process. |
| `ls.ts:200-257` | Filesystem path: resolve, `Path not found`, `Not a directory`, `Cannot read directory`, case-insensitive locale sort, `/` suffix, skip unstatable entries | `list_with_operations`; `lists_dotfiles_and_directories_sorted_case_insensitively`, `reports_missing_and_non_directory_paths`, differential cases. |
| `ls.ts:258-281` | Entry limit and byte truncation notices/details in source order | `enforces_the_entry_limit_and_reports_it`, `truncates_by_byte_limit`; differential. |
| `ls.ts:283-326` | `executeRtkLs`, `runRtkLs`, error formatting, `createLsTool` | Covered above; `render_call`/`render_result`/`create_ls_tool_definition`/`create_ls_tool` cover the definition surface. |

Differential: `oracle/coding-agent-ls-tool-oracle.ts` (request `27400`, 7 cases) compares content and details byte-for-byte.

## Discrepancies

| Status | Runtime behavior | Classification / proof |
|---|---|---|
| gap | Node execFile's chunk-dependent UTF-16 overflow prefix | Not reproduced under user2026-10-23 native-runtime policy. Native caps each stream at configured bytes, safely decodes/clamps UTF-8 and terminates before producer completion. Ordinary output/error labels remain exact; see shared capture proof above. |
- `stat` errors on directory entries are skipped exactly like the source's `catch { continue; }`.
