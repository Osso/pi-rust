# Extension compaction callbacks

Frozen `agent-session.ts:5432-5438` at `4f9e6f8a4` governs the extension-context compaction binding.

## What it must do

- [x] Forward custom instructions to compaction and its successful result to `onComplete`.
- [x] Route a compaction or completion rejection to `onError`. Preserve Error branding/properties; convert non-Error throws with the shared JavaScript String authority into Error as frozen does.
- [x] Let an `onError` throw escape to the existing uncaught callback boundary. Do not catch it as another completion failure.
- [x] Use existing `CallbackError` results for both callbacks; no panic carrier between callbacks.

## How it works

- [Agent-session traceability](../porting/traceability/agent-session.md).

## Implementation inventory

- `crates/pi-coding-agent-core/src/core/extensions/types.rs`: callback signatures.
- `crates/pi-coding-agent-core/src/core/agent_session.rs`: compact binding and callback dispatch.
- `crates/pi-coding-agent/examples/extensions/trigger_compact.rs`: completion/error notification consumer.

## Tests asserting this spec

- `tests/agent_session_compaction_extensions.rs::extension_compact_callback_histories_match_actual_source_boundaries`: real native SDK/context/compaction hook; frozen actual `_bindExtensionCore` with controlled producer; success, completion throw, error callback escape.
- The ignored `compact_callback_escape_probe` is explicitly run as a private child, with the existing uncaught observer.

## Known gaps (current cycle)

- Final GREEN/checkpoint gates pending; actual three-history RED is recorded before forwarding/catch correction.

## Out of scope

- Other AgentSession behavior, provider compaction semantics, UI-notify/event-bus migration, and live services/providers.
