# check-model-data.ts

Source: pinned `packages/ai/scripts/check-model-data.ts`. Rust: `crates/pi-ai/src/bin/check-model-data.rs`. [Contract](../../specs/model-data-tools.md); [validator mapping](model-data-tools.md).

| Source branch | Rust | Proof |
|---|---|---|
| Script-relative package root | explicit positional `<ai-package-root>` for native maintenance command | every fixture process invocation |
| `validateGeneratedModelData` success, console.log, exit 0 | success branch | `filesystem_structure_and_checker_success_match_pinned_modules`; `checker_exit_stdout_stderr_match_actual_pinned_script_on_fixtures` |
| Catch validation error, exact message + hydration guidance, exit 1 | failure branch | same checker differential, actual pinned source evaluated with fixture import-meta URL |

Run fixture-only proof and print real checker output:

```text
timeout 900 cargo test -p pi-ai --test model_data_tools checker_exit_stdout_stderr_match_actual_pinned_script_on_fixtures -- --nocapture
```

Initial GREEN pending. No hydration, catalog writes, credentials or network. Standalone login CLI gap remains unrelated. Rust typed errors exclude thrown JS non-Error values, which the imported source validator does not throw. Native usage failure (no/excess package-root arguments) emits usage and exits 2; TS has no corresponding argument surface.
