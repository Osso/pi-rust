# Dependencies

All direct dependencies are exact-pinned.

## Runtime

### `pi-coding-agent`

- Auth commands, config-value reference/environment-status helpers, `ParsedSkillBlock`/`parse_skill_block`, the focused direct-module CLI `Mode` value, the focused direct-module `isValidThinkingLevel` predicate, and the tool-output persistence constants use only the Rust standard library beyond the existing exact local `pi-agent-core` type dependency. `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` uses `usize` only as a narrow exception for the exact TypeScript numeric literal used as a byte limit; this does not generalize JavaScript `number` mappings.
- `pi-agent-core = { version = "=0.0.0", path = "../pi-agent-core" }` — supplies the typed `ThinkingLevel` used by the direct-module default constant and the shared `format_size` implementation used by coding-agent's package-visible wrapper. Request `13185` changes no dependency version or lock entry.
- `serde = 1.0.229` — exposes the typed serialization contract used by RPC JSONL records.
- `serde_json = 1.0.151` — produces compact semantic JSON before the LF record delimiter.

### `pi-ai`

- `serde = 1.0.229` — derives the diagnostics wire records and diagnostic-code enum.
- `serde_json = 1.0.151` — carries JSON-shaped thrown values, optional diagnostic details, and streaming-parse results; the `preserve_order` feature keeps TypeScript object insertion order (`indexmap` is its transitive implementation, not a direct dependency).
- `tokio = 1.53.3` (`macros`, `rt`, `sync`, `time`; dev adds `io-util`, `net`, `rt-multi-thread`, `test-util`) — `Notify` for `EventStream`, timers/cancellation for abort, retry, and OAuth polling.
- `reqwest = 0.13.5` (default features: rustls/aws-lc TLS, HTTP/2) — default `Fetch` implementation replacing `globalThis.fetch`.
- `tokio-util = 0.7.20` — `CancellationToken`, the Rust mapping for `AbortSignal`.
- `getrandom = 0.4.3` — fills UUIDv7 and retry-jitter random bytes from the OS.
- `httpdate = 1.0.3` — parses HTTP-date `Retry-After` headers.
- `sha2 = 0.10.9` — computes the PKCE S256 code challenge.

### `pi-agent-core`

No runtime dependencies. The `ToolExecutionMode`, `ThinkingLevel`, and `AgentToolExecutionContext` slices add no runtime or development dependency; `ThinkingLevel` does not depend on the distinct protocol schema. Existing public-index typeproofs use the pinned TypeScript toolchain. Typed filesystem/execution, compaction and branch-summary errors, `QueueMode`, `ToolExecutionMode`, and `ThinkingLevel` enum/literal mappings, `CompactionSettings`/`should_compact`, module-local `SUMMARIZATION_SYSTEM_PROMPT` (not exported from the harness root or package index), module-local `CutPointResult`/`ContextUsageEstimate` (not exported from the harness root or package index), `Skill`/`PromptTemplate`/`PromptTemplateDiagnostic`/`AgentHarnessResources` values, prompt-template argument helpers, Skill system-prompt/invocation formatting, summary marker constants and value-only `BashExecutionMessage`/`BranchSummaryMessage` records, compaction file-operation formatting, image MIME/Base64 helpers, edit line-ending/BOM helpers, `FileKind`/`FileInfo` metadata values, output truncation, and binary-output sanitization use only the Rust standard library.

### `pi-protocol`

No runtime dependencies.

## Development

- `serde = 1.0.229` — derives typed JSON-lines request and response structures for differential tests.
- `serde_json = 1.0.151` — serializes and parses the protocol, RPC JSONL, truncation, harness-error, metadata, resource-value, auth-command, and config-value differential-oracle messages. Public-index type proofs use the pinned TypeScript toolchain without adding a Rust dependency.
