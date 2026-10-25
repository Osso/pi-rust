# Dependencies

All direct dependencies are exact-pinned.

Shared reason-bearing abort signals replace all direct tokio-util cancellation dependencies; the existing Tokio runtime supplies notification/timers. [Contract and migration status](specs/abort-signals.md).

## Test infrastructure

- `rustix = 1.1.5` (`process`; development dependency of `pi-ai`, `pi-js-compat`, and `pi-tui`) — `process::kill_process_group` tears down the tsx oracle process group in `tests/support/oracle.rs`; oracle children are spawned with `process_group(0)` so a dead test process cannot leave Node/esbuild descendants behind. Crates that already depend on `rustix` at runtime reuse the same pinned release.

## Runtime

- `parking_lot = 0.12.5` (exact; `pi-coding-agent-storage` only) — reentrant retained control-DB connection access matches frozen nested `withControlDb` callbacks. Only `RetainedControlDb` uses `ReentrantMutex`; `RefCell` permits shared recursive connection reads and exclusive close after active callbacks return.

- `pi-coding-agent-ui = 0.0.0` (internal path package) — widgets, selector model search and login options below interactive mode; no bundled-extension dependency. Reuses existing exact external dependencies and features.

- Coding-agent's existing exact `rustix = 1.1.5` enables `net` for safe SO_TYPE/getsockname queries when classifying stderr like Node/libuv. Only stream Unix/TCP sockets are writable streams; datagram/unrecognized/closed descriptors use Node's dummy sink. No package/version or Cargo.lock change (approved2026-10-24).

### `pi-js-compat`

- `nix = 0.30.1` (`user`, exact) — safe effective-UID passwd lookup for POSIX Node `os.homedir()` when HOME is absent. HOME present (including empty) uses the shared Node OS-byte decoder. NSS failure errno discrepancy is an approved gap; no fallback directory is invented.

- `tokio = 1.53.3` (exact; `sync`, `time`, `rt`, `fs`; development adds `macros`, `rt-multi-thread`) — shared Node realpath IO, reason-bearing abort notifications and owner-released timeout tasks; reuses the workspace's existing version. [Contract](specs/abort-signals.md).

- `array_sort` ports the exact pinned V8 TimSort algorithm without a crate dependency. Required for Date/NaN and other inconsistent comparators, whose order and callback trace differ from Rust stable sort. Node v26.11.1 primary-file provenance: [source lock](porting/v8-array-sort-source-lock.json); retained PSF notice and [license](porting/vendor/V8-array-sort-PSF-LICENSE). Changes are documented in the Rust header: dense index workarray/final permutation replace JS object accessors. [Proof](porting/traceability/array-sort.md).

- `node_ignore` ports frozen npm `ignore@7.0.5` defaults over shared `JsString`/`JsRegex`; no additional dependency. Coding-agent package discovery uses this instead of Rust `ignore = 0.4.23` (removed).

- `rustix = 1.1.5` — runtime `io::Errno` names for the centralized Node filesystem/spawn error map, moved unchanged from coding-agent; existing exact dev pin promoted without a new package/version. Runtime `rand` supplies OS entropy for the shared Node `mkdtemp` six-character suffix; no new package or version. The `process` feature remains development-only for oracle teardown.

