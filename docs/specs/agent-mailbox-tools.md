# Agent mailbox tools

Model tool arguments and runtime routing follow frozen `extensions/agents-core/src/mailbox-tools.ts`, `session-message-route.ts`, and `runtime.ts` at `4f9e6f8a4`.

## What it must do

- [x] Expose exact frozen body/toHost schemas, descriptions, guidelines and registration order.
- [x] Send body verbatim; do not accept retired model-tool message arguments as a fallback.
- [x] Reject blank toHost, remote host without target session, and unknown local sessions before producing mailbox rows.
- [x] Persist remote messages locally with recipientHost and explicit recipient session/agent; report queued-for-relay.
- [ ] Reject main/supervisor routing to known local sessions with only child listeners.

## How it works

- [Routing traceability](../porting/traceability/session-message-route.md)

## Implementation inventory

- `crates/pi-coding-agent-extensions/src/bundled_extensions/agents_core/mailbox_runtime.rs` — model adapters, source mailbox transport and enqueue.
- `crates/pi-coding-agent-extensions/src/bundled_extensions/agents_core/session_message_route.rs` — local/remote route validation.

## Tests asserting this spec

- `tests/question_relay.rs::mailbox_body_and_remote_host_are_literal_and_unknown_local_sessions_are_rejected`
- `tests/differential_agents_core_deterministic.rs::registration_metadata_matches_the_pinned_typescript_registration`

## Known gaps (current cycle)

- [ ] Local hostname/child-only listeners/callback behavior need focused branch proof.

## Out of scope

- Actual cross-host transfer requires unported core host-relay modules; this slice proves the canonical outbound queue, not remote receipt.
