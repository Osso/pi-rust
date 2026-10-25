# Synchronous stream acquisition failures

Preserve the frozen `packages/ai/src/compat.ts`, `api/lazy.ts`, `types.ts` and agent-loop failure boundaries at the [common pin](../porting/source-lock.json). Registration callbacks must not turn a synchronous throw into an invented stream event.

## What it must do

- [ ] Raw/simple registered callbacks, missing API/environment lookup failures and API mismatches reject stream acquisition through the existing CallbackError authority before a stream is returned.
- [ ] Preserve native Error class/properties and arbitrary thrown values, including lone UTF-16 units, without text projection or an alternate dispatch API.
- [ ] Lazy adapters return their outer stream immediately; only their frozen async setup catch converts an inner module acquisition failure into an assistant error event.
- [ ] Agent.runWithLifecycle catches acquisition failure after unchanged async-loop propagation at the frozen boundary and emit the exact assistant/event/state outcome rather than losing the rejection or failing outside that catch. Low-level agentLoop/Continue exports have no conversion catch: iteration and result must reject with the original value.
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
- `crates/pi-agent-core/src/agent/agent_loop.rs`: low-level exports fail the stream with unchanged acquisition rejection; no catch conversion.
- `crates/pi-agent-core/src/agent/agent.rs`: Agent lifecycle catch conversion and finally cleanup.
- `crates/pi-ai/src/utils/event_stream.rs`: existing StreamFailure carries CallbackError without a text projection.

## Tests asserting this spec

`crates/pi-coding-agent/tests/model_registry_stream_throw_probe.rs`: four actual-source raw/simple × TypeError/raw-value acquisition cases GREEN at9256b49b. Existing uncaught process-authority targets also pass after pure move0863de71. `model_registry_agent_stream_throw.rs` adds default Agent end-to-end events/state and conversion-rejection cleanup: behavioral RED on raw-value failure text and conversion rejection; TypeError control GREEN. Native Agent catch conversion and finally cleanup are GREEN atbcf84da0 for all three cases. The same target now adds six low-level loop/continue cases: RED q15e-low-level-stream-rejection-red-2 exposes a StreamFailure text projection; current low-level proof remains pending. GitLab's required consumer catch has four actual-source Error/raw UTF16 negatives plus source/native process tests: RED q15e-gitlab-acquisition-catch-red proves frozen conversion rejection exits1 without an error event while native exits0. Parent approved exact catch conversion/native message construction and the proven uncaught boundary. Positive dispatch/lazy controls remain pending.

## Known gaps (current cycle)

- [ ] Complete the prerequisite consumer migration and current full-result proofs.

## Out of scope

Provider protocol changes, source patches, dependency additions, live endpoints/credentials and unrelated callback interfaces. ProviderStreams changes are limited to the required raw/simple acquisition result.
