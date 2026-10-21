# Host relay SSH client

Frozen: `packages/coding-agent/src/core/host-relay-client.ts`, common pin `4f9e6f8a4`. Rust: `core/host_relay_client.rs`. [Contract](../../specs/host-relay.md).

| Export / branch | Proof (`host_relay_resident`) |
|---|---|
| RelaySettings / SSH argv and default command | `ssh_peer_validation_and_backoff_boundaries`; `client_reconnects_after_failure_preserves_queue_and_stops_ssh_on_term` records actual fake-executable argv |
| runSshRelayClient; outage log, reconnect, durable local queue, cancellation | second test: executable fake exits 255 first, second connection receives/ACKs canonical mailbox; exactly two children, pending row disappears only on ACK, SIGTERM removes identity and reaps fake peer |
| Blank/option alias validation; half/full jitter, 30s cap | first test checks boundary inputs and failure counts 0/1/4/5/100 |
| connectRelay; stderr tail, protocol failure cleanup and 1s forced kill | ported; ordinary failure/TERM tested. Dedicated 4096-UTF16-unit stderr truncation, ignored-SIGTERM kill and 30s reset differential pending |

No real SSH is invoked: fake `ssh` is an executable Python program inside sandbox-private PATH. Client/fake process proof is not presented as a deterministic TS differential; store/protocol differentials cover the shared frames. Final mutation/check evidence: lane report.