- `rusqlite = 0.37.0` (optional feature `rusqlite`, enabled by `pi-coding-agent`; `bundled` dev dependency for its test) — `impl ToSql for JsString` binds TEXT exactly like `node:sqlite` (UTF-8, unpaired surrogates become U+FFFD), so control-store SQL encodes raw strings only at the bind boundary. Reuses the coding-agent's existing exact pin.
- `indexmap = 2.14.2` — lossless JSON objects retain UTF-16 key identity, first insertion slots, and last-value-wins duplicate replacement; reuses the already-locked ordered map rather than custom bookkeeping.
- `ryu-js = 1.0.3` (exact `=1.0.3`; Apache-2.0 OR BSL-1.0; MSRV 1.71) — boa-dev ECMAScript shortest binary64 decimal formatter. `Buffer::format` replaces Rust Display plus custom decimal thresholds after same-bit Node RED on the halfway tie `2.9802322387695312e-8`; shared JSON/coercion use that formatter. Published source/API inspected locally; no global serde feature changes.
- `serde = 1.0.229` — `serialize`/`serialize_option` preserve binary64 numeric meaning using representable integer storage or f64, not byte-exact JavaScript decimal text. Development-only `derive` exercises actual `serialize_with` field attributes.
- `serde_json = 1.0.151` (`preserve_order`, `raw_value`) — raw fragments implement lossless UTF-16 serde for shared `JsString`/`JsValue`; `js_number_value` builds numeric-semantic `Value`s (exact i64/u64 integers where representable, f64 otherwise, `null` for non-finite) for extension tool details; `js_json` reproduces V8 `JSON.parse` messages/`JSON.stringify` key order and needs `preserve_order` so object key order survives parsing in a standalone build.
- `jiff = 0.2.38` (`std`, `tz-system`, `tzdb-zoneinfo`) — resolves local offsets and IANA `TZ` names for zone-less `Date.parse` and approved en-US local-date formatting, including DST gaps/folds. Zoneinfo lookup is required in production; without it a named `TZ` may resolve to UTC.
- `icu_collator = 2.3.1` (`compiled_data`) and `icu_locale_core = 2.3.0` — existing UTF-8 `String.prototype.localeCompare` / `Intl.Collator` ordering with the host locale, shared by `pi-coding-agent`, `pi-agent-core`, and `pi-tui`.
- `rust_icu_ucol = 5.8.0` and `rust_icu_ustring = 5.8.0` (exact) — safe ICU4C UTF-16 collation for raw catalog IDs, retaining unpaired surrogate code units. ICU4X's UTF-16 API replaces these units and fails direct Node pair-order differentials. The existing UTF-8 backend is unchanged; there is no backend fallback. Build prerequisites: system ICU4C development headers/libraries, pkg-config, Clang/libclang for bindgen; runtime requires matching ICU4C shared libraries. Development host: ICU4C 78.3. No host package/service changes accompany this slice.
- `regress = 0.12.0` (`utf16`, exact) — ECMAScript regex parser and native UCS-2/non-`u` or UTF-16/`u` execution for the raw minimatch AST and shared validation adapter. Replaces scalar `fancy-regex` in this crate; reuses the existing pi-ai pin. No surrogate encoding or scalar conversion occurs in the matcher. Vendored exact 0.12.0 via the workspace patch at `crates/pi-js-compat/vendor/regress`; `UPSTREAM.json` records crates.io archive SHA-256 `32eef8b209c3c1c15dbad02c1f30f9539f00dc7253e0cbcdaae442a50a09d7c1`, upstream VCS identity, and all retained-file checksums. MIT/Apache licenses retained. Only `src/unicode.rs` and `src/parse.rs` differ: legacy Canonicalize's non-ASCII-to-ASCII guard, its unfold path, and legacy class expansion; Unicode folding delegates unchanged. Purpose: frozen Node distinguishes long-s/Kelvin/dotless-i in non-`u` mode, unlike upstream 0.12.0. Retire vendor/patch when an exact upstream release passes the same literal/class/range/backreference `i`/`iu` oracle corpus. No surrogate encoding/rejection or alternate matching path.

### `pi-coding-agent`

- Internal exact `=0.0.0` path packages `pi-coding-agent-base`, `pi-coding-agent-data`, `pi-coding-agent-settings`, `pi-coding-agent-models`, `pi-coding-agent-storage`, `pi-coding-agent-packages`, `pi-coding-agent-tool-support`, `pi-coding-agent-tools`, `pi-coding-agent-core`, `pi-coding-agent-extensions`, and `pi-coding-agent-interactive` divide rebuild units. Data owns shared leaves; settings and models are independent higher branches; storage owns sessions/control DB; packages owns package management; tool-support owns shared algorithms/theme; tools owns tools/native extension contracts; core owns the session runtime. The facade retains every executable/integration target. Existing external pins/features move with their consumers; no external package or version is added. [Round-2 graph and boundary inventory](porting/crate-split-2.md).

