# coding-agent session-tool-output traceability

Frozen source: `packages/coding-agent/src/core/session-tool-output.ts` at `4f9e6f8a4`; whole-module renewal2026-10-25 is bounded by the explicit approved gap and platform exclusions below.

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

Actual frozen module RED/GREEN: `repin_storage_tool_output` compares the complete migration report for four malformed filenames created out of order and for a source-created oversized session changed from mtime -2 seconds to -1 second inside the scanner callback. Native previously emitted filesystem iteration order and silently projected both negative timestamps to0, then overwrote the changed file; frozen reports one error and leaves its bytes unchanged. Native now sorts entries by their OS filename before the source stack traversal and computes POSIX Node mtimeMs from seconds plus nanoseconds. No timestamp fallback remains; the non-POSIX metadata query propagates failure. Logs: `/tmp/pi-rust-repin-storage/tool-migration-order-time-{red,green}.log`. The complete module matrix is recorded below.

## Raw persisted input errors and approved gap (2026-10-25)

Truncation and persistence serialization now return `Result<_, Box<NativeNodeError>>`; manager writes/forks/imports propagate, while migration catches the frozen error message into its report. Actual-source RED/GREEN reproduces a null `toolCallId` at the minimal candidate: native previously omitted it silently, but frozen `utf8Prefix` throws Buffer.from's `TypeError`/`ERR_INVALID_ARG_TYPE`. The expanded primitive matrix covers missing/null/bool/number for both identity fields, including1e12 error formatting. Null/missing raw `message` records also throw the exact frozen property-access TypeError before checking content. `null_record_serializer_and_migration_throw_before_writing` additionally reproduces direct serialization of null and a source-readable session containing a null record: frozen throws reading `type`, and migration reports one caught error without writing/backup. Native formerly serialized/ignored null; the shared entry guard now retains the frozen failure (`tool-null-record-{valid-red,green}.log`). Raw toolResult objects are inspected directly; the old typed-union-only gap is retired.

| Boundary | Classification | Proof |
|---|---|---|
| Non-string Buffer-like array/array-like/object `toolCallId`/`toolName` at minimal fallback | **GAP approved by parent2026-10-25**: no single String conversion; reject explicitly rather than omit/coerce. This does not reject small/untruncated raw records. | `buffer_like_tool_identity_is_explicit_approved_gap_not_silent_omission`: actual frozen returns array[65,66] intact, native returns explicit error |

## Whole-module frozen matrix (2026-10-25)

| Export/branch | Proof |
|---|---|
| Constants and no-op/changed truncation, non-tool entries | current `session_tool_output`7; actual-source candidate matrix includes unchanged identity detection |
| Text-byte cap, serialized-byte cap, nontext retention/replacement, details removal, minimal128-byte UTF-8 identities | `truncation_candidates_match_frozen_wire_and_changed_detection` compares exact source wire and changed flag for7 concrete small/oversized fixtures, including escaped JSON inflation and multiblock/nontext cases |
| Null/missing message and primitive tool identity errors; raw null entry | actual-source error matrices above; serializer/migration guard preserves caught error before backup |
| Header/entry/unknown/raw UTF-16 serialization | `differential_session_manager` exact byte serializer and reciprocal file append9; `lossless_session_persistence`6 retains source order and unknown unpaired keys/values; migration fixtures use actual frozen source-created records |
| Recursive directory traversal, malformed/non-session/read errors, blank lines, empty id | existing `repin_session_storage` migration cases plus four-name ordering fixture |
| Exclusive temp rewrite/backup/mode/idempotence, same-size timestamp races | current tool tests and exact source migration/backup fixtures; signed mtime RED/GREEN above |
| User clock callback throws Error/non-Error; invalid Date | compile-RED against missing fallible API followed by actual-source GREEN on two real oversized files per case; `throwing_clock_and_invalid_date_are_caught_for_each_migration_file` compares full reports and unchanged originals |

Clock callbacks now return `Result<f64, CallbackError>`; per-file catches apply frozen Error.message/otherwise String(value), not message-only coercion. Callback rejection is never silently replaced; invalid Date yields RangeError/`Invalid time value`. The migration API is fallible when the catch's own String conversion throws, matching frozen; CLI callers propagate at their existing error boundary. Logs: `/tmp/pi-rust-repin-storage/tool-{clock,candidate,null-record}*.log`. Final current-revision gates are recorded in the storage proof ledger before checkpoint.

No old-only implementation remains. The explicit Buffer-like fallback gap above and approved Windows-only exclusion remain; this does not promote session-manager, spool or history.

## Remaining platform boundary
- Atomic rewrite compares `mtimeMs` as `f64` using the same POSIX seconds/nanoseconds arithmetic as Node. Windows-only behavior remains under the approved platform gap.

## Shared rm error authority2026-10-25

Frozen `session-tool-output.ts:326-327` calls `rmSync(tempPath,{force:true})` in finally. The migration's existing `node_rm_error` string adapter now delegates to the shared native rm error record, not libuv unlink formatting. Positive errno/syscall`rm` is retained at typed callers; this migration catches only error.message as frozen does. Missing temp files remain ignored, and nonrecursive EISDIR/permission messages remain unchanged. [Actual cleanup RED/GREEN and Node source provenance](session-transcript-spool.md#cleanup-failure-precedence-2026-10-25).

## Discrepancies

The formerly recorded claim that traversal order is unobservable was incorrect: `errors` and `backupPaths` retain traversal order. The four-file differential above reproduces and corrects the error-report ordering; no source discrepancy is introduced.

## JS-value boundary regression (2026-10-20)

Shared JS stringify/object enumeration is used at this module's audited observable boundary; values and parser errors are unchanged. Concrete counterexamples and per-boundary tests: [JS-value proof](../js-value-semantics-proof.md).

Bounded merged evidence and unresolved consumer/classification limits: [session storage integration](session-storage-integration.md). No fresh gate or whole-module promotion.
