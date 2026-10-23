# coding-agent ls tool traceability

## Current-pin cycle (open)

Frozen `coding-agent/src/core/tools/ls.ts` at immutable4f9e6f8a4. [Current contract](../../specs/ls-tool.md); historical rows below are not new-pin closure.

Numeric REDs (`ls-numbers-red.log`, `ls-rtk-numbers-red.log`) prove Option<usize> silently defaulted valid fractional/negative/subnormal/infinite limits and erased RTK Array.slice behavior. Native now retains f64, uses JS number spelling for notices and the shared [slice index](../../specs/js-slice-index.md) for RTK output.17 injected +17 actual executable cases compare exact calls/content/details, including skipped stats and enough remaining entries for paging. GREEN8/0 with read numeric and ordinary ls regressions (`ls-numbers-green-1.log`). IO/abort/RTK stream/renderer closure remains pending; pre-repin cancel-on-abort approval below is not assumed valid for this pin.

Pinned source: `packages/coding-agent/src/core/tools/ls.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::ls`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `ls.ts:14-30` | Schema and prompt contribution | `ls_schema`; differential requests. |
| `ls.ts:32-64` | `LsOperations` (`exists`, `stat.isDirectory`, `readdir`) and local defaults | `LsOperations` + `LocalLsOperations`. |
| `ls.ts:128-168,225-227` | Fallible exists rejects unchanged before stat/readdir; abort wins over pending/rejecting exists | [Operation-error differential and mutation proof](core-ssh-errors.md#fallible-lsfind-operations). Post-abort continuation (TS still runs stat/readdir) is the approved cancel-on-abort deviation ([compatibility](../../specs/compatibility.md#out-of-scope)). |
| `ls.ts:66-101` | Render helpers | `format_ls_call`, `format_ls_result`; renderer differential `ls` cases. |
| `ls.ts:103-121` | `canUseRtk` only for default operations; description limits | `can_use_rtk`; differential uses the filesystem path because `rtk` is absent. |
| `ls.ts:123-199` | `rtk ls` fast path, missing-binary fallback, stderr/stdout error text, entry limit, byte truncation | `execute_rtk_ls`, `run_rtk_ls`, `format_rtk_ls_output`; `ls_rtk.rs` exercises success, limit, failure, and PATH-without-rtk fallback through a child process. |
| `ls.ts:200-257` | Filesystem path: resolve, `Path not found`, `Not a directory`, `Cannot read directory`, case-insensitive locale sort, `/` suffix, skip unstatable entries | `list_with_operations`; `lists_dotfiles_and_directories_sorted_case_insensitively`, `reports_missing_and_non_directory_paths`, differential cases. |
| `ls.ts:258-281` | Entry limit and byte truncation notices/details in source order | `enforces_the_entry_limit_and_reports_it`, `truncates_by_byte_limit`; differential. |
| `ls.ts:283-326` | `executeRtkLs`, `runRtkLs`, error formatting, `createLsTool` | Covered above; `render_call`/`render_result`/`create_ls_tool_definition`/`create_ls_tool` cover the definition surface. |

Differential: `oracle/coding-agent-ls-tool-oracle.ts` (request `27400`, 7 cases) compares content and details byte-for-byte.

## Discrepancies

- The `rtk` `maxBuffer` path is approximated by a 20KB post-read check; the notice path is not differentially exercised because `rtk` is not installed in the test environment.
- `stat` errors on directory entries are skipped exactly like the source's `catch { continue; }`.