- Self-restart reuses exact `nix = 0.30.1` for explicit-environment POSIX execve and existing `rustix = 1.1.5` for safe stdio fcntl get/set. No feature/version/dependency is added. Node's distinct POSIX errno-name table and FD behavior have [version/hash provenance](porting/node-execve-source-lock.json), retained [MIT notice](porting/vendor/Node-LICENSE), and [process proofs](porting/traceability/self-restart.md).

- Host relay reuses exact pinned `tokio`/`tokio-util`, `getrandom`, `gethostname`, `rustix`, `rusqlite` and `pi-js-compat`; no new crate dependencies. Runtime executables: `ssh` for the frozen stdio transport and Linux `setsid` (util-linux) for Node-compatible detached session launch without forbidden unsafe pre-exec code. Tests replace only `ssh` with a sandbox-private executable fake. [Relay contract](specs/host-relay.md).

- `zstd = 0.14.3` (exact `=0.14.3`) — compresses archived session transcripts and supplies frame decoding for transcript reads, validation, restore and the native seekable-spool worker; completed-frame handling follows the [transcript trace](porting/traceability/session-transcript.md).

- `flate2 = 1.0.34` (exact; already locked transitively) — opt-in Doom WAD helper uses `MultiGzDecoder` for Node `gunzipSync`, including concatenated gzip members.

