# run-plan traceability

Source: `packages/coding-agent/extensions/run-plan/src/index.ts` (167 lines).
Rust: `crates/pi-coding-agent/src/bundled_extensions/run_plan.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_run_plan.rs`.

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `findNextPlanItem` (regex, CRLF split, trim) | `find_next_plan_item` | differential command path + direct test |
| `activePlanEntryType` / `readPersistedActivePlan` | `ACTIVE_PLAN_ENTRY_TYPE`, `read_persisted_active_plan` | session_start restore step |
| `persistActivePlan` (object / null) | `persist_active_plan` | appendEntry calls in differential |
| `resolvePlanFile` (`PLAN.md` default, node `join`) | `resolve_plan_file`, `utils::paths::node_join` | differential |
| `buildPlanPrompt` | `build_plan_prompt` | differential message text |
| `planFileEnvValue` / clear+export `PLAN_*` | `plan_file_env_value`, `clear_plan_environment`, `export_plan_environment` | differential `env` steps |
| `submitNextPlanItem` (missing file, no item, followUp) | `submit_next_plan_item` | differential error/notify/message steps |
| `runPlan` idle gate, persist, editor clear | `run_plan` | differential blocked step |
| `completeMarkdownFiles` (`process.cwd`, localeCompare) | `complete_markdown_files` | differential completions |
| `session_start` / `agent_end` handlers | factory handlers | differential event steps |
| `run-plan` command metadata | `register_command` | registration differential |

Approved deviation (2026-10-18): `process.env` mutation uses
`pi_js_compat::process_env` because edition 2024 makes `std::env::set_var` unsafe and
the workspace forbids unsafe code. Child spawns build their environment from
`process_env::vars()`, so `PLAN_*` reaches bash/tool children.

Gaps:
- Environment reads outside child-process spawn paths are not migrated; follow-up.
- Completions use the process cwd, matching the source's `process.cwd()`.
