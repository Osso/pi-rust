# Frozen runtime mailbox recipient acceptance

Source: `packages/coding-agent/src/core/runtime-mailbox-acceptance.ts:1-88`, `runtime-mailbox-delivery.ts:1-64`, and `agent-session.ts:3586-3611,3743-3779` at `4f9e6f8a4`.

| Contract | Native mapping | Proof / limitation |
|---|---|---|
| Persist recipient before first assistant; append message and refs together or custom receipt; fsync before canonical deletion | `runtime_mailbox_acceptance::RuntimeMailboxAcceptance::accept_messages` | `agent_session_runtime_mailbox::polling_claims_mailbox_messages_only_after_listener_binding` has two distinct claims, two durable refs, one user record after MessageEnd; RED had zero refs |
| Snapshot payload and store-ref match before CAS delete | `delete_accepted_claims` | Uses existing snapshot/CAS DB functions; runtime failure injection proof pending |
| Release unaccepted claims in finally, preserving persisted receipts | `release_unaccepted_runtime_mailbox_messages`, `AgentSession::deliver_ready_runtime_mailbox_messages` | Scanner/equality source mapping retained; error-boundary runtime proof pending |
| Intercept → accept marker; steer → accept actual queued message; prompt → accept transformed message before Agent dispatch | `AgentSession::deliver_claimed_runtime_mailbox_messages`, `prompt_turn`, `queue_prompt_if_streaming` | Prompt and steering integration targets; interceptor/error branch tests pending |
| Refresh projections after durable acceptance, not second steering lifecycle mutation | `mark_store_mailbox_message_delivered`, `mark_store_steering_delivered` | Steering completion integration test |
| Prevent second transcript append on MessageEnd | Session-local FIFO of accepted clones; first full equality match consumed once | Approved internal representation adaptation via parent, 2026-10-21. Agent core clones owned enums, so JS WeakSet identity cannot survive dispatch. All fields including timestamp/content participate; equal clones produce identical transcript bytes. Disposal clears unreachable queued entries. Removing skip must fail one-user-record assertion |
| Tool-result delivery descriptor | `read_runtime_mailbox_tool_delivery` and typed descriptor | Descriptor parsing exists; tool-result producer/persistence integration not established by this bounded runtime slice |

The obsolete steering test queried a deleted SQLite rowid after completion had inserted terminal `message_2` at the reused rowid. Frozen mailbox schema has no non-reuse promise; the assertion now checks the original immutable store ref.

No whole-agent-session parity or crash-recovery acceptance claim. Sender store persistence already precedes enqueue (`multi-agent-store.ts:939-945`); historical sender fixture clocks caused missing-row errors through DB expiry, not absent sender JSONL append.
