# Session message routing

Frozen `extensions/agents-core/src/session-message-route.ts` at `4f9e6f8a4`; native `bundled_extensions/agents_core/session_message_route.rs`.

| Export / branch | Native | Proof |
|---|---|---|
| isRemoteSessionMessage | exact host comparison (no normalization) | mailbox_body_and_remote_host_are_literal_and_unknown_local_sessions_are_rejected |
| sessionMessageRouteError blank host, remote requires session | same helper | same test; invalid routes create no messages |
| unknown local session, local-only hostname | is_known_local_session | unknown-local rejection; local hostname branch proof pending |
| child-only live listeners reject main/supervisor | read_live_runtime_mailbox_session_agent_ids | implementation; focused branch proof pending |

Frozen mailbox-tools.ts registration adapters map required body verbatim to literal transport input message; no message fallback. Internal pi.messages transport input remains message, as frozen does. Exact metadata is covered by the 12-tool registration differential.

Runtime remote messages bypass parent/child local edge lookup, persist recipientHost and original whitespace body in the canonical mailbox, return queued-for-relay text, and do not invoke the local session-message callback. Remote queue test uses no network/live state.

Cross-host relay now consumes this canonical queue through the separately ported [protocol](host-relay.md), [SSH client](host-relay-client.md), [resident](host-relay-resident.md), and [store](host-relay-store.md). Their sandboxed/differential proof is recorded there; no live-host delivery is claimed.
