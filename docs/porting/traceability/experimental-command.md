# coding-agent experimental command traceability

Pinned source: `packages/coding-agent/src/cli/experimental/command.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Differential proof: request `27010` builds the same option set in both implementations (string `--name`, numeric `--count` with a custom error, subcommand `server` with `--listen`) and compares `parse` results for 18 argv vectors: both value forms, duplicates, empty and missing values, `--`, unknown options, options after positionals, subcommand selection, and a leading dash. Focused tests: `crates/pi-coding-agent/tests/command.rs`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `NamedCommandInvocation` | `{command}` | `trait NamedCommandInvocation`; test `Invocation` |
| `CommandParseResult` / `CommandExecutionResult` / `CommandBuildResult` | `{ok:true,command}` or `{ok:false,errors}` | `CommandParseResult<T>` (builders and parse share it; execute returns `Result<T, Vec<String>>`); differential `27010` |
| `CommandOption` / `valueOption` / `stringOption` | Named option with a `parse(value)` result | `CommandOption<T>` with `Arc` parser, `value_option`, `string_option` |
| `ParsedCommandInput` | `remainingArgs`, `value(option)`, `values(option)` | `ParsedCommandInput` with `value`/`values` downcasting by option name |
| `Command.option` | Registers an option; duplicate throws | `option`; panic `Option <name> is already registered for <command>`; `duplicate_option_registration_panics` |
| `Command.build` | Builder producing the invocation or errors | `build`; `appends_builder_errors_after_parse_errors` |
| `Command.action` | Side effect for the selected invocation | `action`; `execute_runs_the_selected_action_with_the_context` |
| `Command.command` | Registers a subcommand; duplicate throws | `command(sub, convert)`; panic `Command <name> is already registered`; `duplicate_subcommand_registration_panics` |
| `Command.parse` | Selects a subcommand by `argv[0]`, else parses own options | `parse`; `selects_a_subcommand_by_its_first_argument` |
| `Command.execute` | Delegates to a subcommand, else parses and runs the action; missing action throws | `execute`; panic `Command <name> does not define an action`; `execute_without_action_panics` |
| `parseOwn` | Requires a builder; combines parse and builder errors in that order | `parse_own`; panic `Command <name> does not define a builder`; `parse_without_builder_panics` |
| `parseOptions` | `--name value`, `--name=value`, stop at `--`/unknown, `requires a value`, `may only be specified once`, option parser errors | `parse_options`; five focused tests, differential `27010` |

## Exclusions

- The source's `TInvocation` union is expressed by passing a conversion `Fn(TSub) -> TOwn` to `Command::command`; `TContext & TSubcommandContext` becomes one `TCtx` shared by parent and subcommands.
- Values are stored type-erased (`Any`) keyed by option name and recovered with `downcast_ref`, mirroring the source's `Map<string, unknown>`.

## Discrepancies

- Programmer errors (`Error` throws in the source) are Rust panics with the identical message text.
- The source's `parseOwn` has an unreachable `built.ok === false` branch after the error check; the Rust port reports the same errors and documents the branch as unreachable.