- Auth commands, config-value reference/environment-status helpers, `ParsedSkillBlock`/`parse_skill_block`, the focused direct-module CLI `Mode` value, the focused direct-module `isValidThinkingLevel` predicate, and the tool-output persistence constants use only the Rust standard library beyond the existing exact local `pi-agent-core` type dependency. `MAX_PERSISTED_TOOL_RESULT_CONTENT_BYTES` uses `usize` only as a narrow exception for the exact TypeScript numeric literal used as a byte limit; this does not generalize JavaScript `number` mappings.
- `pi-js-compat = { version = "=0.0.0", path = "../pi-js-compat" }` — shared JS-semantics helpers (`locale_compare`, minimatch) used by `core::model_resolver` and provider attribution; avoids duplicate collation/glob implementations.
- `pi-agent-core = { version = "=0.0.0", path = "../pi-agent-core" }` — supplies the typed `ThinkingLevel` used by the direct-module default constant and the shared `format_size` implementation used by coding-agent's package-visible wrapper. Request `13185` changes no dependency version or lock entry.
- `serde = 1.0.229` — exposes the typed serialization contract used by RPC JSONL records and `GitSource` wire values (`derive` feature).
- `tempfile = 3.27.0` (exact `=3.27.0`) — creates the private `pi-editor-*` directory for the interactive external editor (`mkdtempSync` equivalent; 0700) and private session-transcript spool directories; spool output is exclusively created with mode 0600.
- `serde_json = 1.0.151` (exact `=1.0.151`; `preserve_order`, `float_roundtrip`, `raw_value`) — `raw_value` emits lossless receipt/unknown-summary JSON tokens without projecting escaped UTF-16 through `Value`; generic serde wrappers remain checked, not lossless. Also produces compact semantic JSON before the LF record delimiter, represents parsed frontmatter values, preserves insertion order for byte-identical `settings.json` output (`js_json::stringify_pretty`), and keeps session-entry key order and decimal floats byte-compatible with TypeScript `JSON.stringify`/`JSON.parse`.
- `indexmap = 2.14.2` — insertion-ordered `IndexMap` for unknown CLI flags and Bash environment properties; the latter retain explicit undefined values and use the existing JS own-key authority for validation order. Reuses the pinned direct dependency, with no version/feature/lock change.
- `regex = 1.13.1` (exact) — editor paste-marker and autocomplete trigger/debounce patterns. The later-only LaTeX renderer is removed in Q2.
- `utils::chalk` needs no dependency: `std::io::IsTerminal` supplies TTY detection, replacing `node:tty`.
- `gethostname = 1.1.0` — supplies the local hostname for OSC 7 terminal current-directory sequences.
- `getrandom = 0.4.3` — fills the `Math.random()` jitter between Supervisor request attempts in `supervisor::client` and the `randomBytes(8)` suffix of `pi-bash-*.log` full-output files in `core::bash_executor`.
- `sha2 = 0.10.9` — computes the detached-job output SHA-256 (`createHash("sha256")`) in `core::detached_job_runner` and the launch-manifest checksum in `core::detached_bash_runner`.
- `serde_yaml_ng = 0.10.0` — parses YAML frontmatter in `utils::frontmatter`.
- `url = 2.5.8` — parses WHATWG URLs in `utils::git` and `file://` paths in `utils::paths`.
- `percent-encoding = 2.3.2` — decodes percent-encoded URL components in `utils::git` and `utils::paths`.
- `tokio = 1.53.3` (`rt`, `macros`, `time`, `sync`, `process`, `io-util`, `io-std`, `fs`) — provides async sleep, cancellation, process I/O for `utils::{sleep, abort, child_process}`, and the stdin/stdout line read behind `main::prompt_confirm`.
- `notify = 8.2.0` — provides the platform file watcher behind `utils::fs_watch`.
- Removed the unused coding-agent `fancy-regex` dependency: session-selector search, syntax highlighting and bwrap grep use shared native UTF-16 `pi-js-compat::js_regex::JsRegex`. The separate pi-tui dependency is unchanged.
- `photon-rs = 0.3.3` (exact, `default-features = false`) — the Rust image library compiled to the `@silvia-odwyer/photon-node` 0.3.4 WASM used by `utils::{photon, exif_orientation, image_convert, image_resize_core}`. Resize (Lanczos3), flips, and `PhotonImage` come from this crate so output bytes match.
- `image = 0.24.9` (exact, `default-features = false`, codec features mirroring photon-rs) — supplies the fallible `load_from_memory`/`DynamicImage::write_to` calls behind `utils::photon`, replacing photon-rs' panicking wrappers; pinned with the `png 0.17.14`, `jpeg-decoder 0.3.1`, `gif 0.13.1`, `tiff 0.9.1`, `flate2 1.0.34`, `miniz_oxide 0.8.0`, `fdeflate 0.3.6`, `weezl 0.1.8` versions embedded in that WASM build (see `docs/porting/traceability/photon.md`).
- `clipboard-rs = 0.3.4` (exact) — the release vendored by `@mariozechner/clipboard` 0.3.9, providing the native X11/Wayland/macOS/Windows clipboard behind `utils::clipboard_native`.
- `rustix = 1.1.5` (`fs`, `process`) — supplies `fs::access(WRITE_OK)` for the install-path writability check (`fs.accessSync` parity), `process::kill_process_group` for `utils::shell::kill_process_tree` (`process.kill(-pid)` parity), `SIGTERM`/`SIGKILL` for `core::exec` termination, and the `kill(pid, 0)` existence check in `core::runtime_process`; the crate is already pinned in the workspace by `pi-tui`.
- `nix = 0.30.1` (`process`, exact) — safe `unistd::execve` for `core::self_restart` (`process.execve` parity) and error-reporting `unistd::close` for output-accumulator spill files. `std::os::unix::process::CommandExt::exec` temporarily replaces the process-global `environ` pointer under a read lock, so a concurrent `std::env::vars()`/`Command::spawn` can read the freed temporary array (observed as corrupted env entries in the `self_restart` suite); `execve` takes an explicit environment and never mutates `environ`.
- `nix = 0.30.1` (`poll`, `process`, `term`; development dependency of `pi-coding-agent`, exact) — `pty::openpty` and `poll::poll` for the process-boundary crash tests, which need a pty that is not the child's controlling terminal (`script(1)` delivers SIGHUP when its master closes, which would not exercise the dead-terminal path).
- `core::http_dispatcher` adds no dependency: it configures the shared `pi-ai` client through `pi_ai::http::fetch_with_options` (`http1_only`, `read_timeout`), keeping one HTTP stack.
- `pi-ai` (path) — `utils::management_http` uses the shared `Fetch` contract and default reqwest client (one HTTP stack).
- `pi-tui` (path) — supplies the TUI keybinding registry (`TUI_KEYBINDINGS`, `KeybindingsManager`) that `core::keybindings` extends with the `app.*` bindings instead of redefining the TUI table.
- HTML export embeds the pinned source assets `marked` 18.0.5 and `highlight.js` 11.9.0 (plus `template.html`/`template.css`/`template.js`) as `include_str!` data; they are vendored source files, not Cargo dependencies. See `docs/porting/traceability/coding-agent-export-html.md` for hashes.
- `tempfile = 3.27.0` (development) — creates isolated temporary files, symlinks, and git repositories for path and process tests.
- Executor spill conformance uses Linux `cc`, `dlsym` and the installed Node/libuv headers for test-only `LD_PRELOAD` write/close fault injection. The fixture invalidates only its spill descriptor and observes real Node v26.11.1 error events; nothing is linked into production and no Rust dependency is added.
- `core::package_manager` uses existing `pi-js-compat` (path, `=0.0.0`) for minimatch, Node ignore/path/error/UTF-16 authorities; `sha2 = 0.10.9` for temporary-directory hashes; and `futures-util = 0.3.34` for bounded-concurrency update checks/git updates. Manifest globs use shared minimatch traversal, not globset. Settings/filter JSON preserves insertion order.
- `node-semver = 7.8.0` is a **ported, not linked** dependency: one private `core::package_manager::npm_semver` module replaces the reduced Rust-semver range parser. It translates only the frozen package-manager's strict/default `valid`, `validRange`, `satisfies`, `maxSatisfying`, and `rcompare` dependencies, using shared JsRegex/JsString/Number/V8-sort authorities; no Node process or new Cargo package. Source tag `v7.8.0`, commit `efa4be6096c1f9b77d9d27d6132f6220c43b4e31`; frozen installation and reference byte-identical. ISC [license](porting/vendor/node-semver-LICENSE), [source/fixture provenance](porting/traceability/package-manager-semver.md).

