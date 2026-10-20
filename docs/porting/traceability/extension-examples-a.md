# Extension examples A (Phase E4)

Pinned source: `532f7d12`, `packages/coding-agent/examples/extensions/`.
All examples are opt-in embeddable source under
`crates/pi-coding-agent/examples/extensions/`. Tests include the actual files
with `#[path]`, explicitly register factories in `ExtensionRegistry`, and load
through the real native runtime and `ExtensionRunner`. Nothing loads by default.
No core extension APIs, Cargo dependencies, provider calls or live state added.

`oracle/extension-examples-a-oracle.ts` imports the unchanged pinned examples,
loads them with the real pinned `loadExtensionFromFactory`, `ExtensionRunner`,
event bus and in-memory `SessionManager`. Host UI/action capture and theme colors
are fixture seams. Arrays preserve registration and action order; errors compare
messages without JS stack frames. The native model registry uses in-memory
credentials, no models path and network disabled. Native state remains private.

| Source / lines | Native entrypoint | Behavioral target / branches |
| --- | --- | --- |
| `hello.ts:8–26` | `hello.rs::hello_extension` | `example_hello`: tool metadata/schema; ASCII, empty and Unicode/newline names; full content/details |
| `session-name.ts:12–27` | `session_name.rs::session_name_extension` | `example_session_name`: absent/current names, JS whitespace vs U+0085, set/read order, setter failure/no notification |
| `pirate.ts:15–47` | `pirate.rs::pirate_extension` | `example_pirate`: initial/off, on, repeated prompt calls, empty prompt, exact suffix, off again, isolated factory state |
| `system-prompt-header.ts:8–17` | `system_prompt_header.rs::system_prompt_header_extension` | `example_system_prompt_header`: empty/current prompts, UTF-16 astral/combining/newline count, shutdown removes status |

Typed native tool input excludes invalid JavaScript argument types per project
compatibility policy; valid string arguments preserve values. No source bug or
new semantic deviation identified in these four examples.

## Development proof

Exact commands, revisions/scopes, results and logs:
`/tmp/pi-rust-ext-examples-a/ledger.md`, `NN-*.log`.
Only `cargo test -p pi-coding-agent --test example_<name>` and formatting run.
Four missing-module REDs observed before these implementations. GREEN pending.

Remaining batch: bookmark, event-bus, model-status, status-line.
