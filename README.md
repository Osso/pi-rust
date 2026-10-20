# pi-rust

Systematic Rust port of the Pi Agent Harness.

The TypeScript source oracle is pinned in [`docs/porting/source-lock.json`](docs/porting/source-lock.json). Each slice preserves representable observable behavior through source traceability, characterization tests, and differential execution against the real TypeScript implementation.

Current coverage: `@earendil-works/pi-protocol` binary framing, incremental decoding, and dependency-ready typed schemas; coding-agent RPC JSONL framing, auth commands, config-value reference/configured-status helpers, `ParsedSkillBlock`/`parse_skill_block`, focused direct-module CLI `Mode` and `isValidThinkingLevel` values, direct-module `DEFAULT_THINKING_LEVEL`, package-exported tool-output persistence constants, package-exported `CURRENT_SESSION_VERSION`, package-visible `SourceInfo`/`createSyntheticSourceInfo` metadata construction, and coding-agent truncation constants through the direct module (only the two defaults through tools/package indexes), plus package-exported `formatSize` through direct-module, tools-index, and package-root Rust paths; coding-agent `TruncationOptions`/`TruncationResult` and truncation functions remain excluded; agent-core typed filesystem/execution, compaction and branch-summary errors, module-local `CompactionDetails`, `QueueMode`, `ToolExecutionMode`, and `ThinkingLevel` enum/literal mappings, `AgentToolExecutionContext`, `BeforeToolCallResult`, direct-module `Edit`, `AppliedEditsResult`, and `FuzzyMatchResult` records, and harness-root exports (scheduling/execution remain out of scope), `CompactionSettings`/`should_compact`, module-local `SUMMARIZATION_SYSTEM_PROMPT`, and module-local `CutPointResult`/`ContextUsageEstimate` value boundaries (not exported from the harness root or package index; context-estimation producers deferred), focused `Skill`/`PromptTemplate`/`PromptTemplateDiagnostic`/`AgentHarnessResources` values, prompt-template argument parsing/substitution/content formatting, Skill system-prompt/invocation formatting, summary marker constants, value-only `BashExecutionMessage`/`BranchSummaryMessage` contracts, compaction file-operation formatting, focused `FileKind`/`FileInfo` metadata values, image MIME/Base64 helpers, edit line-ending/BOM helpers, UTF-8/line output truncation, and binary-output sanitization.

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

## Test-process isolation

Linux tests require `bubblewrap`, `iproute2`, `util-linux` (`setpriv`), Python 3,
and permitted unprivileged
network/mount namespaces. Missing/unsupported isolation fails closed; no unsandboxed
fallback. `.cargo/config.toml` routes compiled test binaries through
`scripts/cargo-runner.sh` and `scripts/test-sandbox.py`, including the Cargo phases
of `run-tests.sh`. Ordinary `cargo run` is deliberately unchanged.

For a bounded CLI startup proof, use the same boundary explicitly:

```text
python3 scripts/test-sandbox.py -- /absolute/path/to/pi
python3 scripts/test-isolation-test.py /absolute/path/to/built/pi
```

The launcher masks `/home/osso/.cargo/bin/claude-memory` with a child-only,
read-only controlled helper. `helper.jsonl` records actual argv/PIDs; the normal
shutdown hook still logs its start and the stub's stdout. Product hook and stats
semantics are unchanged. Host helper files are never overwritten. Each test gets
private HOME/XDG/Pi state and an allowlisted environment; homes, runtime sockets,
and host `/tmp` are hidden. Root, checkout, pinned references/Git metadata and the
optional installed Pyrun executable/venv/Python runtime are read-only. Only private
state and an inherited private `TMPDIR` are writable. The latter survives nested
guards so parent fixture paths and sockets remain valid. Set `PI_TEST_SANDBOX_DIR`
to a dedicated proof directory to retain state/logs there; otherwise a fresh
`pi-test-*` directory is retained under TMPDIR. Remove retained directories after
collecting proof.

Every protected process gets new user/network namespaces with loopback enabled
but no external interface/route. NET_ADMIN is retained only within those child
namespaces to bring up their loopback, then dropped before executing tests;
host interfaces are unchanged. Local HTTP/oracle fixtures, PTY descriptors, process
groups and parent watchdogs remain usable. The bounded regression uses an outer
namespace with a forbidden fake at the absolute helper path, real default Pi
startup/double-Ctrl+C shutdown, and real loopback HTTP across nested guards. It
never uses the installed helper, even against the old runner.

**Boundary:** this is test automation, not a general hostile-code sandbox. Other
absolute executables are not individually masked; system files outside hidden
homes/runtime/temp remain readable. Direct test-binary, direct Pi/TS invocation,
and the standalone Node/typecheck phases of `run-tests.sh` do not enter the Cargo
runner; explicitly use `test-sandbox.py` when they may start default Pi. No claim
of whole-host-tool isolation or full-suite compatibility follows from this bounded
regression. Fixture services must be started inside the protected process, not on
host loopback. Build/reference preparation stays outside this runtime guard.

Oracle gate boundary, regression commands, and pinned-source blockers:
[oracle gate](docs/porting/oracle-gate.md).

The agent-harness [ThinkingLevel](docs/specs/thinking-level.md) is distinct from the protocol schema; the protocol value remains capped at `max`.

Status: pre-alpha. No complete Pi-compatible executable exists yet.
