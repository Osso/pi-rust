# coding-agent code-index tools traceability

Pinned source: `packages/coding-agent/src/core/tools/code-index.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `pi_coding_agent::core::tools::code_index`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `code-index.ts:12-38` | Symbol/reference kind unions and the three schemas | `SYMBOL_KINDS`, `REFERENCE_KINDS`, `outline_schema`, `symbol_schema`, `references_schema`. |
| `code-index.ts:48-71` | `CodeIndexOperations.run` contract and local default | `CodeIndexOperations` + `LocalCodeIndexOperations`. |
| `code-index.ts:73-108` | Spawn `code-index`, accumulate stdout/stderr, abort kill | `run_code_index_command`; abort path in `execute_code_index`. |
| `code-index.ts:110-149` | Render helpers | `format_code_index_call`, `format_code_index_result`; renderer differential `outline`/`symbol`/`references` cases. |
| `code-index.ts:151-179` | Exit-code error precedence, `trimEnd`, `No results found`, byte truncation and notice | `execute_code_index`; `reports_stderr_on_failure`, `empty_stdout_reports_no_results`, differential cases. |
| `code-index.ts:181-211` | Generic definition factory and execute | Rust tools implement `AgentTool` directly. |
| `code-index.ts:213-329` | `outline`, `symbol`, and `references` argument builders and factories | `OutlineTool`, `SymbolTool`, `ReferencesTool`, plus `create_*_tool_definition`/`create_*_tool`; `outline_passes_optional_flags_and_trims_output`, `symbol_and_references_pass_filters`, renderer differential. |

Differential: `oracle/coding-agent-code-index-tools-oracle.ts` (request `27700`, 9 cases) runs the installed `code-index` CLI on both sides over a shared Rust fixture.

## Discrepancies

- Spawn failures report `Failed to run code-index: spawn code-index <CODE>` (Node's `error.message` shape); the local binary is expected on `PATH`.
