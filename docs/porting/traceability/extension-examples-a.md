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
Eight missing-module REDs observed before implementations. All eight targets
GREEN at `39d5054`; 10 tests passed, zero warnings. Shared output/clock fixture and
console error oracle changes invalidated the initial four GREENs; all eight
were refreshed with targeted commands (logs 21–28). Later documentation-only
changes do not invalidate those code proofs. No broad suite or final gate run.

| Example | Missing-module RED runs | Final GREEN tests | Current log |
| --- | ---: | ---: | --- |
| hello | 1 | 1 | `21-green-hello.log` |
| session-name | 1 | 1 | `22-green-session_name.log` |
| pirate | 1 | 1 | `23-green-pirate.log` |
| system-prompt-header | 1 | 1 | `24-green-system_prompt_header.log` |
| bookmark | 1 | 2 | `25-green-bookmark.log` |
| event-bus | 1 | 1 | `26-green-event_bus.log` |
| model-status | 1 | 2 | `27-green-model_status.log` |
| status-line | 1 | 1 | `28-green-status_line.log` |

Intermediate failures are retained in the ledger: shared fixture entry-ID/error
conversion errors (05–08) and invalid `switch` session-start reason in two tests
(18/20), corrected to the actual `resume` event. These were fixture errors,
not missing APIs or source discrepancies.

| Source / lines | Native entrypoint | Behavioral target / branches |
| --- | --- | --- |
| `bookmark.ts:12–50` | `bookmark.rs::bookmark_extension` | `example_bookmark`: absent assistant/labels; reverse scan past user/label entries; latest assistant; latest labeled non-assistant; successive removals; JS trim; generated milliseconds; set/remove errors |
| `event-bus.ts:12–43` | `event_bus.rs::event_bus_extension` | `example_event_bus`: before-context silence; ordered session-start context capture then emission; defaults/trim; real peer bus delivery; unrelated channel; repeated session start |
| `model-status.ts:12–31` | `model_status.rs::{model_status_extension,model_status_with_output}` | `example_model_status`: set/cycle/restore/fallback; absent/present previous model; Unicode identifiers; exact notification/status/console order; console failure after UI effects |
| `status-line.ts:10–32` | `status_line.rs::status_line_extension` | `example_status_line`: Ready; zero-turn end; turn start/end colors; count survives session start and ignores turnIndex |

`model_status_extension` uses stdout; its example-local writer entrypoint lets
embedding hosts route console output. No core API was added. The test captures
complete log lines in the same ordered trace as UI effects. `event_bus.rs`
accepts the source's declared `{message: string, from: string}` notification
domain; invalid JavaScript data types remain excluded by compatibility policy.

The bookmark oracle replays the generated native wall-clock milliseconds only;
the test independently bounds the timestamp by real before/after clock readings.
Random SessionManager entry IDs are compared by position in each manager's real
entry list; selected targets and durable in-memory labels are asserted.
No missing API, blocked example, source bug or unapproved semantic deviation
identified. Native embedding and typed-domain exclusions are established project
decisions. JS stack frames are omitted; source values/order/messages remain.
Console failure uses a deliberately failing host writer/console in both runtimes.
No provider payload is involved in these examples: pirate's real runner result
is the effective system-prompt transformation, compared byte-for-byte.
