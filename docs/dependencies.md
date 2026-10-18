# Dependencies

All direct dependencies are exact-pinned.

## Runtime

### `pi-coding-agent`

- Auth commands, config-value reference/environment-status helpers, `ParsedSkillBlock`/`parse_skill_block`, the focused direct-module CLI `Mode` value, the focused direct-module `isValidThinkingLevel` predicate, and the tool-output persistence constants use only the Rust standard library beyond the existing exact local `pi-agent-core` type dependency. `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` uses `usize` only as a narrow exception for the exact TypeScript numeric literal used as a byte limit; this does not generalize JavaScript `number` mappings.
- `pi-agent-core = { version = "=0.0.0", path = "../pi-agent-core" }` — supplies the typed `ThinkingLevel` used by the direct-module default constant and the shared `format_size` implementation used by coding-agent's package-visible wrapper. Request `13185` changes no dependency version or lock entry.
- `serde = 1.0.229` — exposes the typed serialization contract used by RPC JSONL records and `GitSource` wire values (`derive` feature).
- `serde_json = 1.0.151` (`preserve_order`) — produces compact semantic JSON before the LF record delimiter, represents parsed frontmatter values, and preserves insertion order for byte-identical `settings.json` output (`js_json::stringify_pretty`).
- `regex = 1.13.1` — compiles the pinned ANSI compatibility pattern in `utils::ansi` and the three pinned `supports-color` patterns in `utils::chalk`.
- `utils::chalk` needs no dependency: `std::io::IsTerminal` supplies TTY detection, replacing `node:tty`.
- `gethostname = 1.1.0` — supplies the local hostname for OSC 7 terminal current-directory sequences.
- `serde_yaml_ng = 0.10.0` — parses YAML frontmatter in `utils::frontmatter`.
- `url = 2.5.8` — parses WHATWG URLs in `utils::git` and `file://` paths in `utils::paths`.
- `percent-encoding = 2.3.2` — decodes percent-encoded URL components in `utils::git` and `utils::paths`.
- `tokio = 1.53.3` — provides async sleep, cancellation, and process I/O for `utils::{sleep, abort, child_process}`.
- `tokio-util = 0.7.20` — supplies `CancellationToken` as the Rust `AbortSignal` mapping.
- `notify = 8.2.0` — provides the platform file watcher behind `utils::fs_watch`.
- `fancy-regex = 0.16.2` — compiles the translated highlight.js grammar patterns (lookaround, backreferences) in `utils::syntax_highlight`.
- `photon-rs = 0.3.3` (exact, `default-features = false`) — the Rust image library compiled to the `@silvia-odwyer/photon-node` 0.3.4 WASM used by `utils::{photon, exif_orientation, image_convert, image_resize_core}`. Resize (Lanczos3), flips, and `PhotonImage` come from this crate so output bytes match.
- `image = 0.24.9` (exact, `default-features = false`, codec features mirroring photon-rs) — supplies the fallible `load_from_memory`/`DynamicImage::write_to` calls behind `utils::photon`, replacing photon-rs' panicking wrappers; pinned with the `png 0.17.14`, `jpeg-decoder 0.3.1`, `gif 0.13.1`, `tiff 0.9.1`, `flate2 1.0.34`, `miniz_oxide 0.8.0`, `fdeflate 0.3.6`, `weezl 0.1.8` versions embedded in that WASM build (see `docs/porting/traceability/photon.md`).
- `clipboard-rs = 0.3.4` (exact) — the release vendored by `@mariozechner/clipboard` 0.3.9, providing the native X11/Wayland/macOS/Windows clipboard behind `utils::clipboard_native`.
- `httpdate = 1.0.3` — parses the remote catalog's `Last-Modified` header into the millisecond timestamp stored in `models-store.json` (Node `Date.parse`).
- `rustix = 1.1.5` (`fs`, `process`) — supplies `fs::access(WRITE_OK)` for the install-path writability check (`fs.accessSync` parity), `process::kill_process_group` for `utils::shell::kill_process_tree` (`process.kill(-pid)` parity), `SIGTERM`/`SIGKILL` for `core::exec` termination, and the `kill(pid, 0)` existence check in `core::runtime_process`; the crate is already pinned in the workspace by `pi-tui`.
- `jiff = 0.2.38` (`std`, `tz-system`) — resolves the system UTC offset for offset-less timestamps in `utils::iso_time`, replacing V8's local-time `Date.parse` resolution.
- `icu_collator = 2.3.1` (`compiled_data`) and `icu_locale_core = 2.3.0` — reproduce `String.prototype.localeCompare` ordering for `core::detached_job_retention` tie-breaks, using the host locale like Node's ICU collation.
- `core::http_dispatcher` adds no dependency: it configures the shared `pi-ai` client through `pi_ai::http::fetch_with_options` (`http1_only`, `read_timeout`), keeping one HTTP stack.
- `pi-ai` (path) — `utils::management_http` uses the shared `Fetch` contract and default reqwest client (one HTTP stack).
- `pi-tui` (path) — supplies the TUI keybinding registry (`TUI_KEYBINDINGS`, `KeybindingsManager`) that `core::keybindings` extends with the `app.*` bindings instead of redefining the TUI table.
- `tempfile = 3.27.0` (development) — creates isolated temporary files, symlinks, and git repositories for path and process tests.

