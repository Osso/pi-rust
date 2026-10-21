# Host relay store

Frozen: `packages/coding-agent/src/core/host-relay-store.ts`, common pin `4f9e6f8a4`. Rust: `core/host_relay_store.rs`. [Contract](../../specs/host-relay.md).

| Export / branch | Behavioral proof (`host_relay_store`) |
|---|---|
| RelayMailbox, nextRelayMailbox; persistent sequence, assigned-first ordering, retry | `ts_store_rust_sequence_ts_ack_and_lossless_inbound_replay`: frozen TS creates three rows, TS/Rust alternate next, exact wire comparison including surrogate body/key and binary64 |
| ackRelayMailbox; exact payload/route/sequence CAS | same test: changed payload cannot delete; TS ACKs Rust-selected frame, Rust ACKs TS-visible frame |
| importRelayMailbox; route stripping, sender host, atomic enqueue/cursor, notification | same test: Rust import read by TS and TS import read by Rust; replay after deletion; `inbound_collision_rolls_back_cursor_and_sequence_exhaustion_rolls_back_row` |
| malformed sequence, exhausted counter, expired queue | `invalid_stored_sequence_and_expired_rows_match_frozen` compares exact frozen errors for null, zero, negative, fractional, unsafe and string sequences; exhausted counter leaves row unchanged |
| RelayChannel, nextRelayChannel; new-peer tail, no echo, oldest local row | `channel_tail_origin_dedup_expiry_and_monotonic_cursors_match_frozen`: frozen/Rust same DB and exact wire |
| ackRelayChannel; monotonic cursor | same test: TS ACK, Rust lower ACK cannot regress |
| importRelayChannel; origin dedup, expiry-before-insert, main listeners | same test: duplicate imports across runtimes remain one row; imported rows never echo; expiry retry ignored. Main-only listener notification follows shared public control DB notification path; dedicated wake-signal observation pending. |
| requireText, nullableText | protocol differential rejects blank/wrong/missing fields; nullable agent IDs exercised by every real fixture |
| Unknown nested JSON data | `parser_preserves_lossless_nested_payload_wire` plus TS→Rust→TS bytes in first test; no serde projection for authoritative payload |

Focused GREEN recorded in `/tmp/pi-rust-repin-host-relay/green-fourth.log`; final mutation/check evidence recorded in the lane report. Source selection uses the unchanged common source lock. No live state/network.
