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
`config::InstallContext`, `pyrun`, `llama::huggingface`, `clipboard_image`,
`startup_ui`, the interactive theme loaders, and `core::self_restart` build their
process-environment snapshots from `vars()`. `pi_agent_core`'s harness Node env
applies the same lossy value / dropped-key rule when it inherits the environment.

Follow-up: unrelated single-variable `std::env::var` reads (config value references,
provider env, footer data, package manager, desktop notifications, `pi-tui` terminal
detection) still read the process environment directly; they tolerate non-UTF-8 by
treating it as unset rather than Node's lossy value. Migrate only if a caller needs
exact parity.
