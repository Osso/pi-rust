# Traceability: `packages/agent/src/harness/session/jsonl/types.ts`

Rust: `crates/pi-agent-core/src/harness/session/jsonl/types.rs`. Tests: `session_jsonl.rs`, `differential_session.rs`.

## Exports

| TypeScript | Rust | Tests | Notes |
|---|---|---|---|
| `JsonlSessionRepoFileSystem` | `JsonlSessionRepoFileSystem` trait | jsonl | Async methods returning `Result<T, FileError>`; replaces the TS `Result` union. |
| `JsonlSessionRepoOptions` | `JsonlSessionRepoOptions` | jsonl, differential | `Arc<dyn …FileSystem>` + sessions root. |
| `JsonlSessionMetadata` | `JsonlSessionMetadata` | jsonl, differential | `modifiedAt` uses JS number serialization. |
| `JsonlSessionCreateOptions` | `JsonlSessionCreateOptions` | jsonl, differential | `metadata: Option<JsonObject>`. |
| `JsonlSessionListOptions` | `JsonlSessionListOptions` | jsonl, differential | |
| `JsonlV4Header` | `JsonlV4Header` + `HeaderKind` | codec, differential | Header discriminant is a closed enum. |
| `FileSystem` subset | `NativeFileSystem` | jsonl | Rust-native adapter used by tests and Rust-only callers. |

## Branches

- `NativeFileSystem` resolves relative paths against `cwd`, expands `~`, lexically normalizes, and maps IO errors
  onto `FileErrorCode`. `read_text_lines` mirrors Node readline: CRLF stripped, trailing newline not returned,
  `maxLines == 0` returns empty.
- `write_file`/`append_file` create parent directories, matching `NodeExecutionEnv`.
- `list_dir` returns `FileInfo` values without following symlinks.
- `mtime_ms` is `SystemTime` since epoch as fractional milliseconds; list ordering uses it.

## Gaps

- The broader harness `FileSystem` capability (binary reads, temp dirs, canonical paths, abort signals) remains a
  later `env/nodejs.ts` slice. This module ports only the `JsonlSessionRepoFileSystem` pick used by sessions.
- Abort signals are not part of the JSONL backend's filesystem calls in TS either; no cancellation parameter is
  exposed here.