### `pi-ai`

- `pi-js-compat` (path, `=0.0.0`) — `Number`/`parseInt`/`parseFloat`/`Date.parse` and Base64 (`atob`/`btoa`) helpers shared with the rest of the workspace.
- `indexmap = 2.14.2` (`serde`) — `ProviderEnv`/`ProviderHeaders`, model headers, provider header merges, grammar-tool input properties, and chat-template maps keep TypeScript object insertion order because that order reaches serialized payloads and HTTP request headers.
- `icu_normalizer = 2.3.0` (exact) — NFC normalization of valid Unicode runs in the frozen `idn-hostname` format; unmatched UTF-16 surrogates remain untouched.
- `regress = 0.12.0` (exact; `utf16`) — ECMAScript validation patterns compiled from raw UTF-16 units and matched with Unicode UTF-16 or non-Unicode UCS-2 backends according to frozen flags, including lone surrogates, backreferences and lookbehind.
- `regex = 1.13.1` — case-insensitive (`(?i)`) provider error classification for `utils/overflow.ts` and `utils/retry.ts`, replacing JavaScript `RegExp(..., "i")`.
- `serde = 1.0.229` — derives the diagnostics wire records and diagnostic-code enum.
- `serde_json = 1.0.151` — carries JSON-shaped thrown values, optional diagnostic details, and streaming-parse results; the `preserve_order` feature keeps TypeScript object insertion order (`indexmap` is its transitive implementation) and `float_roundtrip` parses decimal JSON floats with the same correctly-rounded result as JavaScript `JSON.parse`.
- `indexmap = 2.14.2` — direct use for `AiGatewayUniversalRequest.headers`, whose insertion order is observable in the `binding.run()` argument (TypeScript builds it as a plain object).
- `url = 2.5.8` — parses manual OAuth redirect input, validates/normalizes verification URIs, and builds authorize/token query strings with `URLSearchParams`-compatible encoding.
- `tokio = 1.53.3` (`io-util`, `macros`, `net`, `rt`, `sync`, `time`; dev adds `rt-multi-thread`, `test-util`) — `Notify` for `EventStream`, timers/cancellation for abort, retry, and OAuth polling, the loopback TCP listener behind the OAuth callback servers, and TCP/WebSocket transport for Codex.
- `reqwest = 0.13.5` (default features: rustls/aws-lc TLS, HTTP/2) — default `Fetch` implementation replacing `globalThis.fetch`.
- `tokio-tungstenite = 0.30.0` (`rustls-tls-webpki-roots`) — Codex WebSocket transport, including header-carrying handshakes and ping/close control frames.
- `futures-util = 0.3.34` (`sink`) — `SinkExt`/`StreamExt` over the split WebSocket stream.
- `zstd = 0.14.3` — compresses Codex SSE request bodies at the source's level 3.
- `getrandom = 0.4.3` — fills native `crypto.randomUUID()` v4 and retry-jitter bytes from the OS. UUIDv7 belongs to agent-core, not AI.
- `httpdate = 1.0.3` — parses HTTP-date `Retry-After` headers.
- `url = 2.5.8` — parses target and proxy URLs for `utils::node_http_proxy` with WHATWG-compatible host/port/protocol semantics; also parses and joins provider base URLs (`mistral-conversations`) and normalizes AI Gateway request paths (`cloudflare-gateway-binding`).
- `sha2 = 0.10.9` — computes the PKCE S256 code challenge and the SHA-256 payload/canonical-request digests used by Bedrock SigV4.
- `hmac = 0.12.1` — derives the Bedrock SigV4 signing key and signatures (`HMAC-SHA256`).
- `sha1 = 0.10.6` — computes the AWS SSO token-cache filename (SHA-1 of the session name or start URL).
- The Anthropic Messages adapter adds no dependency: its private `@anthropic-ai/sdk` 0.91.1 request path (URL, headers, JSON body, status errors) is built with the existing `serde_json`, `tokio`, and `tokio-util` contracts.
- `rsa = 0.9.10` (`pem`, `sha2`) — RS256-signs the Google service-account JWT assertion for the Vertex ADC token exchange.

