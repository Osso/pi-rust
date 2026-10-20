# Extension examples D

Pinned source: `532f7d12` (source-lock.json), `packages/coding-agent/examples/extensions/`.
Opt-in source embedding via `#[path]`, explicit ExtensionRegistry factory, no default loading.

| Source / branches | Rust example | Behavioral proof |
| --- | --- | --- |
| kimi-deferred-tools.ts:12–60: Calculator constant; search miss, activation, already active; session reset | `kimi_deferred_tools.rs::kimi_deferred_tools_extension` | `example_kimi_deferred_tools::deferred_tools_activate_once_and_reset_on_session_start` |
| reload-runtime.ts:13–37: command reload; tool follow-up enqueue and result; delivery error | `reload_runtime.rs::reload_runtime_extension` | `example_reload_runtime` (2 cases) |
| shutdown-command.ts:13–63: quit, finish, deploy; updates present/absent; shutdown before final result | `shutdown_command.rs::shutdown_command_extension` | `example_shutdown_command` (2 cases) |
| send-user-message.ts:19–97: four commands, empty/JS whitespace, idle/busy, raw/structured content, steer/follow-up, errors | `send_user_message.rs::send_user_message_extension` | `example_send_user_message` (2 cases) |

Tests compare full ordered metadata/action/result transcripts against unchanged pinned TS examples loaded with the real ExtensionRunner/loader. The shared batch-A oracle and support are extended, not duplicated. Host actions are private fixture seams; model registry uses in-memory credentials, disables network. Shutdown records requests rather than exiting the test process; reload records the awaited host reload rather than replacing loaded code. No provider/config/DB state is used.

RED/GREEN evidence: `/tmp/pi-rust-ext-examples-d/ledger.md` and numbered logs. Initial RED: missing embedded module for each example. Typed Rust JSON inputs exclude invalid JS runtime parameter types; stack frames omitted only. Calculator returning 42 for every expression is source demo behavior, preserved (not an arithmetic implementation). No new dependencies/public APIs.

## Blocked

- `entry-renderer.ts:29`: `Date.prototype.toLocaleString()` needs pending en-US helper; stopped per instruction, no partial substitute.
- `message-renderer.ts:31`: `Date.prototype.toLocaleTimeString()` has no equivalent in current `pi-js-compat/src/date.rs` (local fields at lines 34–74 omit seconds and locale formatting). No locale/timezone hardcoding added; helper dependency blocks whole example.
- Custom footer and structured output: pending implementation in this slice.
