# coding-agent find tool traceability

Frozen `packages/coding-agent/src/core/tools/find.ts` at immutable `4f9e6f8a4`; [source lock](../source-lock.json). Rust: `core/tools/find.rs`. [Contract](../../specs/find-tool.md).

## Symbol and branch map

| Frozen symbols / branches | Native representation | Behavioral proof |
|---|---|---|
| `findSchema`, `FindToolInput`, `DEFAULT_LIMIT`, `FindToolDetails` | Schema/raw `JsValue` inputs/details, binary64 limit/default1000 | `differential_find_numbers`:17 custom cases; actual FD numeric argv cases; preserved finite/nonfinite spelling and wire null projection |
| `FindOperations`, `defaultFindOperations` placeholder, `FindToolOptions` | Owned `FindOperations`/optional provider; explicit custom glob capability | Exact exists/glob/ignore/pattern/root/limit traces, missing/throw/empty cases; default branch uses real executable fixtures |
| `toPosixPath`, custom prefix slice / `path.relative` | POSIX literal strings, shared Node path authority, fallible helper | `differential_find_paths`:8 frozen paths, actual process cwd aligned; raw surrogate slice and literal backslashes. Deleted cwd becomes uv_cwd error, never panic, in failure matrix |
| `createFindToolDefinition` metadata/name/label/approval/description/snippet/parameters | Native definition and contribution/schema authority | Retained common-pin execution/index/wrapper/renderer tests; raw callback behavior below |
| Execute preabort, settle first-winner, listener lifetime, retained producer | Existing shared [observer](../../specs/tool-promise-observer.md), existing source-position inner checks | `differential_find_abort`:5 gated success/rejection/preabort histories; no later glob after aborted exists, but active operations finish |
| Custom exists false/reject, glob reject/empty | IO error carrier retains message/UTF-16 until tool boundary |11-case `differential_find_failures`: missing, ASCII/raw-unit errors, empty-at-zero, deleted ambient cwd; exact operation traces |
| Custom result mapping, count >= original limit, no extra result slice; byte/entry notices/details/order | Shared prefix/path/truncation plus JS number spelling |17 numeric cases; single oversized result and650-result combined byte/entry caps in failure matrix |
| `ensureTool("fd", true)`, missing fd | Whole [PATH-only tools manager](tools-manager.md); no config/HOME/install/download path | Frozen missing-PATH case returns exact error; version-success/exec-disappearance fixture proves no alternate retry/fallback |
| Git ancestor walk, optional --no-require-git, --max-results, --full-path/pattern rewrite, -- separator | Real fd argv with raw `JsString` until native validation/encoding |42 actual process cases; retained gitignore/nested-root/flag-pattern real-fd differentials. Old-only Windows pattern rewrite removed |
| Spawn error event / synchronous argument throw | Shared Node validator before encoding; native spawn formatter after syscall | Raw NUL+surrogate args[7] diagnostic; actual version-success/exec-ENOENT race in failure matrix |
| stdout readline, stderr per-data conversion, child close, nonzero/no-output vs nonzero/output, JS trimming | Existing two readers share [LineBuffer](../../specs/node-readline.md); EOF-driven capture, concurrent process wait, per-read stderr repair |42-case `differential_find_process`: numeric/pattern argv, CR/LF/CRLF, BOM/U+0085, malformed stdout/stderr, quiet errors, nonzero output, null exit code, inherited late descendant pipe output; full argv/results/errors |
| FD relativity/trailing slash or backslash; empty normalized output; first-line/byte/entry combined truncation | Shared prefix/path logic plus FD-only suffix, shared head truncation | Actual process relative/prefix/backslash/blank-line cases; oversized line and650-line FD byte/entry notices |
| `stopChild`, abort after stdio EOF, close-time aborted check | One SIGTERM through the child's process-lifetime wait; drain/reap existing readers | `differential_find_child_abort`:2 real handler-marker cases with stdio open/closed. Third held-git-IO history proves later child starts and exits naturally after observer abort |
| `formatFindCall`, `formatFindResult`, render callbacks | Exact text/numeric truthiness/hints; existing strict Text setter shared with ls; fallible native HandlerError |19 frozen frames: reuse, invalid/empty/raw paths, zero/NaN/infinities/fractions, collapsed/expanded, images, combined warnings, wrong Container TypeErrors and subsequent state reuse |
| `createFindTool` | Shared result-cap wrapper | Retained common-pin wrapped execution/index proof; no new cap or alternate path |

