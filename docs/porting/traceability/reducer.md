# Harness reducer traceability

Pinned source: `packages/agent/src/harness/reducer.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-agent-core/src/harness/reducer.rs`. Behavioral port of `packages/agent/test/harness/reducer.test.ts`.

Input/output records reuse the ported session types (`LaneRecord`, `Entry`, `EntryBody`, `OperationIntent`) and are
deserialized/serialized with the source wire names, so the differential compares the real JSON shape.

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `reducer.ts:16-27` | `RecordLogCorruptionReason` has twelve literals | `RecordLogCorruptionReason` with exact `as_str()`; focused + differential corruption fixtures |
| `reducer.ts:29-38` | `RecordLogCorruption` carries `reason` and message | `RecordLogCorruption { reason, message }`, `Display`/`Error`; differential compares `{name,message,reason}` |
| `reducer.ts:40-47` | `RecordLogSlice` lane/openOperations/records/entries | `RecordLogSlice` (camelCase serde) |
| `reducer.ts:49-53` | `EffectiveLaneConfiguration` model/thinkingLevel/activeToolNames | `EffectiveLaneConfiguration`/`LaneModel` |
| `reducer.ts:55-59` | `TerminalFailureState` entryId/source/message | `TerminalFailureState` + `TerminalFailureSource` |
| `reducer.ts:61-73` | `ToolBatchState`/calls with optional started/terminate | `ToolBatchState`/`ToolBatchCall` (`started` stores the full record) |
| `reducer.ts:75-106` | `LaneState`/operation fields and nullable members | `LaneState`/`LaneOperationState`; nullable fields serialize as JSON `null` |
| `reducer.ts:108-117` | `LaneReductionInput` adds leafId/ownEntries/configurationEntries/defaults | `LaneReductionInput` |
| `reducer.ts:119-123` | `LaneReductionResult` | `LaneReductionResult` |
| `reducer.ts:125-129` | `corrupt()` throws the tagged error | `corrupt()` returns `Err(RecordLogCorruption)` |
| `reducer.ts:131-133` | `hasRunId` requires an own string `runId` (operation starts excluded) | `record_run_id` excludes `OperationStarted` and returns optional queue/usage ids |
| `reducer.ts:135-138` | Provisioned entries compare after dropping parentId/seq/timestamp | `matches_provisioned_entry` serializes, removes those keys, deep-compares |
| `reducer.ts:140-166` | Exact/result entry validation and messages | `validate_exact_provisioned_entry`, `validate_result_entry` |
| `reducer.ts:168-177` | Compaction attempts require a valid reason; others must not carry one | `validate_attempt_reason` |
| `reducer.ts:179-207` | Attempt continuity, expected attempt, and structural agreement | `validate_attempt_sequence` (series restarts when the previous result exists and precedes the record) |
| `reducer.ts:209-237` | Attempt result type per step kind | `validate_attempt_result` |
| `reducer.ts:239-275` | Tool-start identity, assistant reference, ordinal match, result match | `validate_tool_start` with `\0` invocation identity |
| `reducer.ts:277-290` | Deferred assistant entries must carry a handle | `validate_deferred_handles` |
| `reducer.ts:292-315` | Operation result prefixes: run initial messages, compaction result, navigation summary | `validate_operation_result` |
| `reducer.ts:318-390` | `validateRecordLog`: single open operation, sorted records, unknown operation, record-after-finish, queue-after-abort, queue cancellation, provisioned entries | `validate_record_log`; every branch is exercised by `corruption_cases()` and the durable-prefix traces |
| `reducer.ts:392-401` | `clone`/`bySequence` (stable sort by seq) | Owned clones in outputs; `by_sequence` uses stable `sort_by_key` |
| `reducer.ts:403-431` | Effective configuration folds defaults, then entries by seq; assistant messages set model | `derive_effective_configuration` (insertion order preserved for equal seqs) |
| `reducer.ts:433-445` | `deriveNewestOwn` includes role for non-assistant messages and stopReason for assistant | `derive_newest_own` |
| `reducer.ts:447-505` | Tool batch: newest assistant with tool calls, starts per ordinal, blocked results after the assistant, deferred-write exclusion, terminate, truncated, unresolved | `derive_tool_batch` |
| `reducer.ts:507-560` | Idle vs open operation, pending next-run capture, operation record filter, aborting, steer/follow-up/write queues, missing initial messages, unfinished step | `reduce_lane_state` |
| `reducer.ts:562-590` | Consumed-input sequence, overflow-recovery reset, deferred handle at the tail, structural targets | `reduce_lane_state` + `derive_targets` + `deferred_handle` |
| `reducer.ts:592-629` | Terminal failure requires error stop, non-deferred-write, and step or deferred-fetch provenance | `derive_terminal_failure` |
| `reducer.ts:631-667` | Final lane state assembly | `LaneReductionResult` |

## Approved Rust boundary

- Records and entries are the existing typed session records; invalid JavaScript shapes are rejected by serde instead of being coerced.
- `toolIndex` is an array ordinal (`usize`); non-integral/negative indices cannot reference a call, matching the source `toolCalls[index] === undefined` behavior.
- `thinkingLevel` on persisted `thinking_level_change` entries is carried as the source string (`as ThinkingLevel` is a TypeScript cast with no validation).
- Inputs are borrowed; outputs are owned clones, so the source's structuredClone non-aliasing guarantee holds by construction.

## Proof

- Focused: `tests/harness_reducer.rs` (20 tests): 15 corruption reason/message cases, all durable prefixes of 21 source traces, and every reduction branch.
- Differential: `oracle/harness-core-oracle.ts` operations `validate_record_log`/`reduce_lane_state`.
  - 15 corruption slices (`14500..=14514`) compare error `{name,message,reason}` byte-for-byte.
  - A 35-step scripted sequence (`14600..=14669`) validates and reduces the full state after every durable action and compares the complete JSON result.
- Gaps: none.
