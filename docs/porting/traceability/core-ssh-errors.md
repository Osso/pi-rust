# Custom-operation errors and SSH example

Runtime source: `532f7d12cd19235081bd4d6e425bde185652d86f`, read-only
the historical old pin; current source is [source-lock.json](../source-lock.json), proof renewal pending.
Worktree: `/home/osso/.worktrees/pi-rust-core-ssh-errors`; no merge/push.

## Root correction

Node filesystem formatting belongs in **local operations**, not the tool's
custom-operation result boundary. `LocalReadOperations`, `LocalWriteOperations`,
`LocalEditOperations` read/write and `LocalLsOperations::is_directory` now format
local failures. Tool callers convert operation errors to their message only.
No new core public API, error-string stripping, origin flag, dependency, or
alternate execution path was added. Explicitly supplying a local operations
implementation retains its local formatting.

| Pinned tool / operations interface | Source error contract | Native change / proof |
| --- | --- | --- |
| `src/core/tools/read.ts:69-81,164-174,341-349,439-441` | access, optional MIME/stat/range, text/image reads reject unchanged | `core/tools/read.rs`; six injected failure boundaries in `custom_operation_errors`; existing `read_tool`, `node_fs_errors`, `differential_read_tool` |
| `src/core/tools/write.ts:31-40,219-226` | mkdir/write reject unchanged | `core/tools/write.rs`; two injected failures; `edit_write_tools`, `node_fs_errors`, `differential_edit_write_tools` |
| `src/core/tools/edit.ts:85-98,330-354` | access becomes `Could not edit file: <path>. <Error code or Error string>.`; read/write reject unchanged | `core/tools/edit.rs` read/write fixed; access classification unchanged; three injected failures plus existing local ENOENT/EACCES tests and differential |
| `src/core/tools/ls.ts:43-56,150-168,184-191,225-227` | initial stat rejects unchanged; readdir adds `Cannot read directory: `; per-entry stat errors skip entry | `core/tools/ls.rs` initial-stat boundary fixed; injected stat/readdir failures; existing `ls_tool`, `differential_ls_tool`; skip branch retained |
| `src/core/tools/grep.ts:56-66,186-215,378-380` | isDirectory failure becomes `Path not found: <path>`; content-read failure supplies empty lines; other errors reject unchanged | No unconditional Node wrapping found; unchanged. New real-tool/source differential covers failing directory operation |
| `src/core/tools/find.ts:55-66,167-183,355-361` | exists false becomes `Path not found: <path>`; custom operation rejection is not filesystem-wrapped | No wrapping defect; unchanged. New real-tool/source differential covers custom exists false |
| `src/core/tools/bash.ts:101-126,782-804` | arbitrary exec errors propagate; `aborted`/`timeout:` become output-aware tool status errors | No wrapping defect; unchanged. New real-tool/source differential covers arbitrary error and both special statuses |

The original two `custom_operation_errors` tests cover 13 read/write/edit/ls
failure fixtures plus five grep/find/bash fixtures. Six additional tests cover
fallible exists/glob and abort behavior below. All use the actual pinned tool
modules through `oracle/custom-operation-errors-oracle.ts`, not copied expectations.

### Fallible ls/find operations

`LsOperations::exists` and `FindOperations::{exists,glob}` return `io::Result`.
Local exists retains `path_exists` false-on-filesystem-error behavior; the default
placeholder glob still returns an empty list. Bwrap adapters propagate runner
failures rather than converting them to false/empty results, matching pinned
`extensions/bwrap/src/index.ts:226-244`.

| Source branch | Differential behavioral proof (`custom_operation_errors`) |
| --- | --- |
| `ls.ts:150-168,225-227`: exists rejects unchanged, before stat/readdir | `core_ops_ls_exists_rejection_precedes_stat_and_readdir` |
| `find.ts:171-183,355-361`: exists/glob reject unchanged, no filesystem prefix | `core_ops_find_exists_rejection_precedes_glob`, `core_ops_find_glob_rejection_propagates_without_wrapping` |
| `ls.ts:128-133`, `find.ts:143-165,355-358`: abort wins before execution and during pending/rejecting operations | `core_ops_{ls_exists,find_exists,find_glob}_abort_wins_before_rejection_or_while_pending` |

Bwrap unit tests `core_ops_bwrap_{ls_exists,find_exists,find_glob}_preserves_runner_failure`
exercise each real adapter against a missing executable in a private temp directory.
They assert the exact runner failure, not a missing-path or empty-search result.

