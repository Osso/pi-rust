# dynamic-tools.ts → opt-in native example

Source: `coding-agent/examples/extensions/dynamic-tools.ts` (74 lines), pinned
`532f7d12`. Rust: `crates/pi-coding-agent/examples/extensions/dynamic_tools.rs`.
Load explicitly using the [native embedding pattern](../../../crates/pi-coding-agent/examples/extensions/README.md).
No default registry entry or TypeScript loading added.

| Source branch / symbol | Rust | Behavioral proof |
| --- | --- | --- |
| `ECHO_PARAMS` | identical message JSON schema | `example_dynamic_tools`: complete tool metadata comparison |
| `normalizeToolName` | JS trim, Unicode lowercase, ASCII name validation | blank/space, mixed case, digits/underscore, hyphen/internal space, non-ASCII, BOM cases |
| `registeredToolNames`, `registerEchoTool` | shared set, false on duplicate; same registration order | repeated command, preexisting `echo_session`, repeated startup |
| tool description, snippet, guideline, label | same values | complete ordered tool registry comparison after registration |
| `execute` | prefixed text and `{tool,prefix}` details | complete-result comparison: Unicode/newline, empty message, runtime tool |
| `session_start` | register `echo_session`, notify on every startup | two real runner event dispatches |
| `/add-echo-tool` invalid / duplicate / created | same usage, warning/info, normalized name | command handler transcript comparisons |

`tests/example_dynamic_tools.rs` compiles the shipped example and compares the
entire registration/tool/UI transcript with the actual unchanged pinned TS
example through `support/extension_examples_a.rs` and
`oracle/extension-examples-a-oracle.ts`. Queries and execution use the real runner,
not the factory's private registry. Runner-only host fixture supplies a no-op
`refreshTools`, matching Rust's no-session host actions; this is not session proof.

`agent_session_runtime::late_tools_refresh_the_real_session_without_reactivating_disabled_tools`
loads this example into a real AgentSession: startup registration updates session
metadata/active tools, a command registers a tool, and the actual wrapped agent
tool executes. A command registered by a running command is then invoked through
`AgentSession::prompt` slash-command routing. Existing inactive names stay inactive
on re-registration; a new
name becomes active. No model/network call required.

Mutation proof and command/revision ledger:
`/tmp/pi-rust-ext-runtime-registration/ledger.md`.

Typed API excludes invalid non-string message arguments; source TypeBox schema
requires strings. No semantic deviation, new dependencies, or source bug found.
See [live registration audit](extensions-loader.md#live-registration-identity-2026-10-20).