### `pi-agent-core`

- `pi-js-compat` (path, `=0.0.0`) — `localeCompare` ordering and `Buffer` Base64 helpers.
- `pi-ai` (path, `=0.0.0`) — supplies the message/model/context/event-stream contracts, JSON streaming parser, and (for the agent loop) tool-argument validation shared with the provider layer.
- `getrandom = 0.4.3` (exact) — OS entropy for frozen `agent-core/harness/session/uuid.ts`; UUIDv7 implementation and tests live in agent-core. [Boundary/gap](porting/traceability/uuid.md).
- `serde = 1.0.229` — derives the agent event, message, and tool-result wire records.
- `serde_json = 1.0.151` (`preserve_order`) — carries JSON-shaped tool arguments/details and preserves TypeScript object insertion order in proxy request bodies.
- `tokio = 1.53.3` (`macros`, `rt`, `sync`, `time`, `fs`, `process`, `io-util`) — runs the agent loop, event stream notifications, scripted-stream tests, and the node execution environment's async filesystem/process I/O.
- `rustix = 1.1.5` (`process`, exact) — kills shell process groups with SIGKILL for timeout, abort, and cleanup without `unsafe` or libc.
- Q9 moves `icu_normalizer = 2.3.0` (exact, `compiled_data`, no default features) from `pi-agent-core` to `pi-coding-agent`: NFC/NFD/NFKC/NFKD normalization belongs to the rehomed fuzzy-edit and read-path primitives. ICU4X scalar runs preserve lone-surrogate boundaries.
- Skill loader ignore matching reuses native `pi-js-compat::node_ignore::Ignore` (npm ignore 7.0.5), preserving UTF16 paths and thrown errors. The former direct `ignore = 0.4.23` dependency is removed from pi-agent-core; coding package-manager use remains separate.
- `indexmap = 2.14.2` (`serde`) — `IndexSet` for `AgentState.pending_tool_calls` and `FileOperations`, whose TypeScript `Set` iteration order is insertion order.
- `serde_yaml_ng = 0.10.0` (exact) — parses skill and prompt-template YAML frontmatter in `harness::frontmatter`.
- `tempfile = 3.27.0` (dev, exact) — creates isolated fixture trees for the node-env, tool, shell-capture, loader, and differential tests, and isolated JSONL session roots.
- Session storage/search adds no runtime dependency: `serde`/`serde_json` (`preserve_order`) carry the durable JSONL records and search projections, `tokio` (`sync`) supplies the state/write mutexes, `tokio-util` supplies search cancellation, and `pi-ai` supplies `BoxFuture`; agent-core owns UUIDv7. JSONL file naming formats ISO timestamps with std only.

