# coding-agent find tool traceability

## Current-pin cycle (open)

Frozen `find.ts` at immutable4f9e6f8a4; [contract](../../specs/find-tool.md). Numeric RED (`find-numbers-red.log`) proves valid fractions/negative/nonfinite/subnormal/large limits were coerced to usize before the custom glob boundary, changing callback values, flags and notices. The native API now retains f64 throughout custom glob and fd argument/notices; the existing bwrap direct consumer forwards that number unchanged.17 actual-frozen custom cases plus four ordinary find controls GREEN5/0 (`find-numbers-green-1.log`). Parent cc27ef61 merged; six mechanical find borrow-only clippy findings removed without changing behavior. Path RED (`find-paths-red.log`) proves five current-pin mismatches: non-component prefix slice, process-relative results, literal POSIX backslashes/trailing normalization and UTF-16 surrogate cut. Shared prefix/relative logic now follows frozen exactly, with FD-only trailing-separator handling kept separate; path errors propagate instead of expect-panic. Eight frozen path cases +numeric17 +ordinary find4 +lossless-session18 GREEN24/0 (`find-paths-green.log`). Tools-manager whole current-pin prerequisite is closed. Actual-fd numeric/process and remaining module closure stay pending; historical rows below are not current-pin proof.

Pinned source: `packages/coding-agent/src/core/tools/find.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::find`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `find.ts:16-27` | `relativizeFindResultPath`: keep trailing separators, relativize absolute results, use `/` separators | `relativize_find_result_path`; custom-glob and differential cases. |
| `find.ts:29-55` | Schema, prompt contribution, default limit | `find_schema`; differential requests. |
| `find.ts:57-75` | `FindOperations` and default placeholder glob | `FindOperations` + `LocalFindOperations`; custom glob tests. |
| `find.ts:143-183,355-361` | Fallible exists/glob reject unchanged; exists precedes glob; abort wins over pending/rejecting operations | [Operation-error differential and mutation proof](core-ssh-errors.md#fallible-lsfind-operations). |
| `find.ts:77-129` | Render helpers | `format_find_call`, `format_find_result`; renderer differential `find` cases. |
| `find.ts:137-209` | Custom glob branch: existence check, ignore list, relativization, limit notice (without refine hint), byte truncation | `execute_find` custom branch; `custom_glob_operations_relativize_absolute_results`, `custom_glob_operations_report_missing_paths`. |
| `find.ts:211-268` | `fd` resolution, `--no-require-git` outside git repos, `--max-results`, `--full-path` pattern rewrite | `execute_find`, `inside_git_repo`; differential gitignore/path-pattern cases. |
| `find.ts:269-337` | Stream output lines, abort kill, nonzero-exit error text, `No files found matching pattern` | `run_fd`; differential invalid-glob and no-match cases. |
| `find.ts:339-367` | Limit notice with refine hint, byte truncation, details order | `relativize_output`; `reports_result_limits_and_flag_like_patterns`, differential limit case. |
| `find.ts:369-380` | `renderCall`/`renderResult`/`createFindTool` | `render_call`, `render_result`, `create_find_tool_definition`, `create_find_tool`; renderer differential and `tool_definitions_expose_renderers`. |

Differential: `oracle/coding-agent-find-tool-oracle.ts` (request `27600`, 9 cases) runs the real `fd` on both sides. Cases match at most one file because `fd` traversal order is nondeterministic.

## Current-pin source discrepancies

Frozen uses `p.startsWith(searchPath)` then slices exactly `searchPath.length + 1` UTF-16 units; it does not check a path-component boundary. `/repo/listx/alpha` under `/repo/list` therefore becomes `/alpha`; a following surrogate pair loses its high unit. Source's documented relative custom paths are resolved against process cwd by `path.relative`, not against the search root. These are classified source-logic quirks/bugs and intentionally preserved for parity, not silently corrected. The eight-case differential proves them directly; this is not Node cwd-cache emulation.

Windows separator/path semantics remain under the approved Windows-platform gap. Current cycle targets exact POSIX behavior; historical helper/row claims above are not authoritative for this pin.
