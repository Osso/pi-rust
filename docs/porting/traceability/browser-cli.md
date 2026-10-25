# Browser CLI extension traceability

Source: `packages/coding-agent/extensions/browser-cli/src/index.ts`, common pin `4f9e6f8a4`. Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/browser_cli.rs`.

| Frozen symbol / branch | Native item | Proof |
|---|---|---|
| default factory executable-availability gate | `browser_cli_extension` | standalone `browser_cli_availability`: absent, non-executable and executable PATH entries |
| exported result/execution/batch interfaces and injected runner/options | native structs/type aliases, exact JsString output/argv/session | existing native/frozen callback differential plus raw-result serialization roundtrip |
| exported tool factory: metadata, seven guidelines, schema and sequential mode | `create_browser_cli_tool_definition`, `browser_cli_schema` | `suite_extensions::bundled_extensions_tools::browser_cli_matches_typescript` |
| exported dedicated session id: tool agent, runtime agent, main; absent/empty session throws | `browser_cli_session` | existing callback fixture checks exact session; real child environment test |
| ordered batch, stop on nonzero/null, args copied, abort observation | `run_browser_cli_batch`, `wait_for_abort` | frozen success/failure differential and native abort test |
| default runner: pre-abort, spawn args/session env, remove BROWSER_CLI_TAB, capture streams/exit code | `run_browser_cli_command` | real env child; existing injected-runner differential |
| active abort SIGTERM and child reaping; outer abort must not cancel runner cleanup | detached native runner task, shared TERM capture | `abort_keeps_default_runner_alive_to_send_sigterm_and_reap`: real Python child writes its SIGTERM marker |
| output stdout/stderr order, no separators, JS trim in errors, unknown exit code | `format_browser_cli_batch_output`, `format_browser_cli_failure` | differential plus BOM-trim and raw UTF-16 unit tests |

## Re-pin proof ledger

- RED `/tmp/bundled-browser-runtime-red.log`: frozen JS-whitespace trim and real-child SIGTERM behavior fail in old port.
- RED `/tmp/bundled-browser-availability-red.log`: old factory registers despite absent executable.
- RED `/tmp/bundled-browser-raw-red.log`: native runner result rejects raw UTF-16 stdout/stderr.
- Previous lane already proved dedicated session environment/signature propagation and exact guideline text; current audit retains those fixes.

The default process stdout/stderr decoder uses replacement on invalid UTF-8 bytes, as Node `setEncoding("utf8")` does; this is not a UTF-16-to-scalar projection. Injected runner results and all formatting remain native UTF-16. Abort rejection leaves the runner active to perform the frozen child-termination/settlement side effect. No real browser is invoked in these tests.

Gap: pathological inputs, inherited environment-name oddities and OS-fault shapes follow the shared parity exclusions; normal argv, environment and process lifecycle remain mapped above.

- GREEN `4508576e0`/`d5aba2b1e`: five native/process tests, standalone executable gate, two frozen/injected-runner tests and exact first-party registration pass. Logs: `/tmp/bundled-browser-{runtime,availability,differential,reaping}-green.log`, `/tmp/bundled-availability-registration-green.log`. The abort test observes both the SIGTERM marker and disappearance from `/proc`, proving reaping. The factory differential now calls the same exported tool factory as its TS oracle; default availability is separately tested. Clippy warning mode passes with zero browser warnings (`/tmp/bundled-browser-clippy.log`). Changed Rust lines reviewed with the readability checklist.
