# json-event traceability

Pinned source: `packages/coding-agent/src/modes/json-event.ts` at `532f7d12cd19235081bd4d6e425bde185652d86f`.

Proof: `tests/json_event.rs` (4 behavioral cases) and `tests/differential_json_event.rs` (24 event
fixtures compared byte-for-byte against the real module through `oracle/coding-agent-json-event-oracle.ts`).

| TypeScript source | Observable behavior | Rust mapping and proof |
|---|---|---|
| `ToJsonEvent` / `WithoutPartial` types | Wire type removes `partial` from `message_update.assistantMessageEvent` | `to_json_event` returns `serde_json::Value`; the typed wrapper is unnecessary because the wire contract is JSON. |
| `toJsonEvent(event)` non-`message_update` branch | Returns the same event object | `to_json_event` returns the serialized event unchanged; behavioral `other_session_events_pass_through_unchanged`, differential fixtures 1-13. |
| `toJsonEvent` `message_update` without `partial` | Returns `{ type, assistantMessageEvent }`, dropping the top-level `message` | Rebuilt object keeps only `type` and `assistantMessageEvent`; behavioral `message_update_without_partial_is_returned_unchanged`, differential `done`/`error` fixtures. |
| `toJsonEvent` `message_update` with `partial` | Object-rest drops `partial`; the rebuilt object drops the top-level `message` | `assistantMessageEvent` cloned, `partial` removed (remaining key order preserved), rebuilt with `type` first; behavioral partial cases and differential streaming fixtures. |

## Exclusions and deviations

- The Rust function accepts any `Serialize` value, so call sites pass the typed
  `AgentSessionEvent` and the transform inspects the wire shape exactly like the TypeScript runtime checks.
- Serialization uses the crate's `serde_json` (`preserve_order`); the jsonl number-spelling discrepancy applies
  only to non-integral/negative-zero numbers, which these events do not carry.
- No source bug found.