Fixtures compare the actual pinned modules, including empty, multiline and Unicode
error messages. Cancellation races the operation future, then is checked again
before returning its result so synchronous abort-and-reject cannot win the race.
Proof commands/results: `/tmp/pi-rust-core-ops-fallible/ledger.md` and `NN-*.log`.
RED: four behavioral failures before implementation. Final GREEN: 28 distinct
passing tests across eight integration targets plus three focused bwrap unit tests.
Two aggregate mutation runs rejected changes at all eight reached sites (three
core error conversions, two abort races, three bwrap error conversions), producing
nine expected test failures. Original source bytes were restored after each run.
The existing String boundary excludes direct lone-surrogate inputs; error object
identity/properties remain within [compatibility exclusions](../../specs/compatibility.md#out-of-scope).

## SSH symbol and branch matrix

Native file: `crates/pi-coding-agent/examples/extensions/ssh.rs`.
It is embedded explicitly via `#[path]` and factory registration, not autoloaded.
Its only new public entrypoint is `ssh_extension(&ExtensionApi)`; all operations
use the existing native API.

| TS example symbol / lines | Native item | Behavioral proof |
| --- | --- | --- |
| default factory, flag, four tool spreads `114-182` | `ssh_extension`, `register_tools`, `remote_tools` | exact tool order, labels, descriptions, schemas, flag metadata; pre-session local read; no/empty flag keeps local execution |
| `sshExec` `29-45` | `Remote::run`, `command_for` | fake-only SSH argv/stdin, buffered bytes, exit-7 stderr, missing executable `spawn ssh ENOENT`, failed remote pwd |
| `createRemoteReadOps` `47-62` | `Remote: ReadOperations` | remote path replacement, quoted Unicode path, invalid UTF-8 replacement, supported PNG attachment, unknown MIME/text, failed MIME probe still reads text, remote access error |
| `createRemoteWriteOps` `64-73` | `Remote: WriteOperations` | mkdir before write, JSON-quoted base64 payload, exact file bytes, mkdir and write failures, U+FFFD projection |
| `createRemoteEditOps` `75-79` | `Remote: EditOperations` | remote read/access/write composition; edited contents/diff/patch; source-shaped access error |
| `createRemoteBashOps` `81-112` | `Remote: BashOperations`, `forward`, `wait_for_remote`, `finish_remote_bash`, `terminate` | stdout, stderr, nonzero exit, timeout, abort, already-aborted signal; SIGTERM handler output drained before completion |
| `session_start` `184-200` | `register_session_start`, `fetch_remote_config` | host-only pwd, explicit path, first two colon components, failure before resolution; exact status/notification |
| `user_bash` `203-207` | `register_user_bash` | absent when unresolved; existing operations supplied when resolved; success/exit/timeout/abort/preabort complete outputs |
| `before_agent_start` `210-219` | `register_prompt` | only first cwd occurrence replaced; JS `$&` substitution in host and remote path retained |

`example_ssh::ssh_fixture_remote_matches_pinned_example` loads the shipped module
through the real native factory/runner and executes its actual tool definitions.
Nine scenarios replay the complete transcript against the pinned TS example:
no flag, empty flag, host only, explicit path, extra colon, failed pwd, host `$&`,
path `$&`, and missing SSH executable. Each compares metadata, pre-start result,
UI effects (including exact ANSI status from both real Theme implementations),
start error, modified prompt, 13 tool outcomes, user-bash outcomes,
local/remote edited file contents, and ordered SSH argv plus empty stdin.

The test also feeds the propagated read error through the real interactive
`ToolExecutionComponent`, `pi_tui::TuiRenderer`, and capture terminal. The
terminal must contain the actual remote error, not an EIO wrapper.
No `ExtensionUiComponent`/`ExtensionUiTuiBackend` trait changed.

## Current-pin fixture revalidation

Current `example-ssh-oracle.ts` already uses the actual frozen factory without retired model-runtime imports. Fresh RED `q15-example_ssh-revalidation.log` exposed Python's reentrant buffered-print failure when SIGTERM arrives during readiness output, not an SSH implementation difference. Source/native fixture commands now use os.write/os._exit at the actual process output boundary; complete output remains compared, with no trace stripping or scheduling emulation. Focused current-pin proof pending. Historical runtime-quirk claims below require reclassification under the current AGENTS.md policy.

## SSH timer normalization

Merged in `bfa26e91`: implementation `1a987641`, numeric delay comparison
`ada305f4`, proof `1bfb1e6a`.

`ssh.ts:88-106` multiplies truthy timeout seconds by 1000 and schedules a Node
timer. `wait_for_remote` uses shared `pi_js_compat::node_timers` normalization:
non-finite, below-1ms and above-2147483647ms delays schedule 1ms; valid fractions
are retained. Zero/NaN seconds create no timer, matching JavaScript truthiness.
Original seconds remain in the `timeout:<value>` error, using JS number spelling.

`example_ssh` exercises the real custom `BashOperations` returned by `user_bash`,
including `1e100` and `2147483.648` seconds. Each must reject promptly without
panic, with exact `timeout:1e+100` / `timeout:2147483.648`, and complete results
are compared with the pinned TS example. Fake SSH only; three-second test guard.
Node delay helper contract, proof and limits: [JS compatibility](js-compat.md#node-timers).
RED/GREEN/mutation commands: `/tmp/pi-rust-core-c-fixes/ledger.md`.
C1 RED and production-reversion mutation hit the three-second guard; GREEN
passes the complete nine-scenario differential (1 test). Mutation restoration is
byte-identical to the tested implementation. Required formatting and two-crate
all-target Clippy (`-D warnings`) pass. Manual changed-line Rust/TS readability
review found no new suppression, fallback, or control-flow complexity issue.

## Isolation and source quirks

All binaries/oracles run beneath Cargo's sandbox runner. The SSH fixture child
has a private PATH containing only explicit utility symlinks and its fake SSH;
removing the fixture executable cannot select real SSH. No provider, real remote,
live Pi configuration, or persistent session is used. Separate resets of private
local/remote directories compare side effects, not just successful return codes.

Source quirks are preserved and classified, not repaired:

- JSON string quoting is **not shell escaping**. Remote `$`/backtick expansion
  remains a pinned example defect; enable only with trusted inputs. `$&` tests
  also distinguish JS replacement-string expansion from Rust literal replacement.
- An already-aborted signal does not replay the source's abort listener. The
  command runs and emits output before close rejects `aborted`; the regression
  caught native eager termination and drove its correction.
- The built-in write success message calls UTF-16 code-unit count “bytes”. Its
  12-unit/17-byte Unicode fixture retains the source message and exact file bytes.
- Lone UTF-16 surrogate input is projected to U+FFFD before existing String APIs
  under `compatibility.md`'s approved boundary; this projection is asserted and
  its remote payload/result/file contents are differentially checked.

## Proof ledger

Detailed commands, exact revisions, exit codes, and complete logs:
`/tmp/pi-rust-core-ssh-errors/ledger.md` and `NN-*.log`.

| Proof | Result / logs |
| --- | --- |
| Core behavioral RED | `02-core-red.log`: 1 failure; read/access adds EIO + local path instead of passing remote message |
| Missing example API RED | `05-ssh-api-red.log`: missing shipped module |
| Replacement behavioral RED/GREEN | `11-replacement-red.log` / `12-replacement-green.log`: `$&` prompt mismatch, then pass |
| Pre-abort behavioral RED/GREEN | `13-preabort-red.log` / `15-lifecycle-green.log`: lost pre-abort output, then pass |
| Existing local tests | `04-core-green.log`: 33/0 across custom errors, read, edit/write, ls and real Node filesystem-error targets |
| Expanded core and existing differentials | `30-core-final.log`: 6/0 across two custom-error tests and existing read/edit-write/ls differential targets |
| Complete SSH differential + real TUI | `18-sigterm-green.log`: 1/0, nine scenarios; final `33-final-fixtures.log`: 3/0 including both custom-error tests and exact ANSI SSH status; historical example revision `e31407b1`, before the timer correction above |
| Core mutation | `17-core-mutation.log`: all 11 independently reached changed propagation sites rejected in one aggregate run; exact originals restored |
| SSH mutations | `19`–`29`: 11/11 rejected: flag, path, base64, MIME, SSH error, user-bash, timeout, abort, pre-abort, SIGTERM, prompt; exact originals restored |

Across distinct current targets: 39 tests pass (32 existing local tests, four
existing differential tests, two custom-error tests, one SSH test). Earlier
custom-error runs overlap; do not sum log totals as distinct tests.
Readability metrics/manual review are recorded under `32`/`34`; new SSH/test/oracle
functions stay below cognitive 16 and cyclomatic 21. `execute_edit` retains its
pre-existing cyclomatic 21; only its two error conversions changed.

Initial infrastructure/fixture failures are retained separately (CJS top-level
await, wrong Cargo target names, missing native test imports, removed cwd,
background theme colors, and too-short fixture startup timeout). They are not
counted as behavioral RED evidence. No clippy/check/workspace suite or independent
agent gate was run: the task explicitly excludes them and delegation.

No new dependencies. No new semantic deviation. The ls/find interface limits
were subsequently removed as described above; no remaining SSH port blocker is claimed.
