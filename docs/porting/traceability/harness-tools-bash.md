# Bash tool traceability

Pinned source: `packages/agent/src/harness/tools/bash.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-agent-core/src/harness/tools/bash.rs`. Behavioral proof: `tests/harness_tools.rs` bash tests. Differential: 7 bash fixtures in `tests/differential_harness_tools/`.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `bash.ts:9-10` | Max timeout and 100 ms update throttle | `MAX_TIMEOUT_SECONDS`/`BASH_UPDATE_THROTTLE_MS`; timeout tests |
| `bash.ts:12-15` | TypeBox schema with required `command`, optional `timeout` | `bash_schema`; schema test and differential comparison |
| `bash.ts:19-22` | `BashToolDetails { truncation?, fullOutputPath? }` | details JSON map; truncation tests |
| `bash.ts:24-28` | `BashExecution { command, cwd, env, inheritEnv }` | `BashExecution`; prepare test |
| `bash.ts:30-34` | `BashPrepare` hook | `BashPrepare` trait with `Result<(), String>`; prepare test and differential fixture |
| `bash.ts:36-39` | `commandPrefix` option | `BashToolOptions::command_prefix`; prefix test |
| `bash.ts:41-49` | Timeout validation errors | `validate_timeout`; timeout test |
| `bash.ts:56-63` | Description text | `DESCRIPTION`; schema test |
| `bash.ts:64-70` | Validate, build execution, run prepare | `execute_bash`; prepare test |
| `bash.ts:71-113` | Throttled partial updates with truncation details | `UpdateThrottle`/`progress_result`; truncation test asserts bounded updates and final content |
| `bash.ts:115-143` | Capture execution, append truncation notices, error/abort/timeout/exit mapping | `execute_bash`; exit/timeout/long-line/truncation/abort tests and differential fixtures |
| `bash.ts:145-158` | Final text or `(no output)` plus details | `execute_bash`; no-output test |

## Rust boundary and discrepancies

- Partial updates are emitted when at least 100 ms elapsed since the previous update; the source also schedules a trailing timer. The final capture update is always emitted, and output/details are identical.
- `BashPrepare` receives `ExecutionToolContext` (the harness `TContext` layer is unported) and returns `Result<(), String>` for thrown errors.
- The source's `Error` causes are not preserved in `ToolError`; only the exact message is observable through the tool boundary.
