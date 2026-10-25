# browser-cli traceability

Source: `packages/coding-agent/extensions/browser-cli/src/index.ts` (169 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/browser_cli.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_tools.rs`.

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| default `browserCliExtension(pi, options)` | `browser_cli_extension` | registration differential |
| `createBrowserCliToolDefinition` | `create_browser_cli_tool_definition` | registration differential |
| schema (minItems, nested array, descriptions) | `browser_cli_schema` | registration differential |
| `BROWSER_CLI_PROMPT_GUIDELINES` | `BROWSER_CLI_PROMPT_GUIDELINES` | registration differential |
| `runBrowserCliBatch` ordering/stop-on-nonzero | `run_browser_cli_batch` | differential success/failure steps |
| `{ args, ...result }` execution shape | `BrowserCliCommandExecution` field order | differential details JSON |
| `formatBrowserCliBatchOutput` | `format_browser_cli_batch_output` | differential content text |
| `formatBrowserCliFailure` (stderr then stdout, unknown code) | `format_browser_cli_failure` | differential error text |
| `waitForAbort` / `throwIfAborted` | `wait_for_abort`, `run_browser_cli_command` | abort test (`browser-cli command aborted`) |
| default `runBrowserCliCommand` | `run_browser_cli_command` | implementation; tests inject a runner |

Gaps / discrepancies:
- Abort kills the child with SIGKILL via `Child::kill`; Node `kill()` sends SIGTERM.
  The observable abort error text matches.
- Output is drained by `wait_for_child_process`; Node concatenates stream chunks as
  they arrive. Byte content matches for deterministic commands.
- Spawn failures use Node-style `spawn <cmd> ENOENT`/`EACCES`; other OS errors pass
  through.

## Baseline display re-pin (2026-10-25)

Registration restores the frozen dedicated background-tab prompt guideline verbatim; existing browser_cli differential RED was only that missing line.

## Dedicated session tab and stale callback seam (2026-10-25)

Frozen `BrowserCliCommandRunner` receives `(executable, args, session, signal)`.
The old oracle treated the session string as the signal, producing a false
signal-presence mismatch. Both callback signatures now include session. Native
execution derives `sessionId:(toolExecutionAgentId ?? multiAgentAgentId ?? main)`
from the live context, removes inherited `BROWSER_CLI_TAB`, and supplies
`BROWSER_CLI_SESSION` to the real child. Missing/empty session context fails
with the exact frozen message. The existing native callback asserts the exact
session passed. A new real `env` child test reproduced missing session env RED.
No command/result assertions were relaxed. Mechanical test callback signatures
and the oracle's stale injection seam changed with the runtime API.
