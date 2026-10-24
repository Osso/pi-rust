# self-restart traceability

Source: `packages/coding-agent/extensions/self-restart/src/index.ts` (32 lines).
Rust: `crates/pi-coding-agent/src/bundled_extensions/self_restart.rs`.
Tests: `crates/pi-coding-agent/tests/bundled_extensions_commands.rs` (`self_restart_matches_typescript`).

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `RESTART_NOTICE` | `RESTART_NOTICE` | differential restart options |
| `/restart` command | `register_command` handler | differential restart call |
| `restart_self` tool metadata/schema (`Type.Object({})`) | `ToolDefinition` | registration differential |
| promptGuidelines (2 lines) | literal | registration differential |
| `execute` restart + result text/details | tool execute | differential tool result |

No gaps.

## Common-pin4d re-port (2026-10-23)

In progress; historical table below is not whole-module acceptance. The existing18/20 self_restart regression was a test-only representation assumption: imported SessionEntry records intentionally remain Unknown SSOT, and consumers must use as_custom_message()/known(). Test helper now uses that projection; every existing customType/content/display assertion is unchanged. No session_manager production edit or discarded record.

Actual pinned appendSelfRestartNotice→JSONL→SessionManager.open differential adds same-PID/spawn-PID, Unicode, empty and absent prompts. Genuine RED (`restart-notice-differential-red.log`): native wrote `" PID: ..."` for Some("") while frozen wrote no notice. Native now follows frozen truthy-prompt guard. `restart-notice-differential-green.log`:21/21 GREEN atb7f876bd. No tests removed.

Frozen :97-160 consume/apply differential RED (`restart-state-red.log`) proves missing empty-session rejection and provider/model/thinking clearing. Native now follows those source branches. Frozen :166-203 resolver differential via actual native runtime restart (`restart-pending-red.log`) proves native emitted no pending results; source emits restart_self success, first matching detached job notification, ordinary interrupted error in call order. Native resolver now reads the actual control DB, appends frozen results and runs before notice/replacement. State13 consume +2 handoff branches GREEN (`restart-state-green.log`). Like-boundary direct resolver and actual frozen restartUnlocked/real native persisted replacement GREEN (`restart-pending-like-boundaries-green.log`). Timestamp assertions use each process's measured call interval, not equality of independent wall clocks.

DISCREPANCY (preserved literally; parent classification2026-10-23): frozen runtime:451-462 reopens SessionManager before calling resolver; frozen session-manager:2242-2252 constructs that manager without metadataControlDbPath. It therefore emits ordinary interruption rather than the detached-job result. Native preserves that exact behavior. Direct resolver with the original DB-bound manager still emits first matching detached job. The initial `restart-pending-green.log` failure compared direct source resolver to persisted native runtime (unlike boundaries); corrected oracle now invokes actual frozen restart for the persisted case. Neither assertion is loosened: detached result and persisted interruption are separately asserted.

Frozen main.ts:1086-1087 now has its native startup consumer: a consumed handoff resolves pending calls before appending notice/name validation. Actual isolated TS cli.ts/native binary RED (`restart-cli-handoff-red-2.log`) shows source writes two tool results before notice; native wrote only notice. Fixture uses real canonical session serialization, offline private state and empty --name to exit before any provider can run. Initial manually named fixture was invalid for the real startup path and replaced before RED classification. GREEN at036330fe (`restart-final-cli-green.log`).

Frozen defaultIsProcessAlive:74-81 catches all signal0 errors, including EPERM. Pure native syscall-result classifier is backed by actual frozen wait helper with patched process.kill for OK/EPERM/ESRCH; no live host PID probes. Genuine RED (`restart-kill-classification-red.log`): native treated EPERM as alive; source returned false. Native now maps only Ok to alive. GREEN at166e83c4 (`restart-kill-green.log`).

Explicit zero oldPid is falsy in frozen wait/notice paths. Actual source differentials reproduce native's extra now/alive calls and incorrect `PID 0 -> current` text (`restart-zero-{wait,notice}-red.log`); native now skips wait and uses current-PID notice like frozen. GREEN at036330fe (`restart-final-{notice,wait}-green.log`); actual CLI handoff is GREEN in `restart-final-cli-green.log`.

Consume:118-123 compares parsed Number against supplied current/parent PIDs before integer conversion. Actual source PID-override differential RED (`restart-pid-overflow-red.log`) exposed native saturating4294967296 to4294967295 and treating it as its own restart; native now compares binary64 values first. GREEN at0ce42f49 (`restart-pid-overflow-green.log`).

## Process dependency carrier (in progress, 2026-10-23)

[Contract](../../specs/self-restart-process.md). One `SelfRestartDependencies` replaces `RestartTarget` and its old-only executable override. Default execve is the real POSIX implementation; explicit None selects frozen handoff. Arguments and environment stay JsString/ordered entries until the native callback. Native/raw callback rejections retain constructor/properties/value rather than becoming IO strings.

Actual frozen callback differential10 cases is development GREEN (`self-restart-carrier-green.log`), after behavioral RED (`self-restart-carrier-red.log`): exec return/throw, spawn handoff return/throw, exit throw, each native/raw. Source deliberately forwards only waitForExit:false, not supplied argument overrides.

New no-hook actual-constructor/default-process RED (`self-restart-default-runtime-red-2.log`) exposes another old-only behavior: native runtime default always spawned; frozen execs in place. Native dispatch is now exec-first; acceptance remains pending. Native used PID2460791 instead of original kernel PID2460730; actual source preserved its own PID.

