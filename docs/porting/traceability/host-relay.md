# Host relay protocol

Frozen: `packages/coding-agent/src/core/host-relay.ts`, common pin `4f9e6f8a4`. Rust: `core/host_relay.rs`. [Contract](../../specs/host-relay.md).

| Export / branch | Proof |
|---|---|
| HostRelayOptions, runHostRelay; initial hello, EOF/abort, 2s default poll, stop-and-wait scanning | `host_relay::serve_dispatches_before_provider_initialization_and_exits_on_eof`; `host_relay_protocol::paired_streams_retry_lost_ack_without_resurrecting_deleted_mailbox` |
| parseFrame; object/type checks; positive safe IDs; nonblank and nullable fields | `validation_handshake_routes_and_acks_match_frozen`: frozen runHostRelay differential matrix |
| Missing, duplicate, self hello; mailbox host/sequence; channel origin | same matrix; includes distinct lone-surrogate peer/origin IDs |
| Matching ACKs only, inbound import then ACK, replay suppression | store reciprocal tests; protocol duplicate mailbox/channel differential; paired lost-ACK/reconnect proof |
| CR/LF/CRLF, partial/EOF frame decoding | differential matrix includes bare CR between frames; standalone process EOF |
| Queued writes, continued reads under backpressure, scan suppression | `incoming_mailbox_progresses_while_writer_is_blocked_and_queued_ack_drains`: frozen Writable and native blocked AsyncWrite both import before unblocking and emit identical hello/ACK |
| EOF without output drain; queued writes continue after return | `eof_completes_without_drain_and_retains_queued_output`: verifier blocked-output probe regression, RED before fix; frozen independent driver resolves immediately |
| Lossless peer and higher-sequence address retries | `surrogate_peer_canonical_sender_host_matches_frozen`, `surrogate_routing_higher_sequence_retry_matches_frozen`: actual frozen protocol, exact canonical SQL bytes, both ACKs and durable cursor 2; four address fields |
| Input/output failure, cleanup | CLI/fake-SSH TERM and protocol failures; dedicated injected I/O-error tests pending |
| runRelayServe / CLI dispatch | real `pi relay serve` process with no provider credentials, hello then EOF succeeds; RED before implementation returned No models available |

Current proof: [relay proof ledger](../host-relay-proof.md). No live transports.
