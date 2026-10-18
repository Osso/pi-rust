# Traceability: `packages/agent/src/harness/session/jsonl/errors.ts`

Rust: `crates/pi-agent-core/src/harness/session/jsonl/errors.rs`. Tests: `session_jsonl.rs`,
`differential_session.rs`.

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `JsonlDecodeError` | `JsonlDecodeError` + `JsonlDecodeKind` | codec, differential | `syntax`/`schema`, exact messages. |
| `fileResult(result, message)` | `file_result()` | jsonl append/fork/list failures | Maps `not_found` → `not_found`, everything else → `storage`; message `"{message}: {error.message}"`; retains file cause code/message/path. |
| `invalidFile(path, line, cause)` | `invalid_file()` | jsonl torn/invalid replay | `Invalid JSONL v4 session {path}: line {line} {cause.message}`, code `invalid_entry`. |
| replay `SessionError` cause | `invalid_mutation_file()` | jsonl replay | Same message shape with the inner `invalid_entry` error as a session cause. |

## Branches and gaps

- TS wraps the original `Error` as `cause`; Rust stores a structured `SessionErrorCause` (`File`, `Decode`,
  `Session`) so tests can assert `code`/`message` without dynamic error chains.
- Error stack text and JavaScript prototype identity are outside the typed boundary.
