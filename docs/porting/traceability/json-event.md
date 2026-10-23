# json-event.ts

Frozen `4f9e6f8a4`, `packages/coding-agent/src/modes/json-event.ts`. [Contract](../../specs/json-print-event.md).
Native `modes/json_event.rs::to_json_print_mode_event` replaces the old-pin to_json_event entirely; only print mode calls it. RPC serializes original session events directly, like frozen rpc-mode.ts:372–374.

| Export/branch | Native mapping | Proof |
|---|---|---|
| JsonPrintModeEvent / JsonEvent / metadata conditional types | JsValue wire representation, no duplicate nominal carrier | complete returned JSON compared |
| MESSAGE_BODY_FIELDS, message_start | omit six message body fields | frozen differential metadata/native-unit fixture |
| ASSISTANT_BODY_FIELDS, message_update | omit root message and five assistant body fields, preserve delta/unknown fields | all streaming event tags plus raw UTF-16/key-order fixture |
| turn_end / agent_end | omit message+toolResults / messages | frozen whole-record comparison |
| tool_execution_end | omit content/details from result; preserve terminate/other metadata | frozen raw detail/unknown/terminate fixture |
| default and omitFields | retain all other values/remaining key order | message_end raw-body and default fixtures; exact native lossless stringify comparison |

Behavioral RED: q15-json-print-event-red.log, event1 retained messages that frozen drops. Prior failure (q15-no-settled-json-rpc-proof-2.log) was an obsolete oracle export name, not semantic proof. Oracle now invokes the actual unmodified toJsonPrintModeEvent; no output normalization. GREEN: q15-json-event-and-rpc-gate.log, actual frozen28-fixture differential plus4 unit cases. q15-json-print-done-error-green.log proves real print text/JSON process behavior; the same gate proves real RPC done/error forwarding. Both package all-target checks pass with0 warnings (q15-json-event-checkpoint-all-targets.log).

RPC/print DoneReason process proof uses identical full scripted start/done-error inputs on both runtimes; it does not normalize provider usage or hide the separate broader prompt-token-count mismatch (faux578 vs597). Whole RPC mode remains partial until its unrelated differences are resolved.
