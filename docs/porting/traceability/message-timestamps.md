# Message timestamps — parked WIP

Branch `repin/delta-ai-timestamp`, based on `6ec0609f`. Not integration-ready.

Frozen `ai/src/types.ts:414–452` declares User/Assistant/ToolResult timestamp as number. Frozen `agent-core/src/harness/messages.ts:86–135` uses Date.getTime(), including NaN from invalid date strings.

Three message fields, AssistantMessage::empty and Faux timestamp options now retain f64. Serialization delegates to existing JsValue number JSON writer: exact Node number spelling (including 1e20), negative zero→0, nonfinite→null. No null→NaN decode or integer narrowing added. Native integer clocks convert to f64 only at message construction. AgentMessage timestamp accessor and harness projections retain numbers without finite/integer filtering.

## Proof

- RED before production: fractional12.75 deserialization rejected as i64, `message-timestamps-red.log`.
- GREEN2 focused tests: three roles ×13 values, plus fractional decoding; actual frozen invalid-date creator supplies the NaN oracle. Full serialized bytes and parsed round-trip values compared; `message-timestamps-green.log`.
- Consumer compilation still blocked: old-only `virtual_retained_entries` copies message time into Entry.timestamp:i64. Parent220's session replacement deletes it. No Entry schema migration/cast/fallback added.
- AI all-target compiler inventory:90 remaining test/fixture type errors, mostly integer literals and old integer test-clock helpers. `timestamp-ai-all-targets-errors-1.json`.

Artifacts/commands/revision scope: `/tmp/pi-rust-repin-delta-ai/ledger.md`. Finish workspace consumers and both requested all-target checks after parent session replacement merges, then publish dependency SHA. No passing-workspace claim.
