# pi-rust

Systematic Rust port of the Pi Agent Harness.

The TypeScript source oracle is pinned in [`docs/porting/source-lock.json`](docs/porting/source-lock.json). Each slice preserves representable observable behavior through source traceability, characterization tests, and differential execution against the real TypeScript implementation.

Current coverage: `@earendil-works/pi-protocol` binary framing, incremental decoding, and dependency-ready typed schemas; coding-agent RPC JSONL framing, auth commands, and config-value reference/configured-status helpers; agent-core typed filesystem/execution, compaction, and branch-summary errors, focused `Skill`/`PromptTemplate`/`AgentHarnessResources` values and prompt-template argument parsing/substitution, focused `FileKind`/`FileInfo` metadata values, UTF-8/line output truncation, and binary-output sanitization.

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

Status: pre-alpha. No complete Pi-compatible executable exists yet.
