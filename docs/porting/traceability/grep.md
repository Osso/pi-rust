# coding-agent grep tool traceability

Pinned source: `packages/coding-agent/src/core/tools/grep.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::grep`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `grep.ts:12-46` | Schema, prompt contribution, default limit | `grep_schema`; differential requests. |
| `grep.ts:48-69` | `GrepOperations` (`isDirectory`, `readFile`) and local defaults | `GrepOperations` + `LocalGrepOperations`. |
| `grep.ts:71-130` | Render helpers | **Gap: renderers pending theme.ts** (`formatGrepCall`, `formatGrepResult`). |
| `grep.ts:137-175` | Resolve `rg` via `ensureTool`, `Path not found`, context/limit normalization, relative-path formatting | `execute_grep`, `format_path`; `grep_rg.rs` missing-binary child test; differential path cases. |
| `grep.ts:177-198` | File line cache with CRLF normalization | `format_block` reads and normalizes per call (no cache; observable output identical). |
| `grep.ts:200-215` | `rg --json --max-columns 10240 --line-number --color=never --hidden`, optional flags/glob, `--` before pattern | `collect_matches`; `grep_rg.rs` asserts the argv with a fake `rg`. |
| `grep.ts:217-262` | Stream match events, count toward the limit, kill on limit, stderr capture | `collect_matches`; differential limit case. |
| `grep.ts:263-330` | Exit-code/abort handling, `No matches found`, context blocks, line truncation, byte truncation and notice order | `execute_grep`, `format_block`; differential context/truncation/long-line cases. |
| `grep.ts:350-399` | `renderCall`/`renderResult`/`createGrepTool` | **Gap: renderers pending theme.ts**; `GrepTool` is the `AgentTool` equivalent of `createGrepTool`. |

Differential: `oracle/coding-agent-grep-tool-oracle.ts` (request `27500`, 13 cases) runs the real `rg` on both sides. Cases use single-file or single-match patterns because `rg` traversal order across files is nondeterministic.

## Discrepancies

- None observed. `limit`/`context` are truncated to integers (the typed API excludes fractional JavaScript numbers).
