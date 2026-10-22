# process-env traceability

Rust: `crates/pi-js-compat/src/process_env.rs`.
Tests: `crates/pi-js-compat/tests/process_env.rs`.

Models Node `process.env` for extensions that mutate it. Rust edition 2024 makes
`std::env::set_var`/`remove_var` unsafe and the workspace forbids unsafe code
(approved deviation 2026-10-18).

| Source behavior | Rust item | Proof |
| --- | --- | --- |
| `process.env[name]` (override, tombstone, then process) | `get` | unit test |
| `process.env[name] = value` | `set` | unit test |
| `delete process.env[name]` | `remove` | unit test |
| ordered snapshot (process order + new keys in insertion order) | `vars` | unit test |
| test isolation | `clear` | unit test |
| non-UTF-8 value: decode lossily, one U+FFFD per invalid byte | `get`/`vars` via `to_string_lossy` | `decodes_non_unicode_environment_entries_like_node` spawns a child with `OsString::from_vec` values (`a\xe0\xffz`, `x\xf0\x9fy`, `x\xc0\xafy`) and compares against Node 26 output |
| non-UTF-8 key: drop the entry (Node omits it) | `vars` filters `OsString::into_string()` | same child test (invalid key absent from the snapshot) |

`std::env::vars()` panics on a non-UTF-8 entry (`Vars` calls `into_string().unwrap()`),
so every production snapshot uses `vars_os` plus lossy conversion. `Command::spawn`'s
inherited-environment capture uses the same `vars_os`/lossy mapping internally.

Wiring: `utils::shell::get_shell_env`, `core::exec`, detached payload env, `find`,
`grep`, `ls`, `code_index`, `utils::child_process`, `utils::chalk`, `main`,
`config::InstallContext`, `pyrun`, `clipboard_image`,
`startup_ui`, the interactive theme loaders, and `core::self_restart` build their
process-environment snapshots from `vars()`. `pi_agent_core`'s harness Node env
applies the same lossy value / dropped-key rule when it inherits the environment.

Single-variable reads: every production `std::env::var`/`var_os` read that ports
a TypeScript `process.env` read now goes through `get`, so overlay mutations made
at runtime (`main.ts` sets `PI_OFFLINE`/`PI_SKIP_VERSION_CHECK`, extensions call
`set`/`remove`) are visible exactly like Node. Remaining `std::env` reads are only
the implementation inside `process_env.rs` and Rust-only test hooks.

Migrated readers: `pi-ai` (`env_api_keys`, `provider_env`, `node_http_proxy`,
`auth::context`, `api::google_client`, `api::bedrock_credentials`), `pi-tui`
(`tui`, `tui_alt_screen`, `terminal`, `terminal_image`, `keys`, `autocomplete`),
`pi-agent-core` (`harness::env::nodejs` spawn environment, `harness::session::jsonl`
home expansion), `pi-coding-agent` (config and `PI_PACKAGE_DIR`, config value
references, settings/theme/telemetry/timings flags, trust manager, package
manager, paths/footer/session selectors, clipboard and terminal detection, the
bwrap/pyrun/claude-hook/goal extensions, detached runners, supervisor, and
architect), and `pi-js-compat::collator` locale resolution.

Behavioral proof: `tests/process_env_reads.rs` in `pi-ai`, `pi-tui`,
`pi-agent-core`, and `pi-coding-agent` sets an overlay value and asserts the
ported reader observes it (`ZAI_API_KEY` → `get_env_api_key`, `TMUX`/`TERM` →
`AltScreenEnvironment::detect`, `HOME` → `NativeFileSystem::absolute_path`,
`PI_SHARE_VIEWER_URL` → `config::get_share_viewer_url`). Existing differential
suites for those modules still pass.
