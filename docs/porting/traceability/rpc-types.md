# RPC types traceability

Pinned source: `packages/coding-agent/src/modes/rpc/rpc-types.ts` at commit `532f7d12cd19235081bd4d6e425bde185652d86f`.

The module is type-only; proof is wire-format behavior. `tests/rpc_types.rs` parses every command and
extension-UI response variant and serializes every response/request/state/command shape, asserting the
exact `JSON.stringify` line (field order, omitted `undefined`, explicit `null`).

| TypeScript source | Observable contract | Rust mapping and proof |
|---|---|---|
| `RpcCommand` | Discriminated union over `type`; every variant has optional `id` | `RpcCommandBody` (internally tagged) + `RpcCommand { body, id }`. `parses_every_*` cover all 32 variants; `serializes_command_bodies_with_ts_field_order_and_omits_undefined_fields` covers body field order and `id` appended last |
| `RpcCommandBody = DistributiveOmit<RpcCommand, "id">` | Client command without correlation id | `RpcCommandBody`; `RpcClient::send` appends `id` after the body fields (`{ ...command, id }`) |
| `RpcCommandType` | `RpcCommand["type"]` | `RpcCommandBody::command_type()`; used by request-timeout messages |
| `RpcSlashCommand` | `{ name, description?, source, sourceInfo }` | `RpcSlashCommand`; `serializes_slash_commands_and_omits_absent_descriptions` |
| `RpcSessionState` | 12 fields, `model`/`sessionFile`/`sessionName` optional, `steeringMode`/`followUpMode` queue literals | `RpcSessionState`; `serializes_session_state_in_ts_field_order_and_omits_undefined_fields` pins key order and omission |
| `RpcResponse` success/error union | `{ id?, type: "response", command, success, data? }` or `... success: false, error }` | `RpcResponse` + `RpcResponseTag`; `serializes_response_envelopes_in_ts_field_order`, `parses_response_lines`. `data` is opaque JSON: the wire only carries a payload, and `RpcClient` typed accessors deserialize it |
| `RpcExtensionUIRequest` | Nine `method` variants; `timeout`/`placeholder`/`prefill`/`notifyType`/`statusText`/`widgetLines`/`widgetPlacement` omitted when `undefined` | `RpcExtensionUiRequest` + `RpcExtensionUiRequestPayload`; `serializes_every_extension_ui_request_in_ts_field_order` covers all nine |
| `RpcExtensionUIResponse` | Exactly one of `value`, `confirmed`, `cancelled` | `RpcExtensionUiResponse` + untagged payload; `parses_every_extension_ui_response_shape` |

## Exclusions and deviations

- Typed Rust APIs exclude invalid JavaScript runtime input types (decision 2026-08-12): non-string `id`
  and `type`, unknown `type` values, and non-boolean `success`/`confirmed`/`cancelled` are rejected at
  deserialization. The TypeScript code casts unchecked.
- `RpcResponse.command` is `Option<String>` so a missing `command` (`undefined` in TypeScript) is omitted
  instead of serialized. Non-string command values are excluded by the same typed-input decision.
- `RpcSessionState.messageCount`/`pendingMessageCount` are `u64`; JavaScript numbers outside the safe
  integer range are excluded.
- `timeout` uses `pi_js_compat::number::serialize_option` so integral values print as `100`, not `100.0`.
- `RpcExtensionUiResponsePayload::Cancelled` carries a `bool` (presence-tagged); the TypeScript literal
  type is `true`, but the runtime only checks the key and truthiness.
- No source bug found.
