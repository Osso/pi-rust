# Host relay

Reference: frozen `4f9e6f8a4` `core/host-relay{,-client,-resident,-store}.ts`.

## Contract

- `pi relay serve` runs the bidirectional JSONL protocol on stdin/stdout before settings/provider/session startup. `pi relay connect` loads only global relay settings and maintains its SSH connection; absent settings succeeds without spawning.
- Each endpoint sends one hello, rejects missing/duplicate/self hello, validates positive safe-integer IDs, required nonblank strings and nullable fields. Mailboxes must target this host and agree with their relay sequence; channels must originate at the peer. UTF-16 peer identity and canonical senderHost are exact; only SQLite TEXT bindings replace lone surrogates. Higher-sequence retries compare original UTF-16 routing addresses and literal kind. Input EOF resolves without waiting for output drain; already queued writes retain independent progress. CR, LF and CRLF delimit frames; stream decoding uses Node-compatible UTF-8 replacement.
- Outbound canonical remote mailbox rows remain pending during outages. Assign persistent increasing per-peer sequences atomically; retry the smallest assigned sequence until matching ACK or creation-age expiry. ACK deletes only the matching original serialized payload, host and sequence. Import strips outbound routing, adds senderHost, enqueues and advances its inbound high-water mark atomically, then notifies the recipient. Replayed/deleted inbound messages never resurrect.
- New peers initialize their channel cursor at the current tail. Export only local-origin rows. Persist monotonically increasing ACK cursors. Import preserves origin identity, deduplicates `(origin_host, origin_id)`, rejects expired retries, and wakes only main-session listeners.
- Store schemas, cursor names and compact JSON bytes interoperate with frozen TypeScript. Mailbox unknown fields, UTF-16 strings/keys, key order and binary64 formatting survive without a serde projection at the persistence/wire boundary. A nonblank wire `kind` is preserved literally even outside the global Rust enum (for example `"custom"`), matching the frozen unchecked cast; the private validation adapter must never replace the stored kind.
- SSH argv is `-o BatchMode=yes -o ServerAliveInterval=15 <peer> <remoteCommand-or-pi relay serve>`. Reject blank/option peers. Retain 4096 UTF-16 stderr units, log failures, reconnect with frozen half-to-full exponential jitter (1–30s base), reset failure count after a 30s connection and terminate/reap the child on cancellation/protocol failure. Output queues do not block incoming frame processing; scans respect output backpressure.
- One resident per control DB uses frozen proper-lockfile paths/timing, atomic mode-0600 `.relay.json` identity, append-only `.relay.log`, private parent directories, and SIGTERM/SIGINT cleanup. Startup asynchronously reuses the current version or replaces only a verified older Pi process. No relay starts for standalone workers.

## Boundaries

Native binaries use the frozen compiled-binary launch branch. Linux `setsid` supplies Node's detached-session semantics without unsafe pre-exec code. Tests use private databases, streams and executable SSH fakes, never real SSH or hosts. No new Rust dependencies. The identity's version is the active runtime's package version, not a relay-local constant; current frozen/native package versions differ (`0.80.3` / `0.84.1`) and must not be silently substituted.

The CLI flag/diagnostics and early standalone rejection are included. Broader standalone orchestration/restart-handoff suppression remains the pre-existing main/CLI slice Q14 gap; this relay slice does not claim full `--no-supervisor` parity. [Main traceability](../porting/traceability/main.md).

## Proof

Four focused test targets: `host_relay`, `host_relay_store`, `host_relay_protocol`, `host_relay_resident`. Store and protocol use the immutable modules through `oracle/host-relay-oracle.ts`; production subprocess/resident tests run only under the mandatory Cargo sandbox. Traceability is per [store](../porting/traceability/host-relay-store.md), [protocol](../porting/traceability/host-relay.md), [client](../porting/traceability/host-relay-client.md), and [resident](../porting/traceability/host-relay-resident.md).
