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
| `pirate.ts:15–47` | `pirate.rs::pirate_extension` | `example_pirate`: complete runner result (systemPrompt and collected messages); initial/off, on, repeated prompt calls, empty prompt, exact suffix, off again, isolated factory state |
| `system-prompt-header.ts:8–17` | `system_prompt_header.rs::system_prompt_header_extension` | `example_system_prompt_header`: empty/current prompts, UTF-16 astral/combining/newline count, shutdown removes status |

Typed native tool input excludes invalid JavaScript argument types per project
compatibility policy; valid string arguments preserve values. No source bug or
new semantic deviation identified in these four examples.

## Development proof

Exact commands, revisions/scopes, results and logs:
`/tmp/pi-rust-ext-examples-a/ledger.md`, `NN-*.log`.
Only `cargo test -p pi-coding-agent --test example_<name>` and formatting run.
Eight missing-module REDs observed before implementations. All eight targets
historically GREEN at `39d5054`; 10 tests passed, zero warnings. Shared output/clock fixture and
console error oracle changes invalidated the initial four GREENs; all eight
were refreshed with targeted commands (logs 21–28). Review follow-up changed the
shared fixture/oracle; those logs remain historical. Only the three requested
review targets were refreshed below. No broad suite or final gate run.

| Example | Missing-module RED runs | Final GREEN tests | Current log |
| --- | ---: | ---: | --- |
| hello | 1 | 1 | `21-green-hello.log` |
| session-name | 1 | 1 | `22-green-session_name.log` |
| pirate | 1 | 1 | `36-green-review-blind-spots.log` |
| system-prompt-header | 1 | 1 | `24-green-system_prompt_header.log` |
| bookmark | 1 | 2 | `25-green-bookmark.log` |
| event-bus | 1 | 1 | `36-green-review-blind-spots.log` |
| model-status | 1 | 3 | `36-green-review-blind-spots.log` |
| status-line | 1 | 1 | `28-green-status_line.log` |

Intermediate failures are retained in the ledger: shared fixture entry-ID/error
conversion errors (05–08) and invalid `switch` session-start reason in two tests
(18/20), corrected to the actual `resume` event. These were fixture errors,
not missing APIs or source discrepancies.

| Source / lines | Native entrypoint | Behavioral target / branches |
| --- | --- | --- |
| `bookmark.ts:12–50` | `bookmark.rs::bookmark_extension` | `example_bookmark`: absent assistant/labels; reverse scan past user/label entries; latest assistant; latest labeled non-assistant; successive removals; JS trim; generated milliseconds; set/remove errors |
| `event-bus.ts:12–43` | `event_bus.rs::event_bus_extension` | `example_event_bus`: before-context UI silence; ordered session-start context capture then emission; defaults/trim; incoming peer delivery; independent observer extension receives outgoing channel and full payload on real bus, including before context; unrelated channel; repeated session start |
| `model-status.ts:12–31` | `model_status.rs::{model_status_extension,model_status_with_output}` | `example_model_status`: default entrypoint executes in child test process; real stdout and UI compare with pinned TS for set/cycle/restore/fallback; absent/present previous model; Unicode identifiers; writer entrypoint proves notification/status/console order and console failure after UI effects |
| `status-line.ts:10–32` | `status_line.rs::status_line_extension` | `example_status_line`: Ready; zero-turn end; turn start/end colors; count survives session start and ignores turnIndex |

`model_status_extension` uses stdout; its example-local writer entrypoint lets
embedding hosts route console output. No core API was added. The test captures
complete log lines in the same ordered trace as UI effects for the writer
entrypoint. The default entrypoint's stdout is OS-pipe captured separately from
its UI trace; its cross-stream ordering is not inferred. `event_bus.rs`
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
No provider payload is involved in these examples. Pirate's complete real runner
result is serialized on both sides: effective systemPrompt and collected messages,
including absent result while disabled. No prompt-only projection remains.

## Accepted review blind spots: mutation proof

Every run used only:
`cargo test -p pi-coding-agent --test example_model_status --test example_pirate --test example_event_bus`.
Pinned TS examples stayed unchanged. Each temporary Rust example mutation was
reverted; all three example files are byte-identical to the base revision.

| Contract | Temporary mutation / observed RED | Restored GREEN |
| --- | --- | --- |
| Default model-status registration/stdout | No handler (32), then registered handler writing sink (33): default-entrypoint test fails; stdout empty vs four pinned TS lines | 36: 3 model-status tests pass |
| Complete pirate runner result | Unexpected message with unchanged enabled prompt (34), then message while disabled with no prompt (35): full-result equality fails | 36: 1 pirate test passes, enabled/disabled results compared |
| Outgoing event-bus delivery | `/emit` adds `unexpected:true` (31): own UI trace unchanged, independent observer full-payload comparison fails | 36: 1 event-bus test passes, exact channel and complete payload asserted |

Logs 31–35 exit 101 at behavioral assertions, not compilation. Cargo stops after
the failing target; no later target is claimed as exercised in that run. Log 36
exits 0: 5 tests pass, zero warnings. Ledger records base revision, exact working
diff snapshots/digests and restoration. Other examples remain outside this
follow-up's verification scope.

## UUID carrier follow-up: titlebar spinner (2026-10-25)

`uuid_source_boundaries` loads the actual frozen titlebar-spinner.ts and injects a setTitle crypto failure on the first interval frame (second setTitle overall). The process uncaughtException hook receives the original Error/message/code, proving :37–43 has no catch. The native timer therefore uses only the shared throw_callback_uncaught authority for this proven boundary; synchronous start/stop handlers return Result. Native process-hook test asserts the original payload and callcount2, with a bounded child. No ad-hoc panic implementation or catch_unwind is added. The initial native fixture could not compile with the old infallible match; this was a type-boundary RED, not a native process-result claim.

## Removed old-only Kimi example (2026-10-25)

`kimi-deferred-tools.ts` is absent from frozen `4f9e6f8a4` (tracked source
inventory and module-import RED). The opt-in Rust factory and its one
differential test were removed, rather than treating an absent source as
passing or borrowing a newer source. No frozen example behavior changed.

## Re-pin reload description (2026-10-25)

Frozen `reload-runtime.ts` describes both the command and tool as
"Reload extensions, skills, prompts, and themes". Native registrations had
added context files; both now match frozen. Both runtime differential tests
were RED on those exact strings; no assertion or behavior was weakened.
