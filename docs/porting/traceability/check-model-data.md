# check-model-data.ts

Source: pinned `packages/ai/scripts/check-model-data.ts`. Rust: `crates/pi-ai/src/bin/check-model-data.rs`. [Contract](../../specs/model-data-tools.md); [validator mapping](model-data-tools.md).

| Source branch | Rust | Proof |
|---|---|---|
| Script-relative package root; no arguments | **blocked native contract decision**; current provisional command requires positional `<ai-package-root>` | explicit-root fixtures only; no default parity claim |
| `validateGeneratedModelData` success, console.log, exit 0 | success branch | `filesystem_structure_and_checker_success_match_pinned_modules`; `checker_exit_stdout_stderr_match_actual_pinned_script_on_fixtures` |
| Catch validation error, exact message + hydration guidance, exit 1 | failure branch | same checker differential, actual pinned source evaluated with fixture import-meta URL |

Run fixture-only proof and print real checker output:

```text
timeout 900 cargo test -p pi-ai --test model_data_tools checker_exit_stdout_stderr_match_actual_pinned_script_on_fixtures -- --nocapture
```

Explicit-root checker differential passes (`e9f2e4c`, expanded suite `2379d79`), including exact code 0/1 and stdout/stderr. This does not prove the no-argument workflow.

## Catalog/default boundary

Current command has no default root: zero/extra args emit `Usage: check-model-data <ai-package-root>` and exit 2. It supports only source-layout roots containing `src/models.generated.ts` exact `.models.ts` import lines, matching `src/providers/*.models.ts` shards and `src/providers/data/*.json`. It cannot validate the actual Rust snapshot's `data/providers` layout, which has no TS aggregator/shards. Native artifact/layout adaptation versus intentional source-layout-only command is a user-owned decision; no guessed default, fallback or extra flags are added. Coverage remains partial.

No hydration, catalog writes, credentials or network. Standalone login CLI gap remains unrelated. Rust typed errors exclude thrown JS non-Error values, which the imported source validator does not throw. Native usage failure (no/excess package-root arguments) emits usage and exits 2; TS has no corresponding argument surface.
