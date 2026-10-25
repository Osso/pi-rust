# Message timestamps — binary64 integration

Frozen `ai/src/types.ts:414–452` declares User/Assistant/ToolResult timestamp as number. Frozen `agent-core/src/harness/messages.ts:86–135` uses Date.getTime(), including NaN from invalid date strings.

Three message fields, AssistantMessage::empty and Faux timestamp options retain f64. Serialization delegates to the existing JsValue number JSON writer: exact Node spelling (including1e20), negative zero→0, nonfinite→null. No null→NaN decoding or message-number narrowing. AgentMessage timestamp access and harness conversions preserve fractions, nonfinite values and signed zero. The temporary checked_provider_timestamp rejection was deleted after the frozen tree-session replacement removed virtual_retained_entries.

## Consumer boundaries

| Consumer | Representation/proof |
|---|---|
| User/Assistant/ToolResult JSON | Two actual-source oracle tests: three roles×13 values and fractional decoding; `timestamp-types-final.log` |
| Agent-core custom/summary/bash conversion | Native f64; `messages_convert` including bit-exact fractional/nonfinite/signed-zero test |
| Message producers/transformers, compaction, bundled and example providers | Binary64 clocks; compiler-guided workspace consumers, integer fixture widening only |
| External-session importer | Messages stay f64; TimeClip only for `new Date(message.timestamp).toISOString()`, invalid values error. Actual frozen importer differential covers fractional ISO input and invalid-date failure; `message_timestamp_consumers` |
| Cache stats | [Old-only module removed](cache-stats.md); no common-pin timestamp consumer or parity claim remains |

`pi_js_compat::date::time_clip_millis` models the actual Date boundary: reject nonfinite/out-of-range as Invalid Date, otherwise truncate toward zero. It must not be used to narrow message timestamps.

## SessionInfo Date boundary

The bounded intermediate Invalid Date rejection has been removed. [SessionInfo Date traceability](session-info-dates.md) is the authority for the f64 carrier, real construction/metadata/append/persistence/render/sort boundaries and pinned proof. Raw message/accumulator numbers remain unchanged; Date TimeClip is applied only where frozen constructs a Date.

The existing actual-source buildSessionInfo regression (0.75,12.75,-12.75,maximum valid Date) now serializes f64 Date values through shared numeric-semantic JSON rather than comparing serde integer-vs-double storage shapes.

## Proof scope

Original RED: fractional12.75 rejected as i64 (`message-timestamps-red.log`). Focused JSON and consumer proof plus both all-target checks recorded in `/tmp/pi-rust-repin-delta-ai/ledger.md`. This does not close unrelated provider/type rows or establish whole-workspace runtime parity.

## AssistantImages (2026-10-25)

Frozen types.ts:474 declares an ordinary number timestamp. Native i64 rejected the source-valid12.75 JSON value (`/tmp/ai-image-timestamp-red.log`). AssistantImages now uses f64 and the existing message timestamp serializer authority, with only the two native image producer clock casts and three fixture literals adapted. New source→native→source fractional value/wire round trip uses the shared Node oracle; `assistant-images-timestamp-fixture.mts` typechecks the value against the unmodified frozen interface. No numeric-extreme cases, new carrier or image protocol change were added.
