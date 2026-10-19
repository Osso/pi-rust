# throw audit: TypeScript throws ported as Rust panics

Cross-crate audit of `panic!`, `unreachable!`, `.unwrap()` and `.expect()` in
non-test production code of `pi-coding-agent`, `pi-ai`, `pi-agent-core` and
`pi-tui`, against the pinned source at `docs/porting/source-lock.json`.

Method: brace-aware census of `crates/*/src/**/*.rs` (test modules excluded),
then each site classified by its TypeScript counterpart. A site is in scope when
the TypeScript code (or a Node API it calls) throws. The Rust port must return an
error to the same boundary: a `try/catch` in the caller, a rejected promise, or
the process-level uncaught handler.

Census after this slice: **449 sites** (300 coding-agent, 73 ai, 28 agent-core,
48 tui), of which **23** are `panic!`/`unreachable!` (see
[Kept panics](#kept-panics)) and **426** are `unwrap`/`expect`:

| Category | Count | Why it is not a ported throw |
|---|---:|---|
| Mutex/RwLock lock poison | 130 | Rust-only; TypeScript has no lock poisoning |
| Static regex compilation (`Regex::new` in `LazyLock`/`get_or_init`) | 42 | Rust-only translation of marked/extension patterns |
| `serde_json::to_value`/`to_string` of typed records | 40 | Infallible for these types; TypeScript stringifies without validation |
| `try_into` on length-checked slices | 10 | Frame/prelude parsing after an explicit length check |
| `getrandom::fill` | 9 | OS entropy; Node `randomBytes` has the same failure mode |
| `from_utf8` on `valid_up_to` prefixes | 8 | `Utf8Error::valid_up_to` guarantees the prefix |
| Option/Result after an explicit check (`expect("checked above")`, `is_some_and` guards) | 187 | Typed-API exclusion (August 12, 2026 decision) |

## Scope notes

- `let _ =` swallows of Node-API failures (for example the `pi-tui` debug/crash
  log writes) are outside this panic census; Node fs error-text deduplication is
  owned by the in-flight `ca-errno` slice.
- Two sites named in the slice brief are test-only in this revision:
  `core/session_control_db/mod.rs` (the `instance id must be an object` panic is inside
  `#[cfg(test)] mod tests`) and `modes/interactive/interactive_mode.rs` (~5442, the
  render-timeout panic is inside the test module). They are not production throws.

## Process-level uncaught handler

`utils::uncaught` models Node's default uncaught handler. `throw_uncaught(label,
message)` panics with an `UncaughtError` payload; the `pi` binary installs
`install_uncaught_handler()` as the panic hook, which prints `<label>: <message>`
and exits 1 (not Rust's 101). Plain `panic!` payloads print as `Error: <message>`.
Tests: `crates/pi-coding-agent/tests/uncaught.rs`.

## Converted sites

| Rust site | TypeScript | TS throw / Node API | Catch boundary | Test |
|---|---|---|---|---|
| `config.rs::PackageConfig::load` read | `config.ts:479-486` | `readFileSync` ENOENT/EACCES/EISDIR; non-ENOENT rethrows | module-load uncaught → `try_package_config` / `package_config` | `config_paths::unreadable_package_json_surfaces_as_a_node_fs_error` |
| `config.rs::PackageConfig::parse` | `config.ts:481` | `JSON.parse` SyntaxError | module-load uncaught → `try_package_config` | `config_paths::invalid_package_json_surfaces_as_a_node_syntax_error` |
| `config.rs::package_config` | `config.ts:479-486` | uncaught module-load throw | process-level handler | `uncaught::uncaught_handler_prints_the_error_and_exits_one` |
| `theme/mod.rs::get_builtin_themes` | `theme.ts:472-482` | `readFileSync` + `JSON.parse` of dark/light | `initTheme`/`setTheme` first load catch; fallback rethrows; `getThemeByName` catches | `theme::missing_builtin_themes_return_errors`, `main_process::{missing_builtin_theme_differential,invalid_builtin_theme_differential}` |
| `theme/mod.rs::get_custom_theme_infos` | `theme.ts:523-536` | `readdirSync` (existsSync false → empty) | `getAvailableThemes*` has no catch → uncaught | `theme::missing_builtin_themes_return_errors` (theme dir absent) |
| `theme/mod.rs::get_available_themes_with_paths` | `theme.ts:494-521` | builtin/custom load throws (raw `SyntaxError` for builtins) | uncaught, `ThemeError::label()` keeps the `SyntaxError` name | `theme::missing_builtin_themes_return_errors` |
| `theme/mod.rs::get_available_themes` | `theme.ts:489-491` | same | uncaught | same |
| `theme/mod.rs::parse_theme_json_content` | `theme.ts:597-603` | `Failed to parse theme <label>: SyntaxError: …` | `getThemeByName` catch; `setTheme` catch; others uncaught | `differential_theme` (`invalid-json`, `truncated-json`, `trailing-json`, `empty-json`) |
| `theme/mod.rs::init_theme` | `theme.ts:876-890` | fallback `loadTheme("dark")` rethrows (Error or SyntaxError) | `runCliActionOrReportError` prints `Error: <message>`, exit 1 | `main_process::missing_builtin_theme_differential`, `main_process::invalid_builtin_theme_differential` |
| `theme/mod.rs::set_theme` | `theme.ts:892-913` | dark fallback rethrows | caller / uncaught | `theme::missing_builtin_themes_return_errors` |
| `theme/mod.rs::try_current_theme` / `current_theme` | `theme.ts:847-853` | `Theme not initialized. Call initTheme() first.` | uncaught | `theme::theme_missing_builtin_probe` (message) |
| `theme/theme_controller.rs::apply_theme_name`, `preview` | `theme-controller.ts:105-118` | `setTheme` fallback throw | uncaught | `theme_controller` suite |
| `cli/startup_ui.rs::apply_detected_startup_theme` | `cli/startup-ui.ts` (`setTheme`) | fallback throw | uncaught | `theme_controller` suite (shared path) |
| `bundled_extensions/goal/error_status_scheduling.rs::check_status` | `error-status-scheduling.ts:58` | rethrows non-stale `onStatus` error from `setTimeout` | uncaught | `uncaught` suite (mechanism) |
| `bundled_extensions/goal/empty_response_scheduling.rs` (2 sites) | `empty-response-scheduling.ts:49` | rethrows non-stale errors from `setTimeout` | uncaught | `uncaught` suite (mechanism) |
| `cli/experimental/command.rs::option` | `command.ts:80` | `Option <name> is already registered for <command>` | caller | `command::duplicate_option_registration_returns_an_error` |
| `cli/experimental/command.rs::command` | `command.ts:103` | `Command <name> is already registered` | caller | `command::duplicate_subcommand_registration_returns_an_error` |
| `cli/experimental/command.rs::parse`/`parse_own` | `command.ts:142` | `Command <name> does not define a builder` | caller | `command::parse_without_builder_returns_an_error` |
| `cli/experimental/command.rs::parse_own` | `command.ts:152` | `Command <name> failed without an error` | caller | `command::execute_without_action_returns_an_error` (builder-error path covered by differential `27010`) |
| `cli/experimental/command.rs::execute` | `command.ts:129` | `Command <name> does not define an action` | caller | `command::execute_without_action_returns_an_error` |
| `core/tools/tool_definition_wrapper.rs::cap_tool_result_output` | `tool-definition-wrapper.ts:61` | `writeFileSync` of the spill file | tool execute error (agent harness converts to a tool error) | `tool_definition_wrapper::spill_write_failure_returns_the_node_error` |
| `modes/interactive/external_editor.rs` (mkdtemp/write/read/chmod) | `external-editor.ts:14,17,37` | `mkdtempSync`, `writeFileSync`, `readFileSync` | rejected promise → unhandled rejection → uncaught | `interactive_workflows::external_editor_*` (success/failure); error path reaches the uncaught handler |

## Kept panics

These remain panics because the TypeScript throw is a genuine invariant for
typed input, or because the site has no TypeScript counterpart. The process-level
handler still prints `<Error>: <message>` and exits 1.

| Rust site | TypeScript | Reason |
|---|---|---|
| `theme/mod.rs::fg_ansi` / `bg_ansi` | `theme.ts:392,398,424,430` | `ThemeColor`/`ThemeBg` are enums and every theme reaches `Theme::new` through schema-validated JSON with all required tokens (typed-API exclusion, module header). Message text matches the TS throw. |
| `pi-tui/components/markdown/lexer.rs::block_tokens` / `inline_tokens` (3 sites) | vendored `marked.min.js` `Infinite loop on byte: ` | marked's own internal invariant; identical message. Byte vs UTF-16 code unit differs for a non-ASCII first byte (unreachable; documented discrepancy). |
| `pi-tui/components/markdown/lexer.rs::rx` | none | Rust-only regex translation of marked patterns; a compile failure is a port bug. |
| `pi-tui/components/markdown/lexer.rs` `unreachable!` (4 sites) | marked tokenizer switch exhaustiveness | Rust match exhaustiveness for variants the branch cannot produce. |
| `pi-tui/tui.rs:2864` | `tui.ts:2411`, `tui-main-screen.ts:466` | `Rendered line … exceeds terminal width`; TS throws the same message uncaught. Crash log is written first. |
| `pi-ai/providers/mod.rs:82`, `providers/openai_codex.rs:43` | none | Generated model catalogs are compiled in; a missing entry is a build error (TS imports a static module). |
| `pi-ai/api/openai_completions.rs:1549` | `openai-completions.ts` block index | Index comes from the just-created tool-call block. |
| `pi-coding-agent/bundled_extensions/agents_core/wait_runtime.rs:287` | n/a | Wake variants are filtered by the caller; Rust needs an exhaustive arm. |
| `pi-coding-agent/core/agent_session.rs:3667` | n/a | `SandboxProfileScope::Session` handled by the `if` above. |
| `pi-coding-agent/core/extensions/runner.rs:1365` | n/a | `emit` preserves the event variant. |
| `pi-coding-agent/modes/interactive/components/config_selector.rs:881` | n/a | Variant converted immediately above. |
| `pi-coding-agent/utils/management_http.rs:70` | n/a | Retry loop returns on every terminal attempt. |
| `pi-ai/providers/faux.rs:1008`, `utils/event_stream.rs:206` | n/a | Enum states excluded by the preceding check. |
| `pi-agent-core/harness/session/state.rs:489` | n/a | `ForkOptions::Tree` never selects a branch target. |
| `pi-tui/tui.rs:1419` | n/a | `OverlayFocusRestoreState::Inactive` excluded by the preceding check. |

## Discrepancies

- The pinned TypeScript CLI prints the theme-init error and sets
  `process.exitCode = 1` but does not exit (open handles keep the event loop
  alive; observed with a TTY and with pipes). The Rust port exits 1. The process
  differential compares the error line and the intended exit code.
- `marked` reports the first UTF-16 code unit in `Infinite loop on byte`; the
  Rust lexer reports the first UTF-8 byte. Unreachable for valid input.
- A failed partial-update spill is recorded and returned from the final tool
  execute; TypeScript aborts the tool immediately from inside the update
  callback. Same tool-call error, later point of observation.