Old-test migration (assertions retained): shell executable-override fixtures now reenter their actual test binary; argv0, restart env, same original PID, wait/no-wait latency and delayed output assertions remain. Failed-exec tests invoke the registered real default execve callback and retain NotFound/concurrent-environment assertions. `rejects_when_the_restarted_child_fails_to_spawn` moves unchanged in purpose/assertion to the private real-spawn backend unit boundary, since a missing executable cannot be selected by the frozen public API. Non-Unicode-env child now uses an actual default spawn and additionally checks Node replacement encoding. No compatibility wrapper or alternate target remains.

Actual constructor-default source/native exec is GREEN (`self-restart-default-runtime-green.log`); native regression22/22 + exec-env3/3 GREEN at eff6072f (`self-restart-carrier-native-regressions.log`).

No-wait default-spawn failure now has actual process RED→GREEN (`self-restart-default-spawn-uncaught-{red,green}.log`): each child runs a private copied executable, removes only that copy's execute bit, and invokes the real default spawn without hooks. Frozen returns0 then emits uncaught EACCES/exit1; native formerly caught the launch failure and exited0. Native now distinguishes deferred launch errors and observes no-wait errors through the shared uncaught authority. Source uncaughtExceptionMonitor/native crash handler assert exact name/message/code/errno/syscall/path/spawnargs/string and process outcome. Neither observer handles/suppresses the error. Node E2BIG throws synchronously, so oversized argv was not misclassified as an asynchronous proof.

Child event/unref matrix, raw UTF-16 fixtures, successful actual default spawn and final scoped gates remain open; this paragraph is not whole-module acceptance.

## Current capability coverage

| Frozen statements/features | Native/proof | Remaining proof |
|---|---|---|
| consume: marker/env deletion, empty session, Number/safe-integer/own-PID check | real source13 cases +2 overflow cases; retained behavioral tests | no whole invalid-JS-type claim beyond typed domain |
| wait/default probe: absent/zero/current/exited PID; deadline/poll | real source4 branch cases + patched process.kill OK/EPERM/ESRCH; existing clock/sleep behavior tests | unusual clock ranges remain typed Rust API domain |
| apply: selected session, model overrides, resume/continue/fork/noSession/sessionId/fileArgs/messages | source handoff/no-handoff differential + retained startup assertions | no old-only assertions removed |
| pending results + notices | direct DB-backed resolver and actual persisted restart separately differential; actual CLI resolves before notice; reopen same/spawn/zero/Unicode/empty/absent prompt | wider pending-batch reductions reuse agent-core helper; no exhaustive syscall failures claimed |
| exec/spawn/environment/argv | retained real exec same-PID child, spawn/wait/no-wait/error and non-Unicode-env tests | whole frozen injectable dependency/error-carrier audit still open; historical table below is not new-pin full acceptance |

At98953083 (parent8b15a5c7), `self_restart`21/21, `self_restart_state`4/4, focused default-kill classifier and actual CLI handoff all GREEN (`4d-final-{self-restart,state,kill,cli}.log`). [Shared checkpoint gates](agent-session-runtime.md#bounded4d-checkpoint): workspace0/0, scoped changed-hunk clippy0, TS/Biome/fmt. No whole self-restart/runtime acceptance yet; remaining callback/dependency audit is work, not an approved deviation.

## Historical core self-restart inventory (`packages/coding-agent/src/core/self-restart.ts`)

Rust: `crates/pi-coding-agent/src/core/self_restart.rs`.
Tests: `crates/pi-coding-agent/tests/self_restart.rs`, `self_restart_exec_env.rs`.

| Source symbol / branch | Rust item | Proof |
| --- | --- | --- |
| `consumeSelfRestartRequest` (marker, session, prompt, old-pid validation, deletion) | `consume_self_restart_request` | 6 behavioral tests |
| `waitForSelfRestartParentExit` (pid/current, 5 s deadline, 25 ms poll) | `wait_for_self_restart_parent_exit` | injected clock/alive/sleep tests |
| `applySelfRestartRequest` | `apply_self_restart_request` | args-override tests |
| `appendSelfRestartNotice` / `formatRestartNotice` | `append_self_restart_notice` | notice tests, exec and spawn PID variants |
| `restartCurrentProcess` → `execve` (pid/foreground group preserved) | `restart_current_process` → `exec_self_restart` → `nix::unistd::execve` | real `/bin/sh` exec child test; failed-exec error-kind test |
| `spawnSelfRestart` fallback (cwd, env, stdio inherit, optional wait/unref) | `spawn_self_restart` | spawn/wait and immediate-return tests |
| `buildSelfRestartEnv` over `process.env` | `build_self_restart_env` over `pi_js_compat::process_env::vars()` | env tests; non-UTF-8 lossy test in `process_env`; non-Unicode restart child test |
| `getSelfRestartArgs` / Bun virtual entrypoint filter | `get_self_restart_args` | argv tests |

Deviation: the TS source calls `process.execve` directly. Rust previously used
`CommandExt::exec`, which swaps the process-global `environ` under a read lock and races with
concurrent environment readers (root cause and proof in
[test-infrastructure.md](test-infrastructure.md#6-self_restart-suite-corruption-flake-root-cause-commandext-exec)).
`nix::unistd::execve` gives the same `execve` semantics (same PID, no PATH search) without touching
`environ`. Non-UTF-8 environment entries follow Node's lossy decoding via `process_env::vars()`.
