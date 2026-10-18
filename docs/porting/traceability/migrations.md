# coding-agent migrations.ts traceability

Source: `packages/coding-agent/src/migrations.ts` (365 lines, pinned revision in
`docs/porting/source-lock.json`). Rust: `crates/pi-coding-agent/src/migrations.rs`.

The TypeScript module reads global agent/bin directories. The Rust port exposes
`*_in`/`*_with` variants taking the agent directory, bin directory, and an output
sink so tests and the differential oracle share fixture trees without mutating
process environment. The no-argument wrappers (`run_migrations`,
`migrate_auth_to_auth_json`, `migrate_sessions_from_agent_root`,
`migrate_tools_to_bin`, `show_deprecation_warnings`) keep the TypeScript shape.
`migrateKeybindingsConfig` is reused from `core::keybindings`.

## Exported symbols

| TS symbol | Rust item | Tests |
| --- | --- | --- |
| `migrateAuthToAuthJson` | `migrate_auth_to_auth_json` / `migrate_auth_to_auth_json_in` | 6 auth tests, differential auth/existing-auth/invalid-auth |
| `migrateSessionsFromAgentRoot` | `migrate_sessions_from_agent_root` / `_in` | 2 session tests, differential sessions |
| `showDeprecationWarnings` | `show_deprecation_warnings` | non-TTY path only (gap below) |
| `runMigrations` | `run_migrations` / `run_migrations_in` / `run_migrations_with` | all run-migration tests, differential |

## Branches

| Branch | Test |
| --- | --- |
| missing `auth.json` starts from `{}`; invalid `auth.json` aborts everything | `no_providers_...`, `invalid_auth_json_...`, differential |
| `oauth.json` entries migrated, existing providers win, non-record credentials skipped, file renamed `.migrated` | oauth test, existing-auth test, differential |
| `settings.json` `apiKeys` migrated and field removed, non-string keys skipped | settings test, differential |
| auth file written only when providers migrated; mode `0600` | oauth/settings tests |
| session header `type: "session"` + `cwd`, encoded directory `--cwd--`, existing target skipped, malformed/blank/non-session skipped, nested files untouched | session tests, differential |
| `commands/` → `prompts/` for agent and project, rename failure warning | commands test, differential |
| `keybindings.json` migrated only when changed, malformed/non-object untouched | keybindings tests, differential keybindings |
| `tools/` binaries moved to `bin/`, duplicate target deletes the old file | tools test, differential |
| deprecated `hooks/` warning; `tools/` warning only for non-fd/rg non-hidden entries | deprecation test, differential commands case |

## Gaps

- TTY keypress wait in `showDeprecationWarnings`: the non-TTY path (return
  immediately) is ported; the raw-mode read needs the interactive terminal, which
  the startup binary does not wire into this module. The warning text itself is
  emitted identically.
- Windows `rmSync`/rename semantics are not exercised; the host is Linux.

## Discrepancies

- None observed. Writes use `utils::js_json` so `auth.json`, `settings.json`, and
  `keybindings.json` match `JSON.stringify(..., null, 2)` byte-for-byte, including
  key order and JavaScript number formatting.

## Proof

- `cargo test -p pi-coding-agent --test migrations` (13 tests)
- `cargo test -p pi-coding-agent --test differential_migrations` (8 fixtures; file
  trees, console lines, provider lists, and warnings compared byte-for-byte)
- `cargo clippy -p pi-coding-agent --all-targets -- -D warnings`

The differential snapshot sorts directory entries because directory order is
filesystem-dependent (per-directory hash seeds). Every deterministic array
(providers, warnings, console lines) is compared in source order.