The harness events, messages, reducer, and orchestration-type slices add no runtime or development dependency beyond the shared `pi-ai`/`tokio-util`/`serde` contracts. The `ToolExecutionMode`, `ThinkingLevel`, and `AgentToolExecutionContext` slices add no runtime or development dependency; `ThinkingLevel` does not depend on the distinct protocol schema. Existing public-index typeproofs use the pinned TypeScript toolchain. The harness tools/env slice adds `rustix` for process-group kill and `icu_normalizer` for NFC/NFD/NFKC/NFKD, both exact-pinned; `tempfile` is test-only. Typed filesystem/execution, compaction and branch-summary errors, `QueueMode`, `ToolExecutionMode`, and `ThinkingLevel` enum/literal mappings, `CompactionSettings`/`should_compact`, module-local `SUMMARIZATION_SYSTEM_PROMPT` (not exported from the harness root or package index), module-local `CutPointResult`/`ContextUsageEstimate` (not exported from the harness root or package index), `Skill`/`PromptTemplate`/`PromptTemplateDiagnostic`/`AgentHarnessResources` values, prompt-template argument helpers, Skill system-prompt/invocation formatting, summary marker constants and value-only `BashExecutionMessage`/`BranchSummaryMessage` records, compaction file-operation formatting, image MIME/Base64 helpers, edit line-ending/BOM helpers, `FileKind`/`FileInfo` metadata values, output truncation, binary-output sanitization, and the jsdiff line-diff/unified-patch port use only the Rust standard library beyond these dependencies.

### `pi-tui`

