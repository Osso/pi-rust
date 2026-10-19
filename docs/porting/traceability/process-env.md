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

Wiring: `utils::shell::get_shell_env`, `core::exec`, detached payload env, `find`,
`grep`, `ls`, `code_index`, and `utils::child_process` build child environments from
`vars()`.

Follow-up: unrelated `std::env::var` reads (config value references, provider env,
footer data, package manager, self-restart, desktop notifications) still read the
process environment directly; migrate only if a caller needs overlay visibility.
