# Synchronous stream acquisition failures

Preserve frozen `compat.ts`, `api/lazy.ts`, `types.ts`, agent-loop.ts and Agent lifecycle boundaries at the [common pin](../porting/source-lock.json). A synchronous throw is not a stream error event.

## What it must do

- [x] Registered raw/simple callbacks, missing API/environment lookups and API mismatches reject acquisition through CallbackError before a stream exists.
- [x] Preserve Error branding/properties and arbitrary thrown values, including lone UTF16, without alternate dispatch or text projection.
- [x] Lazy adapters return their outer stream immediately. Only the frozen async setup catch converts inner acquisition rejection to one assistant error event. A rejection of String(error) itself is process-uncaught, as proven by actual-source child processes.
- [x] Agent.runWithLifecycle converts at its own catch and always clears run state. Low-level agentLoop/Continue has no conversion catch: iteration and result reject with the original value.
- [x] Successful request/event behavior stays unchanged; optional deferred methods are not modified.

## How it works

- [Registry symbol/branch map and checkpoint](../porting/traceability/model-registry.md)
- [Compatibility](../porting/traceability/compat.md)
- [Lazy catch](../porting/traceability/api-lazy.md)
- [Agent](../porting/traceability/agent.md) and [low-level loop](../porting/traceability/agent-loop.md)

ProviderStreams, registered callback and Provider/createProvider acquisition returns Result. ModelsCollection and LazyApi retain infallible outer streams; their setup consumes that Result. Existing StreamFailure wraps the same CallbackError, not a second rejection representation. `throw_callback_uncaught` moved unchanged to pi-js-compat in pure commit0863de71; its old Core path re-exports it. The one process hook remains in Core-base.

## Behavioral proof

| Boundary | Actual frozen/native proof |
|---|---|
| Registered direct dispatch | `model_registry_stream_throw_probe`: raw/simple × TypeError+properties/raw UTF16,4cases; no stream exists |
| Agent and low-level loop | `model_registry_agent_stream_throw`:3full Agent events/state outcomes plus6loop/continue iteration/result rejection outcomes, including failing String conversion |
| Lazy setup | `provider_lazy_acquisition`:4full raw/simple Error/raw UTF16 event/result outcomes; separate source/native process proof for uncaught conversion rejection |
| GitLab required consumer | `example_gitlab_duo`:4full backend acquisition negatives across Anthropic/OpenAI paths; source/native process proof for the catch's conversion rejection;6active tests pass |
| Positive lifecycle | `model_registry_dispatch_lifecycle`:6full callback/input/event/result captures and6registration/upsert/OAuth/reset/unregister states;0HTTP |

Codex direct/lazy2controls pass; shared lazy/compat/alias/models47regressions and Agent31/loop43regressions pass. All16existing authority tests pass after the pure move and parent hook-diagnostic merge. Private ignored process entrypoints are exercised by active parent tests, not omitted behavior.

## Checkpoint

Parent36aba9a2 merged at0639d2d8. Workspace/all-target check0errors/0warnings; all-feature warning Clippy0owned findings, including new-signature audit at unchanged callers (1664existing warnings). Strict scoped TS/Biome/fmt and source inventory686rows/186679lines pass. [Registry map](../porting/traceability/model-registry.md#remaining-checkpoint-work) records artifact/revision bounds. Independent acceptance remains the parent's gate; this does not certify whole-provider parity.

## Out of scope

Protocol changes, source patches, dependencies, live endpoints/credentials and unrelated callback interfaces. The raw/simple Result migration does not widen deferred APIs.
