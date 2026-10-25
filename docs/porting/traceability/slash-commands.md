# slash-commands.ts traceability

Source: `packages/coding-agent/src/core/slash-commands.ts` (48 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent-tools/src/core/slash_commands.rs`.

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `SlashCommandSource` union | `SlashCommandSource` enum with `as_str` | compile-time; used by `SlashCommandInfo` |
| `SlashCommandInfo` | `SlashCommandInfo` (`camelCase` serde, optional description) | serde field names asserted by the differential output shape |
| `BuiltinSlashCommand` | `BuiltinSlashCommand` | `builtin_commands_keep_source_order_and_argument_hints` |
| `BUILTIN_SLASH_COMMANDS` | `builtin_slash_commands()` | differential `coding-agent-slash-commands-oracle.ts` compares all 28 entries, order, descriptions, and hints |
| `Quit ${APP_NAME}` | `Quit {package_config().app_name}` | `quit_uses_the_package_app_name`; differential uses the same package.json resolution |

## Notes

- The TypeScript constant is module-evaluated once. Rust evaluates `package_config()` on each call, which is
  process-cached, so the observable value is identical.
- No branches or error paths exist beyond list construction.

## Baseline registry re-pin (2026-10-25)

Frozen built-in model/login records have no argumentHint. Native registry now leaves both hints absent; direct unit expectations follow the frozen-probed absent values. Exact full-record differential was RED.