- `unicode-segmentation = 1.13.3` (exact) — Unicode 17.0 grapheme cluster segmentation for `utils`, `components::input`, and the editor; matches the Unicode tables used by Node 26 `Intl.Segmenter`.
- `regex = 1.13.1` (exact) — compiles the Unicode-property and whitespace patterns ported verbatim from `latex` (`\p{L}`, `\p{N}`, script spacing, environment row splitting), plus the editor paste-marker and autocomplete trigger/debounce patterns.
- `fancy-regex = 0.16.2` (exact) — executes the marked 18.0.5 grammar ported into `components::markdown::lexer`, which needs lookaround and backreferences that `regex` rejects.
- `serde = 1.0.229` (exact, `derive`) — serializes marked tokens and cursor positions.
- `serde_json = 1.0.151` (exact) — emits the frozen TUI debug-dump JSON fields and terminal output; also drives differential-oracle transport in tests.
- `icu_segmenter = 2.3.0` (exact, default `compiled_data`/`auto`) — dictionary/LSTM word segmentation behind `word_navigation`, chosen as the pure-Rust mapping of `Intl.Segmenter` (decision 2026-10-18; dictionary-word deviations documented in the traceability doc).
- `tokio = 1.53.3` (`process`, `rt`, `sync`, `io-util`, `macros`; dev adds `rt-multi-thread`, `time`) — spawns `fd` asynchronously, drives autocomplete cancellation/select branches, and runs editor autocomplete futures and debounce timers in tests.
- `tempfile = 3.27.0` (development) — creates isolated directory trees and symlinks for autocomplete tests.
- `pi-js-compat` (path, `=0.0.0`) — shared ECMAScript string whitespace, `Number`, `Buffer` Base64, and `localeCompare`; `terminal_image` no longer carries a crate-local Base64 implementation.
- `rustix = 1.1.5` (exact, `termios`/`event` features; `process` as a development feature) — safe termios raw-mode/window-size syscalls and cancellable stdin polling for `ProcessTerminal`, plus `process::kill_process_group` for the shared tsx oracle teardown; `unsafe_code = "forbid"` rules out `libc`/`nix` wrappers that need unsafe call sites.
- `src/unicode_tables.rs` is generated data, not a dependency: Unicode 17.0 emoji sequences, character-property ranges, and `get-east-asian-width` 1.6.0 width ranges (see `docs/porting/traceability/utils.md`).
- The editor adds no dependency: it reuses `unicode-segmentation`, `regex`, `tokio`, and `tokio-util`.

### `pi-agent-core`

- `indexmap = 2.14.2` (`serde`) — `AgentHarnessStreamOptions` headers and the per-request header/metadata patches preserve TypeScript object key order when merged into provider requests.

## Development

- `serde = 1.0.229` — derives typed JSON-lines request and response structures for differential tests.
- `serde_json = 1.0.151` — differential-oracle transport (including `pi-tui`) and typed protocol/RPC/truncation/harness-error/metadata/resource/auth/config-value messages. Public-index type proofs use the pinned TypeScript toolchain without adding a Rust dependency.


Session-name v15 slice adds no dependencies. `pi-js-compat` supplies JS whitespace/trim and UTF-16 slicing with Node SQLite UTF-8 replacement semantics for generated session titles.

`pi-coding-agent`: direct `zstd = "=0.14.3"` (existing lock version), frozen `.jsonl.zst` transcript decoding and archive storage.

`pi-coding-agent` enables existing exact `serde_json` dependency `raw_value` feature only to emit validated lossless JsValue receipt/summary-extra JSON tokens (including escaped UTF16 strings/keys) without projecting through serde Value.

Q18: `crates/pi-js-compat/src/js_json_macros.rs` adapts the token grammar from the already-pinned serde_json 1.0.151 `src/macros.rs` under its MIT license (permission text retained inline). Needed because `serde_json::json!` eagerly projects string expressions into scalar-only Value and cannot build arbitrary UTF-16 provider bodies. No new dependency.

## pi-orchestrator (Q3)

All direct versions exactly match workspace pins. `pi-coding-agent` reuses RPC commands/responses/UI envelopes; `pi-ai` supplies injectable Fetch; `pi-js-compat` supplies JS JSON and date semantics. `serde`/`serde_json` serialize records, `tokio` runs Unix sockets, children and timers, `url` resolves Radius URLs, `gethostname` provides registration metadata, `getrandom` produces IDs/jitter, `indexmap` preserves live-instance order, `rustix` sends child signals. Test-only `tempfile` isolates storage and sockets. Frozen package declares no binary: library-only packaging; native children use sibling `pi --mode rpc` (parent-approved).

`pi-orchestrator` test-only `tokio-util = =0.7.20` names the shared Fetch cancellation-token argument for fake transports.

- `pi-coding-agent` directly uses the already-pinned `jiff =0.2.38` (`std`, `tz-system`, `tzdb-zoneinfo`, no defaults) for frozen codex-usage's America/Chicago reset labels, including DST; no process TZ mutation.