### `pi-ai`

- `regex = 1.13.1` — case-insensitive (`(?i)`) provider error classification for `utils/overflow.ts` and `utils/retry.ts`, replacing JavaScript `RegExp(..., "i")`.
- `serde = 1.0.229` — derives the diagnostics wire records and diagnostic-code enum.
- `serde_json = 1.0.151` — carries JSON-shaped thrown values, optional diagnostic details, and streaming-parse results; the `preserve_order` feature keeps TypeScript object insertion order (`indexmap` is its transitive implementation, not a direct dependency) and `float_roundtrip` parses decimal JSON floats with the same correctly-rounded result as JavaScript `JSON.parse`.
- `url = 2.5.8` — parses manual OAuth redirect input, validates/normalizes verification URIs, and builds authorize/token query strings with `URLSearchParams`-compatible encoding.
- `tokio = 1.53.3` (`io-util`, `macros`, `net`, `rt`, `sync`, `time`; dev adds `rt-multi-thread`, `test-util`) — `Notify` for `EventStream`, timers/cancellation for abort, retry, and OAuth polling, the loopback TCP listener behind the OAuth callback servers, and TCP/WebSocket transport for Codex.
- `reqwest = 0.13.5` (default features: rustls/aws-lc TLS, HTTP/2) — default `Fetch` implementation replacing `globalThis.fetch`.
- `tokio-util = 0.7.20` (`rt`) — `CancellationToken`, the Rust mapping for `AbortSignal`.
- `tokio-tungstenite = 0.30.0` (`rustls-tls-webpki-roots`) — Codex WebSocket transport, including header-carrying handshakes and ping/close control frames.
- `futures-util = 0.3.34` (`sink`) — `SinkExt`/`StreamExt` over the split WebSocket stream.
- `zstd = 0.14.3` — compresses Codex SSE request bodies at the source's level 3.
- `getrandom = 0.4.3` — fills UUIDv7 and retry-jitter random bytes from the OS.
- `httpdate = 1.0.3` — parses HTTP-date `Retry-After` headers.
- `url = 2.5.8` — parses target and proxy URLs for `utils::node_http_proxy` with WHATWG-compatible host/port/protocol semantics; also parses and joins provider base URLs (`mistral-conversations`) and normalizes AI Gateway request paths (`cloudflare-gateway-binding`).
- `sha2 = 0.10.9` — computes the PKCE S256 code challenge and the SHA-256 payload/canonical-request digests used by Bedrock SigV4.
- `hmac = 0.12.1` — derives the Bedrock SigV4 signing key and signatures (`HMAC-SHA256`).
- `sha1 = 0.10.6` — computes the AWS SSO token-cache filename (SHA-1 of the session name or start URL).
- The Anthropic Messages adapter adds no dependency: its private `@anthropic-ai/sdk` 0.91.1 request path (URL, headers, JSON body, status errors) is built with the existing `serde_json`, `tokio`, and `tokio-util` contracts.
- `rsa = 0.9.10` (`pem`, `sha2`) — RS256-signs the Google service-account JWT assertion for the Vertex ADC token exchange.

### `pi-agent-core`

