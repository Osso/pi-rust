# coding-agent session-tool-output traceability

Frozen source: `packages/coding-agent/src/core/session-tool-output.ts` at `4f9e6f8a4`; earlier whole-module proof is not renewed by this bounded delta.

Rust module: `crates/pi-coding-agent-storage/src/core/session_tool_output.rs`.
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

## Storage migration ordering and timestamp fixes (2026-10-25)

Actual frozen module RED/GREEN: `repin_storage_tool_output` compares the complete migration report for four malformed filenames created out of order and for a source-created oversized session changed from mtime -2 seconds to -1 second inside the scanner callback. Native previously emitted filesystem iteration order and silently projected both negative timestamps to0, then overwrote the changed file; frozen reports one error and leaves its bytes unchanged. Native now sorts entries by their OS filename before the source stack traversal and computes POSIX Node mtimeMs from seconds plus nanoseconds. No timestamp fallback remains; the non-POSIX metadata query propagates failure. Logs: `/tmp/pi-rust-repin-storage/tool-migration-order-time-{red,green}.log`. Whole-module row remains open.

## Raw persisted input errors and approved gap (2026-10-25)

Truncation and persistence serialization now return `Result<_, Box<NativeNodeError>>`; manager writes/forks/imports propagate, while migration catches the frozen error message into its report. Actual-source RED/GREEN reproduces a null `toolCallId` at the minimal candidate: native previously omitted it silently, but frozen `utf8Prefix` throws Buffer.from's `TypeError`/`ERR_INVALID_ARG_TYPE`. The expanded primitive matrix covers missing/null/bool/number for both identity fields, including1e12 error formatting. Null/missing raw `message` records also throw the exact frozen property-access TypeError before checking content. Raw toolResult objects are inspected directly; the old typed-union-only gap is retired.

| Boundary | Classification | Proof |
|---|---|---|
| Non-string Buffer-like array/array-like/object `toolCallId`/`toolName` at minimal fallback | **GAP approved by parent2026-10-25**: no single String conversion; reject explicitly rather than omit/coerce. This does not reject small/untruncated raw records. | `buffer_like_tool_identity_is_explicit_approved_gap_not_silent_omission`: actual frozen returns array[65,66] intact, native returns explicit error |

## Remaining platform boundary
- Atomic rewrite compares `mtimeMs` as `f64` using the same POSIX seconds/nanoseconds arithmetic as Node. Windows-only behavior remains under the approved platform gap.

## Discrepancies

The formerly recorded claim that traversal order is unobservable was incorrect: `errors` and `backupPaths` retain traversal order. The four-file differential above reproduces and corrects the error-report ordering; no source discrepancy is introduced.

## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).

Bounded merged evidence and unresolved consumer/classification limits: [session storage integration](session-storage-integration.md). No fresh gate or whole-module promotion.
