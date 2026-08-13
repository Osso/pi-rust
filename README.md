# pi-rust

Systematic Rust port of the Pi Agent Harness.

The TypeScript source oracle is pinned in [`docs/porting/source-lock.json`](docs/porting/source-lock.json). Each slice preserves representable observable behavior through source traceability, characterization tests, and differential execution against the real TypeScript implementation.

Current coverage: `@earendil-works/pi-protocol` binary framing, incremental decoding, and dependency-ready typed schemas; coding-agent RPC JSONL framing, auth commands, config-value reference/configured-status helpers, `ParsedSkillBlock`/`parse_skill_block`, and the direct-module `DEFAULT_THINKING_LEVEL` value (the pinned package index omits `defaults.ts`); agent-core typed filesystem/execution, compaction and branch-summary errors, module-local `CompactionDetails`, `QueueMode`, `ToolExecutionMode`, and `ThinkingLevel` enum/literal mappings and harness-root export (scheduling/execution remain out of scope), `CompactionSettings`/`should_compact`, module-local `SUMMARIZATION_SYSTEM_PROMPT`, and module-local `CutPointResult`/`ContextUsageEstimate` value boundaries (not exported from the harness root or package index; context-estimation producers deferred), focused `Skill`/`PromptTemplate`/`PromptTemplateDiagnostic`/`AgentHarnessResources` values, prompt-template argument parsing/substitution/content formatting, Skill system-prompt/invocation formatting, summary marker constants, value-only `BashExecutionMessage`/`BranchSummaryMessage` contracts, compaction file-operation formatting, focused `FileKind`/`FileInfo` metadata values, image MIME/Base64 helpers, edit line-ending/BOM helpers, UTF-8/line output truncation, and binary-output sanitization.

## Commands

```text
./run-tests.sh focused
./run-tests.sh differential
./run-tests.sh oracle
./run-tests.sh all
cargo fmt --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The agent-harness [ThinkingLevel](docs/specs/thinking-level.md) is distinct from the protocol schema; the protocol value remains capped at `max`.

Status: pre-alpha. No complete Pi-compatible executable exists yet.
