# Host relay SSH client

Frozen: `packages/coding-agent/src/core/host-relay-client.ts`, common pin `4f9e6f8a4`. Rust: `core/host_relay_client.rs`. [Contract](../../specs/host-relay.md).

| Export / branch | Proof (`host_relay_resident`) |
|---|---|
| RelaySettings / SSH argv and default command | `ssh_peer_validation_and_backoff_boundaries`; `client_reconnects_after_failure_preserves_queue_and_stops_ssh_on_term` records actual fake-executable argv |
| runSshRelayClient; outage log, reconnect, durable local queue, cancellation | second test: executable fake exits 255 first, second connection receives/ACKs canonical mailbox; exactly two children, pending row disappears only on ACK, SIGTERM removes identity and reaps fake peer |
| Blank/option alias validation; half/full jitter, 30s cap | first test checks boundary inputs and failure counts 0/1/4/5/100 |
| connectRelay; stderr tail, protocol failure cleanup and 1s forced kill | ported; ordinary failure/TERM tested. Dedicated 4096-UTF16-unit stderr truncation, ignored-SIGTERM kill and 30s reset differential pending |

`frozen_client_runs_same_fake_outage_reconnect_and_ack_on_rust_queue` runs the actual frozen client against the same executable fake and Rust-created canonical queue, verifies its exact outage log/argv, ACK deletion and child termination. `spawn_failure_is_logged_like_frozen_without_executing_any_ssh` compares frozen/native EACCES logs; its PATH contains only a non-executable fake SSH and Node, so it cannot resolve a real SSH executable. RED native log was `Permission denied (os error 13)`, fixed by the shared Node spawn-error formatter.

No real SSH is invoked: executable fakes live inside sandbox-private PATH. Final mutation/check evidence: [relay proof ledger](../host-relay-proof.md).
