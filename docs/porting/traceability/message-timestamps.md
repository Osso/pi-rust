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
| Cache stats | PreviousRequest stores f64; idle arithmetic preserves fractional/NaN/Infinity. Native behavioral test. Frozen cache-stats.ts is absent, so no frozen-module oracle claim |

`pi_js_compat::date::time_clip_millis` models the actual Date boundary: reject nonfinite/out-of-range as Invalid Date, otherwise truncate toward zero. It must not be used to narrow message timestamps.

## Temporary SessionInfo dependency

Parent approved a bounded intermediate boundary on2026-10-22: SessionInfo.created/modified remain i64 until the separate Date-representation slice. The accumulator retains f64; finish_session_info applies TimeClip only at frozen new Date(number), returning SessionManagerError for invalid dates instead of clamping. build_session_info's existing Option interface returns None for this error; metadata refresh propagates it. This explicit temporary rejection is **not parity** with frozen Invalid Date rendering/sorting. Remove it when SessionInfo can retain Invalid Date; do not expand it into a fallback.

Actual-source buildSessionInfo differential covers0.75,12.75,-12.75 and the maximum valid Date. Protected interactive SessionInfo consumers were not edited. Parent221 owns Date representation/render/sort and invalid-date behavior.

## Proof scope

Original RED: fractional12.75 rejected as i64 (`message-timestamps-red.log`). Focused JSON and consumer proof plus both all-target checks recorded in `/tmp/pi-rust-repin-delta-ai/ledger.md`. This does not close unrelated provider/type rows or establish whole-workspace runtime parity.
