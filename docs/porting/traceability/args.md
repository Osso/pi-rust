# Coding-agent CLI args traceability

Pinned source: `packages/coding-agent/src/cli/args.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent-data/src/cli/args.rs`.

## Common-pin renderer flag removal (2026-10-22)

Frozen args.ts has no `--tui-mode` option/help entry. Removed native parser branch, typed field and main/interactive plumbing. Existing differential requests28035–28037 cover old flag names through the frozen unknown-long-flag contract, retaining values and repeated/missing-value behavior. Native `removed_tui_mode_uses_unknown_flag_contract` asserts `--tui-mode fullscreen` is unknown data, not a renderer selector. RED `test-args-fullscreen-red.log` observed old native parse/help divergence before deletion.

## Exported symbols

| TypeScript | Rust | Proof |
|---|---|---|
| `Args` interface | `Args` struct (field order mirrors the interface; `Option` models `undefined`) | `tests/args.rs`, differential requests `28000..=28044` |
| `isValidThinkingLevel` | `is_valid_thinking_level` | `tests/args.rs`, differential request `28200` |
| `parseArgs` | `parse_args` | `tests/args.rs` (19 tests), differential requests `28000..=28044` |
| `printHelp` | `print_help` | `tests/args.rs::prints_help_with_and_without_extension_flags`, differential requests `28100..=28101` |
| `ExtensionFlag` type (`core/extensions/types.ts`) | `ExtensionFlag`, `ExtensionFlagType`, `ExtensionFlagDefault` | Differential requests `28100..=28101` |

## Branches

| Source lines | Behavior | Rust proof |
|---|---|---|
| 74-81 | `--help/-h`, `--version/-v`; `--mode` only accepts `text`/`json`/`rpc` and consumes its value even when invalid | parse cases `28001..=28003` |
| 82-84 | `--continue/-c`, `--resume/-r` | `28004` |
| 85-93 | Single-value flags consume only when a next argument exists | `28005`, `28007`, `28012..=28013` |
| 94-100 | `--append-system-prompt` appends; repeated values keep source order | `28006` |
| 101-113 | `--name/-n` and `--worktree/-w` emit `requires a value` errors when trailing | `28007` |
| 114-126 | Session flags (`--no-session`, `--session`, `--session-id`, `--fork`, `--session-dir`) | `28008` |
| 127-141 | `--models` trims every comma part; `--tools`/`--exclude-tools` also drop empty names | `28010..=28011` |
| 142-163 | Approval preset/policy consume only non-flag values; invalid values emit exact messages; `--dangerously-bypass-approvals` maps to `auto-approve` | `28014..=28019` |
| 164-172 | `--thinking` accepts the eight literals; invalid values warn with the joined list | `28020..=28022` |
| 173-178 | `--print/-p` consumes the next argument unless it starts with `@`, `-`, or is `---`-prefixed | `28023..=28025` |
| 179-209 | `--export`, repeated `--extension/-e`, `--skill`, `--prompt-template`, `--theme`, and the `no-*` disablers | `28026..=28030` |
| 210-223 | `--list-models` takes a search value only when the next argument is neither flag nor `@file` | `28031` |
| 224-241 | `tools`, `extensions`, `login <provider>`; `login` errors when the provider is missing or flag-like | `28032..=28034` |
| 242-256 | `--tui-mode` accepts `regular`/`fullscreen`, consumes invalid non-flag values, and reports missing/flag-like values without consuming | `28035..=28037` |
| 257-267 | `--verbose`, `--approve/-a`, `--no-approve/-na`, `--offline` | `28038..=28039` |
| 268-283 | `@` prefix strips one character into `fileArgs`; unknown long flags use `name=value` or consume one non-flag/non-`@` value, otherwise `true`; unknown short flags error; other arguments become messages | `28040..=28044` |
| 284-285 | Return the accumulated result | all parse cases |
| 290-299 | Extension flag block: ` <value>` for string flags, fallback `Registered by <path>`, 30-column pad | differential `28101` |
| 301-479 | Full help text including package identity, env var names, and tool list; `console.log` adds the final newline | differential `28100..=28101` |

## Semantics

- Unknown flags use `IndexMap`, matching JavaScript `Map` insertion order and last-value-wins replacement.
- `ListModels::All` models the TypeScript `true` union member; `list_models` keeps search strings verbatim.
- JavaScript `number` fields do not exist in this module, so no `f64` mapping is required.
- Typed Rust inputs exclude non-string argv elements (invalid JavaScript runtime input types).

## Discrepancies

None found; differential comparison is byte-exact for help output and structural for parsed values.

## Baseline registry re-pin (2026-10-25)

Frozen help has the plain config command, both terminal-export flags, and no Anthropic bearer/Baseten environment-variable lines. Restore those exact lines; parser behavior is unchanged. Existing full-help differential was RED; no expected strings were edited.
