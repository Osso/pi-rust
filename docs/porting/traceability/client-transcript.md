# coding-agent client transcript traceability

Pinned source: `packages/coding-agent/src/client/transcript.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.
Rust: `crates/pi-coding-agent/src/client/transcript.rs`.
Focused tests: `crates/pi-coding-agent/tests/client_transcript.rs` (9, ported from `test/client/transcript.test.ts`).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `TranscriptState` | snapshot + progress items + insertion order + tool-call buffers | `TranscriptState` (`IndexMap` maps, `Vec` order) |
| `createTranscriptState` | deep-cloned snapshot, empty progress | `create_transcript_state`; all tests |
| `applyTranscriptSnapshot` | lower revision for the same id is ignored | `apply_transcript_snapshot`; `a_newer_snapshot_is_authoritative_and_stale_snapshots_are_ignored`, `accepts_a_lower_revision_when_switching_to_a_different_session` |
| `applyTranscriptProgress` item_started/item_updated | replace or append transient item | `set_progress_item`; `appends_transient_tool_progress_and_replaces_it_by_id` |
| `applyTranscriptProgress` item_finished | drop `${id}:` tool-call buffers, then store item | `apply_transcript_progress`; `finished_items_clear_their_tool_call_buffers` |
| `applyTranscriptProgress` assistant_delta text/thinking | append delta to the addressed part | `apply_delta_part`; `projects_progress_without_mutating_the_authoritative_snapshot` |
| `applyTranscriptProgress` assistant_delta toolCall | buffer raw input, parse when valid JSON | `parse_partial_tool_input`; `applies_streamed_tool_call_argument_deltas`, `appends_tool_call_deltas_to_a_partial_input_restored_from_a_snapshot` |
| `selectTranscript` | snapshot order, transient order, queued steering | `select_transcript`; `renders_accepted_steering_messages_from_authoritative_queued_state` |
| `isJsonValue` / `structuredClone` | parsed JSON accepted, other prefixes kept as strings; items copied by value | Rust ownership clones values; `parse_partial_tool_input` |

## Gaps

- Differential oracle skipped: the protocol transcript types are CBOR-typed and have no JSON serializer, so a
  differential would require a second projection layer. Behavior is proven against the ported TypeScript cases.
- `isJsonValue` rejects non-plain-prototype objects and non-finite numbers; `serde_json` cannot produce them
  from parsed text, so the accepted set is identical for JSON input.

## Discrepancies

None.
