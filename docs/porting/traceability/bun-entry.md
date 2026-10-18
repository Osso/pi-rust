# bun/ entry shims traceability

Sources (pinned revision in `docs/porting/source-lock.json`):

- `packages/coding-agent/src/bun/cli.ts` (15 lines) → `crates/pi-coding-agent/src/bun/cli.rs`
- `packages/coding-agent/src/bun/register-bedrock.ts` (4 lines) →
  `crates/pi-coding-agent/src/bun/register_bedrock.rs`
- `packages/coding-agent/src/bun/restore-sandbox-env.ts` (36 lines) → not ported (gap)

## register-bedrock.ts

| TS step | Rust | Tests |
| --- | --- | --- |
| `setBedrockProviderModule(bedrockProviderModule)` | `register_bedrock()` (`Arc::new(bedrock_provider_module())`) | `register_bedrock_is_idempotent` |

The override slot and `bedrock_converse_stream_api()` are covered by `pi-ai`'s
`bedrock_provider` tests; this shim only wires the bundled module in.

## bun/cli.ts

| TS step | Rust | Tests |
| --- | --- | --- |
| `registerBunOAuthFlows()` | `pi_ai::bun_oauth::register_bun_oauth_flows()` | `bun_cli_entry_registers_and_hands_off_to_the_cli` (runs before the runner gate) |
| `restoreSandboxEnv()` | not ported (gap) | — |
| `await import("./register-bedrock.ts")` | `register_bedrock()` | `register_bedrock_is_idempotent` |
| `await import("../cli.ts")` | `run_cli_entry(...)` | `bun_cli_entry_registers_and_hands_off_to_the_cli`, `bun_cli_entry_runs_main_when_no_internal_runner_matches` |
| `process.title` / `process.emitWarning` | see `cli-entry.md` | — |

## Gaps

- **`restoreSandboxEnv()`** — repairs oven-sh/bun#27802 (empty `process.env` inside sandboxes
  under Bun compiled binaries). Rust binaries do not run under Bun, and edition 2024 forbids a
  safe `set_var`, so there is no equivalent work to perform. The existing coverage row for
  `bun/restore-sandbox-env.ts` stays `gap`.
- **`process.title` / `process.emitWarning`** — same deviations as `cli.ts` (see
  `cli-entry.md`).

## Proof

`cargo test -p pi-coding-agent --test bun_entry` (3 tests).
