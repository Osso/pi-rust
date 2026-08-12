# Dependencies

All direct dependencies are exact-pinned.

## Runtime

### `pi-coding-agent`

- Auth commands and pure config-value reference parsing use only the Rust standard library.
- `serde = 1.0.229` — exposes the typed serialization contract used by RPC JSONL records.
- `serde_json = 1.0.151` — produces compact semantic JSON before the LF record delimiter.

### `pi-agent-core`

No runtime dependencies for output truncation or binary-output sanitization.

### `pi-protocol`

No runtime dependencies.

## Development

- `serde = 1.0.229` — derives typed JSON-lines request and response structures for differential tests.
- `serde_json = 1.0.151` — serializes and parses the protocol, RPC JSONL, truncation, auth-command, and config-reference differential-oracle messages.