- `pi-ai` (path, `=0.0.0`) — supplies the message/model/context/event-stream contracts, JSON streaming parser, and (for the agent loop) tool-argument validation shared with the provider layer.
- `serde = 1.0.229` — derives the agent event, message, and tool-result wire records.
- `serde_json = 1.0.151` (`preserve_order`) — carries JSON-shaped tool arguments/details and preserves TypeScript object insertion order in proxy request bodies.
- `tokio = 1.53.3` (`macros`, `rt`, `sync`, `time`, `fs`, `process`, `io-util`) — runs the agent loop, event stream notifications, scripted-stream tests, and the node execution environment's async filesystem/process I/O.
- `tokio-util = 0.7.20` (`rt`) — `CancellationToken` as the `AbortSignal` mapping.
- `rustix = 1.1.5` (`process`, exact) — kills shell process groups with SIGKILL for timeout, abort, and cleanup without `unsafe` or libc.
- `icu_normalizer = 2.3.0` (exact, `compiled_data`, no default features) — the workspace's single `String.prototype.normalize` implementation for NFC/NFD/NFKC/NFKD, used by fuzzy edit matching and read-path variants; ICU4X data matches Node's ICU family.
- `ignore = 0.4.23` (exact) — gitignore pattern matching (`matched`, last-match-wins, negation) behind the skill loader's `.gitignore`/`.ignore`/`.fdignore` handling, replacing the npm `ignore` package.
- `serde_yaml_ng = 0.10.0` (exact) — parses skill and prompt-template YAML frontmatter in `harness::frontmatter`.
- `pi-telemetry = { path = "../pi-telemetry", version = "=0.0.0" }` — supplies the `TelemetryContext`/`TelemetrySpan` runtime contract and the schema-definition types used by `harness::telemetry` and `AgentHarnessOptions.context`.
- `tempfile = 3.27.0` (dev, exact) — creates isolated fixture trees for the node-env, tool, shell-capture, loader, and differential tests, and isolated JSONL session roots.
- Session storage/search adds no runtime dependency: `serde`/`serde_json` (`preserve_order`) carry the durable JSONL records and search projections, `tokio` (`sync`) supplies the state/write mutexes, `tokio-util` supplies search cancellation, and `pi-ai` supplies `BoxFuture`, `js_number`, and UUIDv7. JSONL file naming formats ISO timestamps with std only.

The harness events, messages, reducer, and orchestration-type slices add no runtime or development dependency beyond the shared `pi-ai`/`tokio-util`/`serde` contracts. The `ToolExecutionMode`, `ThinkingLevel`, and `AgentToolExecutionContext` slices add no runtime or development dependency; `ThinkingLevel` does not depend on the distinct protocol schema. Existing public-index typeproofs use the pinned TypeScript toolchain. The harness tools/env slice adds `rustix` for process-group kill and `icu_normalizer` for NFC/NFD/NFKC/NFKD, both exact-pinned; `tempfile` is test-only. Typed filesystem/execution, compaction and branch-summary errors, `QueueMode`, `ToolExecutionMode`, and `ThinkingLevel` enum/literal mappings, `CompactionSettings`/`should_compact`, module-local `SUMMARIZATION_SYSTEM_PROMPT` (not exported from the harness root or package index), module-local `CutPointResult`/`ContextUsageEstimate` (not exported from the harness root or package index), `Skill`/`PromptTemplate`/`PromptTemplateDiagnostic`/`AgentHarnessResources` values, prompt-template argument helpers, Skill system-prompt/invocation formatting, summary marker constants and value-only `BashExecutionMessage`/`BranchSummaryMessage` records, compaction file-operation formatting, image MIME/Base64 helpers, edit line-ending/BOM helpers, `FileKind`/`FileInfo` metadata values, output truncation, binary-output sanitization, and the jsdiff line-diff/unified-patch port use only the Rust standard library beyond these dependencies.

### `pi-session-sqlite`

- `pi-agent-core` (path, `=0.0.0`) — reuses the ported `SessionStorage`/`SessionSearch` contracts, `Session` facade, `FileSystem` execution env, `SessionError`, and UUIDv7 id generator; no session type is redefined.
- `pi-ai` (path, `=0.0.0`) — `BoxFuture`, `js_number` float formatting for durable payloads, `uuidv7` for writer-lease owners and branch-cache ids, and the message/usage wire types.
- `rusqlite = 0.37.0` (exact, `bundled`) — the SQLite driver. `bundled` compiles SQLite from source so the schema, PRAGMAs (`WAL`, `synchronous=FULL`, `busy_timeout=5000`), FTS5 virtual table with the `trigram` tokenizer, and `bm25()` ranking are available without depending on the host SQLite build; `node:sqlite` is the TypeScript equivalent.
- `serde = 1.0.229` / `serde_json = 1.0.151` (`preserve_order`) — durable entry/record payloads, session metadata, and error-shaped values with TypeScript key order.
- `tokio = 1.53.3` (`macros`, `rt`, `sync`, `time`) — repository operation queue (`tokio::sync::Mutex`), writer-lease heartbeat task, and lease timers.
- `tokio-util = 0.7.20` (`rt`) — `CancellationToken` carried by `SessionSearchOptions`.
- `tempfile = 3.27.0` (development) — isolated database directories for behavior and cross-implementation tests.

