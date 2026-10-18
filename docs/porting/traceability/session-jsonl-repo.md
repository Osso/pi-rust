# Traceability: `packages/agent/src/harness/session/jsonl/repo.ts`

Rust: `crates/pi-agent-core/src/harness/session/jsonl/repo.rs`. Tests: `session_jsonl.rs`,
`differential_session.rs`.

## Exports and branches

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `SESSION_ID_PATTERN` / `validateSessionId` | `is_valid_session_id`, `validate_session_id` | jsonl metadata test | First/last ASCII alphanumeric; middle `[A-Za-z0-9._-]`; exact error message/code `invalid_payload`. |
| `jsonlSessionDirectoryName` | `jsonl_session_directory_name` | jsonl, differential | Strips one leading slash/backslash, replaces `/`, `\`, `:` with `-`, wraps in `--…--`. |
| `jsonlSessionsRoot` | `jsonl_sessions_root` | jsonl | `absolutePath`, error mapping. |
| `jsonlSessionDirectory` | `jsonl_session_directory` | jsonl | `joinPath([root, dir])`. |
| `jsonlSessionDirectories` | `jsonl_session_directories` | jsonl, differential | With cwd: single dir when it exists. Without: direct children that are directories or symlinks. |
| `listJsonlSessionMetadata` | `list_jsonl_session_metadata` | jsonl, differential | Reads one line per `.jsonl`, skips empty/malformed headers, sorts by `modifiedAt` descending (stable). |
| `loadJsonlSessionStorage` | `load_jsonl_session_storage` (+ clock variant) | jsonl, differential | Missing file → `not_found`; header id mismatch → `invalid_entry`. |
| `sessionFileName` | `session_file_name` | jsonl, differential | `Date#toISOString()` with `:`/`.` → `-`, then `_{id}.jsonl`. |
| `JsonlSessionRepo` | `JsonlSessionRepo` | jsonl, differential | Injectable clock/id generator. |
| `create` | `create` | jsonl, differential | Destination claim, duplicate id check, directory creation, header write. |
| `open` | `open` | jsonl, differential | Loads and verifies id. |
| `list` | `list` | jsonl, differential | |
| `delete` | `delete` | jsonl | `remove(force: true)`, idempotent. |
| `fork` | `fork` | jsonl, differential | Loads source, defaults `parentSessionId`, claims destination, stages and renames. |
| `resolveCreateDestination` | `resolve_create_destination` | jsonl | id default from generator; cwd absolute. |
| `claimCreateDestination` | `claim`/`release` | jsonl | In-process `{cwd}\0{id}` reservation released on failure. |
| `prepareCreate` | `prepare_create` | jsonl | Duplicate suffix check, clock timestamp, header, metadata, recursive directory. |
| `sessionIdExists` | `session_id_exists` | jsonl | Files ending `_{id}.jsonl`, excluding directories. |
| `root()` promise cache | – | – | Root is recomputed per call; `absolutePath` is deterministic for a filesystem instance. |

## Branches and gaps

- `modifiedAt` is fractional milliseconds in both implementations; differential metadata comparisons normalize it
  to a boolean because the two temp roots have independent mtimes.
- TS caches the resolved sessions root in a promise; Rust recomputes it. No observable behavior changes for the
  supported filesystem contract.
- Concurrent create/fork protection covers same-process races only, like TS.
