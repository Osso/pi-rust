# Lossless tools fixture sweep

Frozen source: common pin in [source-lock](../source-lock.json). Native fixtures retain `JsString`/`JsValue`; generated OS paths require strict UTF-8. Runtime proofs use sandboxed Cargo tests only. This ledger is bounded fixture evidence, not whole-module acceptance.

| Fixture scope | Revision | Proof | Remaining gap |
|---|---|---|---|
| `control_command`, `repin_control_{agents,exports,lifecycle,mailbox,migrations,raw_json,reciprocal,requests,retention_clock,sessions,validation,whitespace}` | `f6c128b8` | `cargo test -j4 -p pi-coding-agent` selecting these 13 targets: 71 PASS, zero warnings; `fixture-control-small-test.log` | None in selected assertions; broader sweep pending |
| `differential_control_db`, `differential_multi_agent_store` | native fixture checkpoint | Targeted Cargo check: zero errors/warnings; behavioral run pending | Incoming content currently asserts ASCII before the old scalar API; temporary fixture restriction only, not an approved exclusion. Next dedicated RED-first slice must widen the whole incoming API path and remove this conversion. |

Internal serialized process identities remain scalar by the existing exception; fixture parsers decode their JSON losslessly before typed deserialization. Scalar provider/model/role/thinking tags are asserted at their existing typed API boundary. Human content and arbitrary argument JSON have no scalar exception.
