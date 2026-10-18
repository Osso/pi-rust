# Coding-agent runtime message markers traceability

Pinned source: `packages/coding-agent/src/core/runtime-message-markers.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Rust: `crates/pi-coding-agent/src/core/runtime_message_markers.rs`.

| TypeScript source | Observable behavior | Rust proof |
|---|---|---|
| `runtime-message-markers.ts:3` | Two marker literals | `RuntimeMessageMarker::as_str`, `runtime_message_marker_literals_match_the_source` |
| `runtime-message-markers.ts:19-27` | Assistant predicate: stored marker or event marker equals `duplicate_turn_assistant` | Differential requests `28400..=28402`, `stored_markers_are_honored...` |
| `runtime-message-markers.ts:29-35` | Guard predicate: stored marker or event marker equals `duplicate_turn_guard` | Same |

## Deviation (approved by parent slice assignment, recorded here)

The source stores markers in a `WeakMap` keyed by message object identity. Rust `AgentMessage` is an owned value with no stable identity, so the port takes the stored marker as an explicit parameter. The owner (`AgentSession`) supplies it when emitting events; the source itself already forwards it as `runtimeMessageMarker`. `markDuplicateTurnAssistantMessage`, `markDuplicateTurnGuardMessage`, and `getRuntimeMessageMarker` therefore have no Rust counterparts; marker attachment belongs to the agent-session slice's message records.

## Discrepancies

None beyond the identity-map deviation above.
