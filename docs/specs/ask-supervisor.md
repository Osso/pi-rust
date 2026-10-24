# Ask supervisor tool

Port `packages/coding-agent/src/core/tools/ask-supervisor.ts` at immutable `4f9e6f8a4`. This tool requests advisory text without applying policy or runtime changes. [Module map](../porting/traceability/ask-supervisor.md).

## What it must do

- [x] Expose exact sequential schema/metadata, with question/context maxLength4000/8000.
- [x] Check standalone noSupervisor, child runtime, nonempty control path, then nonempty sender ID, in that order. Empty child ID alone is falsy.
- [x] Retain the sender guard: real SessionManager.open loads a persisted empty header ID, although newSession rejects caller-supplied empty IDs.
- [ ] Resolve project from context cwd and frozen default KB root. Request durable supervisor_advisory with original question, present context, sender/project IDs and180000ms timeout. Ignore tool abort/update arguments.
- [ ] Return advisory answer in text/details. Reject error responses with their reason and other kinds with the exact invalid-advisory message. Preserve UTF-16 through tool boundaries.
- [ ] Match Text reuse, joined text-only output and native errors for incompatible retained components.

## How it works

- [Traceability and proof boundaries](../porting/traceability/ask-supervisor.md).

## Implementation inventory

- `crates/pi-coding-agent/src/core/tools/ask_supervisor.rs`: tool contract and rendering.
- `crates/pi-coding-agent/src/supervisor/client.rs`: existing durable request transport.
- `crates/pi-coding-agent/src/supervisor/project_resolver.rs`: existing project lookup.

## Tests asserting this spec

- `crates/pi-coding-agent/tests/differential_ask_supervisor.rs`: guard ordering and persisted empty-ID boundary.
- `crates/pi-coding-agent/tests/session_coordination_tools.rs`: retained native schema/guard/advisory tests.
- `crates/pi-coding-agent/tests/differential_change_cwd_renderers.rs`: exact descriptor and retained Text history.

## Known gaps (current cycle)

- [ ] Complete source/native durable transport proof under a private KB mask and resident identity fixture, without starting services.
- [ ] Complete payload/response and retained-renderer proofs.

## Out of scope

- Shared runtime/storage/DB implementation changes require main approval.
- No live KB, provider or service access; fixtures must remain private and loopback-only.
