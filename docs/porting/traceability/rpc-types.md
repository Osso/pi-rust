# RPC types traceability

Frozen source: `packages/coding-agent/src/modes/rpc/rpc-types.ts` at [common pin 4f9e6f8a4](../source-lock.json). This module contains declarations only. The native records represent its declared branches; actual external wire logic is separately literal JsValue in [RPC mode](rpc-mode.md) and [RPC client](rpc-client.md), not an unchecked cast through these records.

| Frozen lines/type | Native capability | Behavioral proof |
|---|---|---|
|20–80 `RpcCommand` | All32 declared variants, every required/optional field, raw message/path/id strings, client id appended last | `rpc_types::parses_every_*`, command serialization test; old-only `get_available_thinking_levels` is explicitly rejected after its RED |
|89–99 `RpcSlashCommand` | Shared `SlashCommandInfo` and source enum; description omission, source metadata | `serializes_slash_commands_and_omits_absent_descriptions`; real frozen-server `get_commands` getter proof |
|105–117 `RpcSessionState` | All12 fields; raw names/paths/id; declared `number` counts are f64, not u64; shared number serialization | State field-order/omission test; `session_count_numbers_follow_the_declared_number_type` RED→GREEN; actual frozen-server state getter |
|125–225 `RpcResponse` | Native known-envelope construction; raw JsString ids/commands/errors and optional raw JsValue data distinguish null from omission | Envelope serialization/parsing tests; lossless unit tests; actual process RPC mode. Client correlation now returns raw JsValue without this DTO conversion |
|234–266 `RpcExtensionUIRequest` | All9 methods; raw UTF-16 in every string/id; optional fields omitted; numeric timeouts use shared formatting | All9 exact-wire request serializations; `declared_ui_strings_preserve_every_utf16_unit_when_decoded` covers every method after real scalar-buffering RED |
|274–277 `RpcExtensionUIResponse` | Value, confirmed and cancelled branches; raw id/value; `cancelled:false` rejects on decode/serialize because declaration requires true | All3 branch parsing; raw-unit test; `ui_cancellation_requires_the_declared_true_literal` RED before validation |
|283 `RpcCommandType` | `command_type` and32-entry `COMMAND_TYPES` | Every command branch plus old-only rejection; client timeout/error use |

## Boundaries and exclusions

- `ThinkingLevel`, queue modes, streaming behavior, notification types and widget placements reuse their declared shared enums. Model records in protocol state/response data use the domain Model; the client-specific declared ModelReference/ModelInfo shapes are separate and preserve ordered extras.
- Native serde record parsing is a declared-branch projection, not frozen runtime routing. Process ingress stays JsValue. Mixed UI answer properties, invalid runtime types, raw unknown command types and arbitrary echoed ids are handled by the raw mode/client authorities, never normalized through the enum decoder.
- Command provider/model-id strings now remain JsString on the wire too (`set_model_command_keeps_raw_string_fields_on_the_wire`, genuine prior scalar-rejection RED). Domain Model metadata's separate opaque-scalar approval is not used to narrow the client transport.
- `RpcResponse.command` remains optional JsString for the frozen unknown-type error path whose undefined command is omitted; non-string external command echoes use the mode's raw envelope rather than this DTO.
- The type-only source has no runtime branches or source bugs to classify. Actual process behavior and approved deep-return gaps are listed in the corresponding client/mode traces.

Current proof ledger: `/tmp/pi-rust-repin-lossless-interactive/rpc-client-proof-ledger.md`. Final DTO target15/15 plus4 lossless lib cases pass after UI/count/true-literal/raw model-command fixes. Whole-pair behavior and scoped gates are recorded in the [checkpoint proof](../rpc-client-proof.md).
