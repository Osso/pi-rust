# Browser CLI batch tool

The bundled tool follows the [common source pin](../porting/source-lock.json); see [traceability](../porting/traceability/browser-cli.md).

## What it must do

- [x] Register only when browser-cli is executable; preserve frozen metadata/schema and sequential execution mode.
- [x] Select the dedicated session/agent tab, pass it to the command runner and child environment, and remove inherited BROWSER_CLI_TAB.
- [x] Run batches in order, preserve argv/results/output and exact UTF-16, and stop with the frozen stderr-first failure text on nonzero/null exit.
- [x] Reject aborted commands with the frozen text while allowing the runner to send SIGTERM and reap the real child.

## How it works

- [Module traceability](../porting/traceability/browser-cli.md)

## Implementation inventory

- `crates/pi-coding-agent-extensions/src/bundled_extensions/browser_cli.rs` — factory, native runner, batching and formatting.

## Tests asserting this spec

`crates/pi-coding-agent/tests/{bundled_extensions_tools,browser_cli_availability}.rs` and the browser module's process/UTF-16 unit tests.

## Known gaps (current cycle)

No current behavioral gap in the mapped module.

## Out of scope

No live browser automation is used. Pathological runtime/OS-fault shapes follow the shared parity bar.
