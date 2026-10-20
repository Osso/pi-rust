# SDK examples

## Native SDK configuration programs

| Target | Behavior |
| --- | --- |
| `sdk_02_custom_model` | Runtime lookup, authenticated model order, medium thinking |
| `sdk_03_custom_prompt` | Replace prompt, then append instructions |
| `sdk_04_skills` | Filter discovery and append a synthetic skill |

Run with `cargo run -p pi-coding-agent --example <target>`. Like 01,
entrypoints are opt-in SDK clients and may contact configured providers. Tests
call each exact program's `run` with private state and Faux providers; no live
configuration/DB/provider is used. Structured console values print as JSON.
See [02–13 traceability](../../../../docs/porting/traceability/sdk-examples.md).

## Minimal session

```text
cargo run -p pi-coding-agent --example sdk_01_minimal
```

`01_minimal.rs` uses `CreateAgentSessionOptions::default()`: normal SDK model,
configuration, tools, skills, extensions, and context discovery. Running it may
contact the configured provider and persist session state. It is not an offline
demo. It never invokes the Pi CLI.

The example streams only assistant text deltas, prompts
`What files are in the current directory?`, prints every resulting message as
one JSON value per line, adds a blank line, and disposes the session on success
or error. JSON preserves message values rather than Node's inspect formatting.
Failures retain `Error: <message>` and a failing exit status without Rust Debug
string quoting. JSON object formatting and omitted JavaScript stack frames were
approved on 2026-10-20; no other output normalization is applied.

Tests compile this exact file but call `run(options, writer)` with a Faux
provider, private state directories, static resources, and an explicit empty
extension registry. They do not exercise default user configuration or live
providers. See [traceability](../../../../docs/porting/traceability/example-sdk-minimal.md).