The SQLite slice also relies on the pinned TypeScript toolchain (`tsx`) through `oracle/session-sqlite-oracle.ts`; that is test-only and adds no Rust dependency.

### `pi-tui`

- `unicode-segmentation = 1.13.3` (exact) — Unicode 17.0 grapheme cluster segmentation for `utils`, `components::input`, and the editor; matches the Unicode tables used by Node 26 `Intl.Segmenter`.
- `regex = 1.13.1` (exact) — compiles the Unicode-property and whitespace patterns ported verbatim from `latex` (`\p{L}`, `\p{N}`, script spacing, environment row splitting), plus the editor paste-marker and autocomplete trigger/debounce patterns.
- `fancy-regex = 0.16.2` (exact) — executes the marked 18.0.5 grammar ported into `components::markdown::lexer`, which needs lookaround and backreferences that `regex` rejects.
- `serde = 1.0.229` (exact, `derive`) — serializes the ported marked token model for the markdown differential oracle; the model is also the component's internal token representation.
- `icu_segmenter = 2.3.0` (exact, default `compiled_data`/`auto`) — dictionary/LSTM word segmentation behind `word_navigation`, chosen as the pure-Rust mapping of `Intl.Segmenter` (decision 2026-10-18; dictionary-word deviations documented in the traceability doc).
- `tokio = 1.53.3` (`process`, `rt`, `sync`, `io-util`, `macros`; dev adds `rt-multi-thread`, `time`) — spawns `fd` asynchronously, drives autocomplete cancellation/select branches, and runs editor autocomplete futures and debounce timers in tests.
- `tokio-util = 0.7.20` (`rt`) — `CancellationToken` maps the `AbortSignal` passed to autocomplete suggestions and kills an in-flight `fd` child; the editor aborts superseded requests and `CancellableLoader` maps its abort signal.
- `tempfile = 3.27.0` (development) — creates isolated directory trees and symlinks for autocomplete tests.
- No runtime Base64 dependency: `terminal_image` uses the crate-local `src/base64.rs` implementation of Node `Buffer` leniency and standard padded encoding.
- `httpdate = 1.0.3` — parses the remote catalog's `Last-Modified` header into the millisecond timestamp stored in `models-store.json` (Node `Date.parse`).
- `rustix = 1.1.5` (exact, `termios` feature) — safe termios raw-mode and window-size syscalls for `ProcessTerminal`; `unsafe_code = "forbid"` rules out `libc`/`nix` wrappers that need unsafe call sites.
- `src/unicode_tables.rs` is generated data, not a dependency: Unicode 17.0 emoji sequences, character-property ranges, and `get-east-asian-width` 1.6.0 width ranges (see `docs/porting/traceability/utils.md`).
- The editor adds no dependency: it reuses `unicode-segmentation`, `regex`, `tokio`, and `tokio-util`.

### `pi-protocol`

No runtime dependencies. CBOR encoding and decoding are hand-written against the pinned TypeScript subset so encoded bytes match the source byte-for-byte; a general-purpose CBOR crate would not guarantee that parity.

### `pi-server`

- `pi-protocol` (path, `=0.0.0`) — typed protocol messages, CBOR codec, framing, and schema validation are reused unchanged.
- `pi-ai` (path, `=0.0.0`) — bridge input types (`Model`, messages, usage, tool calls) and the shared `uuidv7` generator for server, connection, and server-assigned session IDs.
- `serde_json = 1.0.151` (`preserve_order`) — carries `pi-ai` tool arguments/details and JSON-shaped error details without reordering object keys.
- `tokio = 1.53.3` (`io-util`, `macros`, `net`, `rt`, `sync`, `time`) — async server tasks, Unix-domain sockets, write serialization, timers, and handshake cancellation.
- `sha2 = 0.10.9` — derives the private Unix bind path from the SHA-256 prefix, matching the TypeScript transport.
- No `tokio-util` dependency: the server package contains no `AbortSignal`, so there is no `CancellationToken` mapping to perform.
- Development: `tempfile = 3.27.0` for isolated socket paths; `tokio` adds `rt-multi-thread` and `test-util` for integration tests and the cross-implementation client run.

### `pi-telemetry`

- `indexmap = 2.14.2` (`serde`) — `TelemetryMap`, the insertion-ordered map behind schema span/attribute/event maps and `SpanAttributes`, matching TypeScript object key order (already in the lock via `serde_json` `preserve_order`).
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
