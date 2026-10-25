# Synchronous stream acquisition failures

Preserve the frozen `packages/ai/src/compat.ts`, `api/lazy.ts`, `types.ts` and agent-loop failure boundaries at the [common pin](../porting/source-lock.json). Registration callbacks must not turn a synchronous throw into an invented stream event.

## What it must do

- [ ] Raw/simple registered callbacks, missing API/environment lookup failures and API mismatches reject stream acquisition through the existing CallbackError authority before a stream is returned.
- [ ] Preserve native Error class/properties and arbitrary thrown values, including lone UTF-16 units, without text projection or an alternate dispatch API.
- [ ] Lazy adapters return their outer stream immediately; only their frozen async setup catch converts an inner module acquisition failure into an assistant error event.
- [ ] Agent-loop consumers catch acquisition failure at the frozen async-loop boundary and emit the exact assistant/event/state outcome rather than losing the rejection or failing outside that catch.
- [ ] Successful provider request/event behavior and optional deferred methods remain unchanged.

## How it works

- [Compatibility traceability](../porting/traceability/compat.md)
- [Registry consumers](../porting/traceability/model-registry.md)
- [Lazy provider traceability](../porting/traceability/api-lazy.md)

## Implementation inventory

- `crates/pi-ai/src/request.rs`: shared ProviderStreams acquisition result.
- `crates/pi-ai/src/compat/mod.rs`: registered callback/API wrapper dispatch.
- `crates/pi-coding-agent-models/src/core/model_registry.rs`: extension registration callback contract.
- `crates/pi-ai/src/api/lazy.rs`: frozen caught async setup boundary.
- `crates/pi-ai/src/models.rs`: Provider/createProvider pass acquisition Result unchanged; ModelsCollection returns its outer stream and delegates rejection conversion to lazy setup.
- `crates/pi-ai/src/legacy_api_aliases.rs`: direct Result forwarding, no additional catch.
- `crates/pi-agent-core/src/agent_loop.rs`: default consumer error boundary.

## Tests asserting this spec

`crates/pi-coding-agent/tests/model_registry_stream_throw_probe.rs` contains the actual-source four-case contract and native compiler RED. Agent-loop end-to-end and positive dispatch/lazy controls remain pending.

## Known gaps (current cycle)

- [ ] Complete the prerequisite consumer migration and current full-result proofs.

## Out of scope

Provider protocol changes, source patches, dependency additions, live endpoints/credentials and unrelated callback interfaces. ProviderStreams changes are limited to the required raw/simple acquisition result.
