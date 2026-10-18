# Dependencies

All direct dependencies are exact-pinned.

## Runtime

### `pi-coding-agent`

- Auth commands, config-value reference/environment-status helpers, `ParsedSkillBlock`/`parse_skill_block`, the focused direct-module CLI `Mode` value, the focused direct-module `isValidThinkingLevel` predicate, and the tool-output persistence constants use only the Rust standard library beyond the existing exact local `pi-agent-core` type dependency. `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` uses `usize` only as a narrow exception for the exact TypeScript numeric literal used as a byte limit; this does not generalize JavaScript `number` mappings.
- `pi-agent-core = { version = "=0.0.0", path = "../pi-agent-core" }` — supplies the typed `ThinkingLevel` used by the direct-module default constant and the shared `format_size` implementation used by coding-agent's package-visible wrapper. Request `13185` changes no dependency version or lock entry.
- `serde = 1.0.229` — exposes the typed serialization contract used by RPC JSONL records and `GitSource` wire values (`derive` feature).
- `serde_json = 1.0.151` — produces compact semantic JSON before the LF record delimiter and represents parsed frontmatter values.
- `regex = 1.13.1` — compiles the pinned ANSI compatibility pattern in `utils::ansi` and the three pinned `supports-color` patterns in `utils::chalk`.
- `utils::chalk` needs no dependency: `std::io::IsTerminal` supplies TTY detection, replacing `node:tty`.
- `gethostname = 1.1.0` — supplies the local hostname for OSC 7 terminal current-directory sequences.
- `serde_yaml_ng = 0.10.0` — parses YAML frontmatter in `utils::frontmatter`.
- `url = 2.5.8` — parses WHATWG URLs in `utils::git` and `file://` paths in `utils::paths`.
- `percent-encoding = 2.3.2` — decodes percent-encoded URL components in `utils::git` and `utils::paths`.
- `tokio = 1.53.3` — provides async sleep, cancellation, and process I/O for `utils::{sleep, abort, child_process}`.
- `tokio-util = 0.7.20` — supplies `CancellationToken` as the Rust `AbortSignal` mapping.
- `notify = 8.2.0` — provides the platform file watcher behind `utils::fs_watch`.
- `pi-ai` (path) — `utils::management_http` uses the shared `Fetch` contract and default reqwest client (one HTTP stack).
- `tempfile = 3.27.0` (development) — creates isolated temporary files, symlinks, and git repositories for path and process tests.

### `pi-ai`

- `regex = 1.13.1` — case-insensitive (`(?i)`) provider error classification for `utils/overflow.ts` and `utils/retry.ts`, replacing JavaScript `RegExp(..., "i")`.
- `serde = 1.0.229` — derives the diagnostics wire records and diagnostic-code enum.
- `serde_json = 1.0.151` — carries JSON-shaped thrown values, optional diagnostic details, and streaming-parse results; the `preserve_order` feature keeps TypeScript object insertion order (`indexmap` is its transitive implementation, not a direct dependency) and `float_roundtrip` parses decimal JSON floats with the same correctly-rounded result as JavaScript `JSON.parse`.
- `tokio = 1.53.3` (`macros`, `rt`, `sync`, `time`; dev adds `io-util`, `net`, `rt-multi-thread`, `test-util`) — `Notify` for `EventStream`, timers/cancellation for abort, retry, and OAuth polling.
- `reqwest = 0.13.5` (default features: rustls/aws-lc TLS, HTTP/2) — default `Fetch` implementation replacing `globalThis.fetch`.
- `tokio-util = 0.7.20` (`rt`) — `CancellationToken`, the Rust mapping for `AbortSignal`.
- `getrandom = 0.4.3` — fills UUIDv7 and retry-jitter random bytes from the OS.
- `httpdate = 1.0.3` — parses HTTP-date `Retry-After` headers.
- `sha2 = 0.10.9` — computes the PKCE S256 code challenge.

### `pi-agent-core`

- `pi-ai` (path, `=0.0.0`) — supplies the message/model/context/event-stream contracts, JSON streaming parser, and (for the agent loop) tool-argument validation shared with the provider layer.
- `serde = 1.0.229` — derives the agent event, message, and tool-result wire records.
- `serde_json = 1.0.151` (`preserve_order`) — carries JSON-shaped tool arguments/details and preserves TypeScript object insertion order in proxy request bodies.
- `tokio = 1.53.3` (`macros`, `rt`, `sync`, `time`) — runs the agent loop, event stream notifications, and scripted-stream tests.
- `tokio-util = 0.7.20` (`rt`) — `CancellationToken` as the `AbortSignal` mapping.

