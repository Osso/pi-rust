# coding-agent session-tool-output traceability

Pinned source: `packages/coding-agent/src/core/session-tool-output.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust module: `crates/pi-coding-agent/src/core/session_tool_output.rs`.
Focused tests: `crates/pi-coding-agent/tests/session_tool_output.rs` (6).
Differential: `crates/pi-coding-agent/tests/differential_session_manager.rs` (`truncates_tool_results_identically`, request `41040`) and byte-identical entry serialization (request `41000`).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` | `1_048_576` | same constant |
| `TOOL_RESULT_TRUNCATION_MARKER` | `"\n[tool output truncated at 1 MiB]"` | same constant |
| `ToolResultSessionMigrationReport` | counters, backup paths, errors | `ToolResultSessionMigrationReport` |
| `truncateToolResultForPersistence` | unchanged when under the limit; text blocks capped to marker; details dropped; tool id/name capped at 128 bytes as a last resort | `truncate_tool_result_for_persistence`; `caps_single_line_tool_result_on_disk_without_changing_the_runtime_entry`, `truncates_multibyte_utf8_at_a_valid_boundary`, `returns_unchanged_small_tool_results`, differential `41040` |
| `serializeSessionEntryForPersistence` | serializes headers/entries with persistence-only truncation | `serialize_session_entry_for_persistence` / `serialize_session_entry`; differential `41000` |
| `migrateToolResultSessionFiles` | recursive `.jsonl` scan, malformed/non-session/error counters, backup + atomic rewrite, mode preservation, idempotence, changed-file detection | `migrate_tool_result_session_files`; `migrates_oversized_sessions_atomically_and_idempotently`, `skips_a_session_that_changes_during_migration` |
| `utf8Prefix` | UTF-8 byte prefix without replacement characters | `utf8_prefix`; multibyte tests |
| `capTextContent`/`capSerializedContent`/`capToolResultContent`/`capToolResultEntry` | text-byte cap, serialized-content cap, entry-byte cap, minimal fallback | same-named private helpers; exercised by the focused and differential cases |

## Gaps

- Typed boundary: only `AgentMessage::ToolResult` values are truncated. A raw custom object with `role:"toolResult"` and an array `content` is preserved untruncated, because the Rust message union parses that shape as a custom message.
- Atomic rewrite compares `mtimeMs` as `f64`; the source compares Node's `stat.mtimeMs`. Both derive from the same filesystem timestamp.

## Discrepancies

None observed. File traversal order differs but is not observable in the report.
