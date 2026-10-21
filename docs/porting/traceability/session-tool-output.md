# coding-agent session-tool-output traceability

Frozen source: `packages/coding-agent/src/core/session-tool-output.ts` at `4f9e6f8a4`; earlier whole-module proof is not renewed by this bounded delta.

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

## Frozen lossless migration boundary

`read_session_file_for_migration` now calls the existing crate-visible `parse_session_file_entry` without changing its signature/visibility. Actual source-valid escaped receipt and unpaired summary extras previously counted malformed (RED:2 malformed/0 changes vs TS1 malformed/1 change). Focused migration differential covers semantic rewritten entries via frozen Node deep equality, raw receipt/extra tokens, immutable backup bytes, idempotence, malformed JSON and non-session counters; GREEN4403306e. Initial post-fix comparison returned lone-surrogate fields through the Value-only oracle transport and failed there; equality now runs inside Node without projecting those fields through serde. No generic public-serde rewrite.

Entry sizing directly serializes the lossless KnownSessionEntry; impossible serialization errors fail explicitly, never count as zero. Existing cap/search algorithm unchanged. The same oversized escaped-text fixture compares actual frozen truncation output, not only sizes.

## Gaps

- Typed boundary: only `AgentMessage::ToolResult` values are truncated. A raw custom object with `role:"toolResult"` and an array `content` is preserved untruncated, because the Rust message union parses that shape as a custom message.
- Atomic rewrite compares `mtimeMs` as `f64`; the source compares Node's `stat.mtimeMs`. Both derive from the same filesystem timestamp.

## Discrepancies

None observed. File traversal order differs but is not observable in the report.

## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).

Bounded merged evidence and unresolved consumer/classification limits: [session storage integration](session-storage-integration.md). No fresh gate or whole-module promotion.
