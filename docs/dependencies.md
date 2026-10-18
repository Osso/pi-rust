# Dependencies

All direct dependencies are exact-pinned.

## Runtime

### `pi-coding-agent`

- Auth commands, config-value reference/environment-status helpers, `ParsedSkillBlock`/`parse_skill_block`, the focused direct-module CLI `Mode` value, the focused direct-module `isValidThinkingLevel` predicate, and the tool-output persistence constants use only the Rust standard library beyond the existing exact local `pi-agent-core` type dependency. `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` uses `usize` only as a narrow exception for the exact TypeScript numeric literal used as a byte limit; this does not generalize JavaScript `number` mappings.
- `pi-agent-core = { version = "=0.0.0", path = "../pi-agent-core" }` — supplies the typed `ThinkingLevel` used by the direct-module default constant and the shared `format_size` implementation used by coding-agent's package-visible wrapper. Request `13185` changes no dependency version or lock entry.
- `serde = 1.0.229` — exposes the typed serialization contract used by RPC JSONL records.
- `serde_json = 1.0.151` — produces compact semantic JSON before the LF record delimiter.

### `pi-agent-core`

No runtime dependencies. The `ToolExecutionMode`, `ThinkingLevel`, and `AgentToolExecutionContext` slices add no runtime or development dependency; `ThinkingLevel` does not depend on the distinct protocol schema. Existing public-index typeproofs use the pinned TypeScript toolchain. Typed filesystem/execution, compaction and branch-summary errors, `QueueMode`, `ToolExecutionMode`, and `ThinkingLevel` enum/literal mappings, `CompactionSettings`/`should_compact`, module-local `SUMMARIZATION_SYSTEM_PROMPT` (not exported from the harness root or package index), module-local `CutPointResult`/`ContextUsageEstimate` (not exported from the harness root or package index), `Skill`/`PromptTemplate`/`PromptTemplateDiagnostic`/`AgentHarnessResources` values, prompt-template argument helpers, Skill system-prompt/invocation formatting, summary marker constants and value-only `BashExecutionMessage`/`BranchSummaryMessage` records, compaction file-operation formatting, image MIME/Base64 helpers, edit line-ending/BOM helpers, `FileKind`/`FileInfo` metadata values, output truncation, and binary-output sanitization use only the Rust standard library.

### `pi-tui`

- `unicode-segmentation = 1.13.3` (exact) — Unicode 17.0 grapheme cluster segmentation for `utils`; matches the Unicode tables used by Node 26 `Intl.Segmenter`.
- No runtime Base64 dependency: `terminal_image` uses the crate-local `src/base64.rs` implementation of Node `Buffer` leniency and standard padded encoding.
- `src/unicode_tables.rs` is generated data, not a dependency: Unicode 17.0 emoji sequences, character-property ranges, and `get-east-asian-width` 1.6.0 width ranges (see `docs/porting/traceability/utils.md`).

### `pi-protocol`

No runtime dependencies.

## Development

- `serde = 1.0.229` — derives typed JSON-lines request and response structures for differential tests.
- `serde_json = 1.0.151` — differential-oracle transport (including `pi-tui`) and typed protocol/RPC/truncation/harness-error/metadata/resource/auth/config-value messages. Public-index type proofs use the pinned TypeScript toolchain without adding a Rust dependency.