The `ToolExecutionMode`, `ThinkingLevel`, and `AgentToolExecutionContext` slices add no runtime or development dependency; `ThinkingLevel` does not depend on the distinct protocol schema. Existing public-index typeproofs use the pinned TypeScript toolchain. Typed filesystem/execution, compaction and branch-summary errors, `QueueMode`, `ToolExecutionMode`, and `ThinkingLevel` enum/literal mappings, `CompactionSettings`/`should_compact`, module-local `SUMMARIZATION_SYSTEM_PROMPT` (not exported from the harness root or package index), module-local `CutPointResult`/`ContextUsageEstimate` (not exported from the harness root or package index), `Skill`/`PromptTemplate`/`PromptTemplateDiagnostic`/`AgentHarnessResources` values, prompt-template argument helpers, Skill system-prompt/invocation formatting, summary marker constants and value-only `BashExecutionMessage`/`BranchSummaryMessage` records, compaction file-operation formatting, image MIME/Base64 helpers, edit line-ending/BOM helpers, `FileKind`/`FileInfo` metadata values, output truncation, and binary-output sanitization use only the Rust standard library.

### `pi-tui`

- `unicode-segmentation = 1.13.3` (exact) — Unicode 17.0 grapheme cluster segmentation for `utils`; matches the Unicode tables used by Node 26 `Intl.Segmenter`.
- `regex = 1.13.1` (exact) — compiles the Unicode-property and whitespace patterns ported verbatim from `latex` (`\p{L}`, `\p{N}`, script spacing, environment row splitting).
- `icu_segmenter = 2.3.0` (exact, default `compiled_data`/`auto`) — dictionary/LSTM word segmentation behind `word_navigation`, chosen as the pure-Rust mapping of `Intl.Segmenter` (decision 2026-10-18; dictionary-word deviations documented in the traceability doc).
- `tokio = 1.53.3` (`process`, `rt`, `sync`, `io-util`; dev adds `macros`, `rt`, `rt-multi-thread`) — spawns `fd` asynchronously and waits for its output in `autocomplete`.
- `tokio-util = 0.7.20` (`rt`) — `CancellationToken` maps the `AbortSignal` passed to autocomplete suggestions and kills an in-flight `fd` child.
- `tempfile = 3.27.0` (development) — creates isolated directory trees and symlinks for autocomplete tests.
- No runtime Base64 dependency: `terminal_image` uses the crate-local `src/base64.rs` implementation of Node `Buffer` leniency and standard padded encoding.
- `rustix = 1.1.5` (exact, `termios` feature) — safe termios raw-mode and window-size syscalls for `ProcessTerminal`; `unsafe_code = "forbid"` rules out `libc`/`nix` wrappers that need unsafe call sites.
- `src/unicode_tables.rs` is generated data, not a dependency: Unicode 17.0 emoji sequences, character-property ranges, and `get-east-asian-width` 1.6.0 width ranges (see `docs/porting/traceability/utils.md`).

### `pi-protocol`

No runtime dependencies. CBOR encoding and decoding are hand-written against the pinned TypeScript subset so encoded bytes match the source byte-for-byte; a general-purpose CBOR crate would not guarantee that parity.

### `pi-telemetry`

- `serde = 1.0.229` — derives the camelCase wire shape for schema definitions and recorded spans/events, including the internally tagged status and parent definitions.

### `pi-client`

- `pi-protocol = { version = "=0.0.0", path = "../pi-protocol" }` — framing, CBOR message codecs, typed commands/results, snapshots, and protocol validation errors.
- `tokio = 1.53.3` (`io-util`, `net`, `rt`, `sync`) — one-shot request/handshake resolvers, spawned transport/read/write tasks, `UnixStream`, and async I/O. Dev targets add `macros` and `rt-multi-thread` for `#[tokio::test]`.
- `tokio-util = 0.7.20` — `CancellationToken` stops the Unix read loop on local close. The source package has no `AbortSignal`; the dependency is not an `AbortSignal` mapping.
- Dev-only `serde_json = 1.0.151` — compares Rust client frames, state transitions, and snapshots with the TypeScript differential oracle.

## Development

- `futures = 0.3.31` — provides `block_on` and a oneshot channel for the async telemetry behavioral tests.
- `serde_json = 1.0.151` — serializes recorded spans and schema definitions for the TypeScript differential oracle and attribute wire-shape tests.

- `serde = 1.0.229` — derives typed JSON-lines request and response structures for differential tests.
- `serde_json = 1.0.151` — differential-oracle transport (including `pi-tui`) and typed protocol/RPC/truncation/harness-error/metadata/resource/auth/config-value messages. Public-index type proofs use the pinned TypeScript toolchain without adding a Rust dependency.
