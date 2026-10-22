# Lossless tools fixture sweep

Frozen source: common pin in [source-lock](../source-lock.json). Native fixtures retain `JsString`/`JsValue`; generated OS paths require strict UTF-8. Runtime proofs use sandboxed Cargo tests only. This ledger is bounded fixture evidence, not whole-module acceptance.

| Fixture scope | Revision | Proof | Remaining gap |
|---|---|---|---|
| `control_command`, `repin_control_{agents,exports,lifecycle,mailbox,migrations,raw_json,reciprocal,requests,retention_clock,sessions,validation,whitespace}` | `f6c128b8` | `cargo test -j4 -p pi-coding-agent` selecting these 13 targets: 71 PASS, zero warnings; `fixture-control-small-test.log` | None in selected assertions; broader sweep pending |
| `differential_control_db`, `differential_multi_agent_store` | `b6107565` | Targeted Cargo test: 4 PASS, zero warnings; `fixture-control-diffs-test.log` | Incoming ASCII restriction removed by the dedicated native incoming slice below. |
| `lossless_incoming_content` | RED `81c32624`; production correction follows | Actual sandboxed Cargo RED: native DTO ingress rejects lone units (`unexpected end of hex escape`); `incoming-content-incrate-red.log`. GREEN pending. | Producer accepts native content, DTO/claim return native content, and SQLite binding alone repairs unmatched units. |

Internal serialized process identities remain scalar by the existing exception; fixture parsers decode their JSON losslessly before typed deserialization. Scalar provider/model/role/thinking tags are asserted at their existing typed API boundary. Human content and arbitrary argument JSON have no scalar exception.
