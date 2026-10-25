# RPC client

The client follows frozen `packages/coding-agent/src/modes/rpc/rpc-client.ts` and the declared protocol in `rpc-types.ts` at the [common source pin](../porting/source-lock.json). Branch proofs are mapped in [traceability](../porting/traceability/rpc-client.md).

## What it must do

- [ ] Spawn the configured executable (default `node`) with the frozen CLI argument order, environment, startup checks, stop escalation and pending-request errors.
- [ ] Preserve parsed external JSON envelopes, property presence, raw UTF-16 and unknown properties through correlation and event delivery.
- [ ] Apply frozen success truthiness and Error construction before decoding a declared getter result; distinguish missing data from explicit null.
- [ ] Settle wait/collection on the first `agent_end`, regardless of `willRetry`; subscribe before prompting.
- [ ] Expose only the frozen declared commands and return types; retain unknown properties and order in declared model reference/info records.

## How it works

- [Client and protocol traceability](../porting/traceability/rpc-client.md)
- [Protocol types traceability](../porting/traceability/rpc-types.md)

## Implementation inventory

- `crates/pi-coding-agent/src/modes/rpc/rpc_client.rs`: private child, correlation, events and typed getters.
- `crates/pi-coding-agent/src/modes/rpc/rpc_types.rs`: native declared wire records.
- `crates/pi-ai/src/types/model_properties.rs`: shared raw-property/order authority; decode/serialize visibility only is approved for client records.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_rpc_client.rs`: actual frozen client and native client against identical private stdio fixtures.
- `crates/pi-coding-agent/tests/rpc_client.rs`: native process/lifecycle and command behavior.
- `crates/pi-coding-agent/tests/rpc_types.rs`: declared DTO wire contracts.

## Known gaps (current cycle)

- [ ] Whole-module revalidation is in progress; pre-repin proof is not current acceptance.
- [ ] Main approved explicit Result errors where an external value cannot inhabit a declared getter type. Enumerate each getter and rejected field in traceability; frozen unchecked casts can return such mistyped values.

## Out of scope

- Live providers, host state, and unapproved semantic deviations.
