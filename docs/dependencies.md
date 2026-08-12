# Dependencies

All direct dependencies are exact-pinned.

## Runtime

### `pi-coding-agent`

- Auth commands and config-value reference/environment-status helpers use only the Rust standard library.
- `serde = 1.0.229` — exposes the typed serialization contract used by RPC JSONL records.
- `serde_json = 1.0.151` — produces compact semantic JSON before the LF record delimiter.

### `pi-agent-core`

No runtime dependencies. Typed filesystem/execution errors, `Skill`/`PromptTemplate`/`AgentHarnessResources` values, `FileKind`/`FileInfo` metadata values, output truncation, and binary-output sanitization use only the Rust standard library.

### `pi-protocol`

No runtime dependencies.

## Development

- `serde = 1.0.229` — derives typed JSON-lines request and response structures for differential tests.
- `serde_json = 1.0.151` — serializes and parses the protocol, RPC JSONL, truncation, harness-error, metadata, resource-value, auth-command, and config-value differential-oracle messages. Public-index type proofs use the pinned TypeScript toolchain without adding a Rust dependency.
