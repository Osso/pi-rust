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

## Renderers and footer

| Source / branches | Rust example | Behavioral proof |
| --- | --- | --- |
| custom-footer.ts:14–64: toggle, branch subscription/disposal, active-branch assistant input/output/cost, 1000 threshold, model/no-model, branch/no-branch, spacing/truncation | `custom_footer.rs::custom_footer_extension`, `CustomFooter::render/dispose` | `example_custom_footer` (2 cases), widths 0/1/12/40/80, data changes while installed, redraw before restore and no redraw after restore |
| entry-renderer.ts:18–41: registered renderer, missing-data default, expanded date, durable append with trim/default text | `entry_renderer.rs::entry_renderer_extension` | `example_entry_renderer`, expanded/collapsed component lines at 12/40/80 columns, default and Unicode command data |
| message-renderer.ts:16–59: level colors, optional/truthy expanded time, fixed box padding, level prefix parsing, custom displayed messages | `message_renderer.rs::message_renderer_extension` | `example_message_renderer`, default/warn/error/custom levels, zero/nonzero timestamp, frozen Box(1,1), fixed width 32; command whitespace/default/prefix branches |
| structured-output.ts:19–65: schema, prompt hints, final terminating result, details/fallback/empty render branches | `structured_output.rs::structured_output_extension` | `example_structured_output`, complete metadata + results, fixed widths 12/40/80 |

Date dependency resolved by merging master `d3680de` into this worktree (merge `3c88e7d`). Uses existing en-US `pi_js_compat::date::to_locale_string`; message time uses the suffix after its specified comma-space date separator. Local zone retained. Locale/unsupported-calendar limitations remain those of that helper; unsupported formatting fails explicitly instead of silently substituting UTC. Data interfaces require string text and numeric timestamps (invalid JavaScript runtime types remain excluded). No missing extension API.

The fixture replays native-generated clock timestamps into TS only; tests separately bound native values by wall-clock before/after. TS functions/renderers remain unchanged. UI components are compared as complete ANSI-bearing rendered lines, not source strings or implementation shapes. The footer uses private temporary git HEAD files, never the checkout's branch or live state. All TS oracle subprocesses run only under the Cargo runner isolation guard.

## Targeted proof results

Each example first failed compilation for its missing `#[path]` source module (8 RED invocations). Current behavioral/differential counts: custom_footer 2, entry_renderer 1, message_renderer 1, structured_output 1, kimi_deferred_tools 1, reload_runtime 2, shutdown_command 2, send_user_message 2: **12 passed, 0 failed**. GREEN logs 23–30; initial RED logs 01–04 and 09–12. Ledger records exact revisions, commands, superseded evidence, and current relevant file hashes. Final broad verification intentionally not run by this implementation slice.
