# Dependencies

All direct dependencies are exact-pinned.

## Runtime

### `pi-coding-agent`

- Auth commands, config-value reference/environment-status helpers, `ParsedSkillBlock`/`parse_skill_block`, the focused direct-module CLI `Mode` value, and the focused direct-module `isValidThinkingLevel` predicate use only the Rust standard library beyond the existing exact local `pi-agent-core` type dependency.
- `pi-agent-core = { version = "=0.0.0", path = "../pi-agent-core" }` — supplies the typed `ThinkingLevel` used by the direct-module default constant. The Cargo.lock change adds this dependency only to `pi-coding-agent`; no registry package or unrelated lock entry changes.
- `serde = 1.0.229` — exposes the typed serialization contract used by RPC JSONL records.
- `serde_json = 1.0.151` — produces compact semantic JSON before the LF record delimiter.

### `pi-agent-core`

No runtime dependencies. The `ToolExecutionMode`, `ThinkingLevel`, and `AgentToolExecutionContext` slices add no runtime or development dependency; `ThinkingLevel` does not depend on the distinct protocol schema. Existing public-index typeproofs use the pinned TypeScript toolchain. Typed filesystem/execution, compaction and branch-summary errors, `QueueMode`, `ToolExecutionMode`, and `ThinkingLevel` enum/literal mappings, `CompactionSettings`/`should_compact`, module-local `SUMMARIZATION_SYSTEM_PROMPT` (not exported from the harness root or package index), module-local `CutPointResult`/`ContextUsageEstimate` (not exported from the harness root or package index), `Skill`/`PromptTemplate`/`PromptTemplateDiagnostic`/`AgentHarnessResources` values, prompt-template argument helpers, Skill system-prompt/invocation formatting, summary marker constants and value-only `BashExecutionMessage`/`BranchSummaryMessage` records, compaction file-operation formatting, image MIME/Base64 helpers, edit line-ending/BOM helpers, `FileKind`/`FileInfo` metadata values, output truncation, and binary-output sanitization use only the Rust standard library.

### `pi-protocol`

No runtime dependencies.

## Development

- `serde = 1.0.229` — derives typed JSON-lines request and response structures for differential tests.
- `serde_json = 1.0.151` — serializes and parses the protocol, RPC JSONL, truncation, harness-error, metadata, resource-value, auth-command, and config-value differential-oracle messages. Public-index type proofs use the pinned TypeScript toolchain without adding a Rust dependency.