## Source discrepancies intentionally preserved

- Prefix matching is not component-aware: `/repo/listx/alpha` under `/repo/list` becomes `/alpha`. Slicing `searchPath.length + 1` UTF-16 units can split a following surrogate pair. Relative custom results resolve against process cwd, not the search root. Eight actual frozen cases prove these application quirks; native does not silently correct them.
- Abort before `stopChild` exists removes the listener but does not prevent a later spawn after git IO. Frozen fs.access gate and native current-thread/held-blocking-pool IO boundary prove observer-abort → child-start → natural-exit without SIGTERM. No production hook or timer-order guess.

## Explicit exclusions and gaps

| Status | Case | Classification |
|---|---|---|
| gap | Windows path.sep/win32 paths and signal behavior | Approved Windows-platform gap2026-10-22; exact current cycle covers POSIX. Old-only pattern rewriting is deleted rather than retained as compatibility. |
| gap | Native byte-read partition versus Node data-event chunks; implicit readline elapsed-time CRLF window | Approved native-runtime/timer-interleaving policy2026-10-23. Stderr still decodes each native chunk as frozen does each event; no Node chunk-size/timer emulation. Byte-level CR/LF/CRLF/UTF-8 semantics stay shared/exact. |
| shared runtime policy | Node process.cwd cache oddities / implicit spawnSync version maxBuffer | [Cwd authority](process-cwd.md) / [tools-manager gap](tools-manager.md); do not duplicate emulation here. |
| excluded | Invalid JavaScript runtime input types and malformed required typed metadata | Approved typed native API domain. Raw UTF-16 strings and valid binary64 numbers are not excluded. Incompatible prior UI components remain valid context values and are explicitly tested as errors. |
| approved ownership | Mutation through JS returned references | Owned values per2026-10-22 approval; built-in Text component identity reuse explicitly preserved. |

## RED → GREEN proof ledger

Artifacts: `/tmp/pi-rust-repin-core-tools/`; `ledger.md` records commands/revisions and invalidation scope.

- Numeric: `find-numbers-red.log` → `find-numbers-green-1.log` (17 frozen custom cases +ordinary4).
- Paths: `find-paths-red.log` → `find-paths-green.log` (8 frozen cases, numeric17, ordinary4, lossless-session18;24 tests GREEN).
- Process/readline: `find-process-red-2.log` / `find-process-red-3.log` → `find-process-green-2.log`; expanded42-case final scope GREEN in `find-process-final-42.log`. Shared file/FIFO3 and bytechunk2 GREEN; shared dependency clippy passes; workspace all-target0/0 at1de72606.
- Abort: `find-abort-red.log` → `find-abort-green.log`; `find-child-abort-red-2.log` → `find-child-abort-green.log`. Late-git ordering GREEN in `find-child-late-ordering-2.log`.
- Rendering: `find-render-red.log` → `find-render-green.log` (find16+LS16+raw-path12);19-frame error-branding follow-up GREEN in `find-render-branding-green-2.log`.

Remaining11-case failure matrix GREEN in `find-failures-first-2.log` and `find-failures-final-green.log`; actual missing/spawn failures, raw messages, empty results, deleted cwd and combined caps covered. No remaining module branch gap beyond classified exclusions above. Final current-revision package check/own clippy gate follows the closure commit; global coding-agent lint debt remains parent-owned and is not claimed passing.
