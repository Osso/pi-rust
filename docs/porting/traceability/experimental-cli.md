# Coding-agent experimental CLI traceability

Pinned source: `packages/coding-agent/src/cli/experimental/{command-options,cli}.ts` and `cli/experimental/commands/{client,pi,server}.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/cli/experimental/{command_options,cli}.rs` and `cli/experimental/commands/{client,pi,server}.rs`.

Behavioral proof: `tests/experimental_cli.rs` (18 tests) ports every case of `test/experimental-cli-command.test.ts` and `test/experimental-cli-resolution.test.ts`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `command-options.ts:9-10` | `--auth-token` / `--auth-token-file` string options | `parses_authentication_sources`, mutual-exclusion cases |
| `command-options.ts:12-20` | `--listen`/`--connect` transport parsing reuses `parseTransportAddress` | listener/connect tests and the invalid-address matrix |
| `command-options.ts:22-30` | `parseAuth` delegates to `parseAuthInput` | authentication tests |
| `command-options.ts:32-39` | `parseLegacyOptions` parses remaining args and keeps only error diagnostics | existing-parser tests, `reports_existing_parser_errors_before_capability_errors` |
| `command-options.ts:41-44` | `unsupportedLegacyOptions` for non-empty remaining args | unsupported-option tests |
| `commands/client.ts:22-45` | Client command: connect/auth options, legacy errors, unsupported legacy, action | `parses_a_client_transport_address`, rejection matrix |
| `commands/pi.ts:23-51` | Pi command: listen/auth options, legacy options, `--connect` rejection | `composes_pi_command_options...`, `--connect=` case |
| `commands/server.ts:22-45` | Server command: listen/auth options, unsupported legacy | server listener and rejection tests |
| `cli.ts:6-8` | `experimentalCli` composes `pi` + `server` + `client` | `executes_the_parsed_command`, all parse tests |

## Semantics

- `ExperimentalCommand` is the union invocation (`Pi`/`Server`/`Client`); `Command::command` takes an explicit convert function where the TypeScript method widens the invocation union.
- `ExperimentalCliContext` holds the three action callbacks instead of an object with async methods; the ported `Command` actions are synchronous, so an async consumer dispatches from these callbacks.
- `AUTH_TOKEN_OPTION`/`AUTH_TOKEN_FILE_OPTION` are `LazyLock` statics mirroring the source's exported option constants.

## Discrepancies

None found.
