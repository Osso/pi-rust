# Self-restart traceability

Core source: `coding-agent/src/core/self-restart.ts` (303 lines); native: `crates/pi-coding-agent-storage/src/core/self_restart.rs`. Current status: [whole-module audit closure](#whole-module-audit-closure-2026-10-25). The bundled extension inventory below is a separate historical caller proof.

## Historical bundled-extension inventory

Source: `packages/coding-agent/extensions/self-restart/src/index.ts` (32 lines).
Rust: `crates/pi-coding-agent-extensions/src/bundled_extensions/self_restart.rs`.
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

## Whole-module audit closure (2026-10-25)

All 303 common-pin source lines were re-read against the current implementation: env consumption/validation/deletion; parent wait; Args handoff; pending-result first-match/order; truthy notices/PID formatting; exec-versus-spawn selection; Bun argv filtering; ordered environment overlay; target callbacks and spawn/unref/exit behavior. The existing source/runtime/process proofs below cover these realistic branches and actual persistence/OS boundaries. This closes **self-restart.ts only**, not agent-session-runtime or interactive-mode.

Proof reuse is deliberate: current `self_restart.rs` differs from tested `7fd6dd94` only in the relocated test-oracle include. `native_execve.rs` differs only in removal of a needless borrow at `validate_string_argument`; shared validator/error-label changes preserve these static argument labels. Current actual constructor/default-exec integration also passes in `import-runtime-integration.log` at `6cbc0bafa`. No source behavior or missing realistic branch justified another numeric/OS-fault matrix run. Implementation SHA256s at audit: `342bceb07f553debbbfca4545fbe83f7209973f1797ec9baa81fc21a797fb3bb` (`self_restart.rs`), `3bb6f7683aa7550dc8d25d11143c0f070d48ddc6aed1c819b5bd2dbcdc6f0817` (`native_execve.rs`).

Gap: pathological inputs, OS/runtime fault shapes, numeric extremes and Node scheduling oddities follow the project user-observable exclusions; existing native UTF-16/value carriers and already recorded process/environment/bootstrap deviations remain unchanged. The historical stronger tests below remain evidence, not new requirements or new proof matrices.

## Process dependency carrier checkpoint (2026-10-23)

[Contract](../../specs/self-restart-process.md). One `SelfRestartDependencies` replaces `RestartTarget` and its old-only executable override. Default execve is the real POSIX implementation; explicit None selects frozen handoff. Arguments and environment stay JsString/ordered entries until the native callback. Native/raw callback rejections retain constructor/properties/value rather than becoming IO strings.

Actual frozen callback differential10 cases is development GREEN (`self-restart-carrier-green.log`), after behavioral RED (`self-restart-carrier-red.log`): exec return/throw, spawn handoff return/throw, exit throw, each native/raw. Source deliberately forwards only waitForExit:false, not supplied argument overrides.

New no-hook actual-constructor/default-process RED (`self-restart-default-runtime-red-2.log`) exposes another old-only behavior: native runtime default always spawned; frozen execs in place. Native dispatch is now exec-first; acceptance remains pending. Native used PID2460791 instead of original kernel PID2460730; actual source preserved its own PID.

Old-test migration (assertions retained): shell executable-override fixtures now reenter their actual test binary; argv0, restart env, same original PID, wait/no-wait latency and delayed output assertions remain. Failed-exec tests invoke the registered real default execve callback and retain NotFound/concurrent-environment assertions. `rejects_when_the_restarted_child_fails_to_spawn` moves unchanged in purpose/assertion to the private real-spawn backend unit boundary, since a missing executable cannot be selected by the frozen public API. Non-Unicode-env child now uses an actual default spawn and additionally checks Node replacement encoding. No compatibility wrapper or alternate target remains.

Actual constructor-default source/native exec is GREEN (`self-restart-default-runtime-green.log`); native regression22/22 + exec-env3/3 GREEN at eff6072f (`self-restart-carrier-native-regressions.log`).

No-wait default-spawn failure now has actual process RED→GREEN (`self-restart-default-spawn-uncaught-{red,green}.log`): each child runs a private copied executable, removes only that copy's execute bit, and invokes the real default spawn without hooks. Frozen returns0 then emits uncaught EACCES/exit1; native formerly caught the launch failure and exited0. Native now distinguishes deferred launch errors and observes no-wait errors through the shared uncaught authority. Source uncaughtExceptionMonitor/native crash handler assert exact name/message/code/errno/syscall/path/spawnargs/string and process outcome. Neither observer handles/suppresses the error. Node E2BIG throws synchronously, so oversized argv was not misclassified as an asynchronous proof.

Callback matrix expanded to40 restart +56 child cases: scalar, raw UTF-16/NUL, empty argv0/virtual-only argv/zero PID, absent prompt/PID; both native and raw rejections. Serialized wire equality asserts property ordering of the recorded calls/errors, not only object equality. Child cases cover exit17/null0/error, immediate spawn throw, no-wait unref/present/absent/throw, and actual event-vs-return ordering. `self-restart-process-matrix-green.log`:6/6 including actual no-hook successful default source/native spawn (different kernel PID, env/cwd/session, overlay-deleted inherited env) and deferred native failure.

Unobserved child events additionally have isolated actual-frozen-EventEmitter/native-future RED→GREEN (`self-restart-unobserved-raw-{red,green}.log`). Native RangeError remains unchanged. Raw UTF-16/NUL JSON events must become ERR_UNHANDLED_ERROR with original context and Node inspection/stringification, not a directly thrown raw object. Source monitor and native shared crash authority both exit1 after the same no-wait return; exact serialized error/return wires match.

### Native execve boundary

Supplementary [Node v26.11.1 source/hash lock](../node-execve-source-lock.json), with [MIT notice](../vendor/Node-LICENSE). Exact raw-file lines: `src/node_process_methods.cc:518-530` persist-standard-stream helper, `:585-617` clear/partial-restore/exec/full-restore; `src/api/exceptions.cc:59-109` positive errno/code/libc strerror/path/syscall construction; `src/node_errors.cc:750-1076` separate POSIX errno_string switch. Native implementation: `core/self_restart/native_execve.rs`, using existing safe rustix fcntl + nix explicit-environment execve. Oracle runtime is checked against the locked Node version.

`self-restart-native-backend-red.log` proves both missing stdio inheritance and wrong E2BIG exec code. `self-restart-native-backend-green-2.log`:3/3 GREEN covering numeric/insertion/overwrite/raw environment values, six actual default exec/spawn NUL/E2BIG cases, and private actual-process stdio success/all-flag restore/partial restore on closed-stdout fcntl error. No process hooks or installed/frozen executable mutations. Stdio success checks real targets, same kernel PID and physical stdout/stderr output; failure and fcntl cases compare all saved flags exactly.

| Classification | Exact source | Behavior / decision | Proof |
|---|---|---|---|
| DISCREPANCY, preserved literally (parent2026-10-23) | Node v26.11.1 execve error construction: `node_process_methods.cc:616`, `api/exceptions.cc:68-70`, `node_errors.cc:750-1076` | POSIX errno_string omits E2BIG: execve emits errno7, code empty, syscall execve, message `, Argument list too long '<execPath>'`; spawn still uses libuv E2BIG/-7. No generic name fix applied to execve. | Actual frozen module/default syscall comparison, `native_validation_and_os_error_boundary_matches_frozen` |
| GAP, approved runtime oddity (parent2026-10-23) | Replacement bootstrap `src/node.cc:889-893` calls uv_disable_stdio_inheritance | Replacement Node re-marks stdio close-on-exec after inheritance; replacement Rust leaves cleared kernel flags. These image-startup flags are recorded separately, not silently equated. No native bootstrap policy added. | `stdio_cloexec_success_failure_and_fcntl_boundary_matches_frozen`; source true/native false explicitly asserted only after success, physical inheritance/targets/PID/output exact; failures retain exact flag comparisons |

### Callback number domain

Parent approved f64 exit/return carrier fields, 2026-10-23. Actual frozen RED (`self-restart-numeric-red.log`) proves the i32 carrier loses negative zero at an injected exit and incorrectly exits17 for a17.5 default-exit input instead of throwing RangeError. GREEN (`self-restart-numeric-green.log`):30 injected callback/child cases +15 actual default-exit subprocess cases. Cases include -0, fractions, NaN/±Infinity, signed/unsigned32-bit edges, MAX_SAFE_INTEGER/beyond and1e100. Explicit numeric classification flags distinguish non-finite values despite JSON null projection and preserve -0 despite JSON zero projection.

Default exit uses frozen integer then safe-integer validation, shared unchanged `node_error::format_range_number` (moved from abort.rs with parent approval), and signed32-bit wrapping only after validation. Exact source: locked Node `lib/internal/bootstrap/node.js:112-124` exitCode setter and `src/node_process_methods.cc:502-512` ReallyExit/Int32Value. Received-number formatting including `1e+_100` stays literal; default exit errors preserve constructor/code/message/string. Existing zero-value assertions are mechanically typed0.0, not removed or weakened.

### Scoped checkpoint proof

Code7fd6dd94; latest parentf9167bc9 merged via09b3ad27. The merge changes none of self_restart/native_execve/agent_session_runtime/abort/node_error, so scoped behavioral and clippy evidence remains valid. `self-restart-final-native.log`: runtime44/44 plus the explicitly parent-invoked ignored cwd child, legacy22/22, exec-env3/3, process16/16. After test-only lint fixes, process16/16 again in `self-restart-final-process-current.log`; backend unit1/1, cold-cwd real-consumer1/1, lossless notice1/1 and moved abort timeout formatter1/1 are `self-restart-final-{backend-unit,cwd-consumer,lossless-notice,abort-helper}.log`.

Strict two-oracle tsgo/Biome/fmt PASS; manual Rust/TS readability audit extracted validation/command/observer/stream stages and retained explicit test fixtures. Metrics CLI unavailable; no package installed. `self-restart-final-clippy-current{.jsonl,-owned.json}`: exit0,0 changed-hunk diagnostics,1693 unrelated warnings; no whole-workspace lint-clean claim. `self-restart-checkpoint-workspace.{jsonl,log}`: workspace/all-targets exit0,0 errors/0 warnings. `self-restart-checkpoint-inventory.log`: PASS686 rows/186679 lines, statuses399 done/19 gap/88 partial/180 todo. Detailed command/revision ledger: `/tmp/pi-rust-repin-lossless-interactive/self-restart-proof-ledger.md`.

This closes the bounded process-dependency slice, not whole-runtime/interactive acceptance.

## Current capability coverage

| Frozen statements/features | Native/proof | Remaining proof |
|---|---|---|
| consume: marker/env deletion, empty session, Number/safe-integer/own-PID check | real source13 cases +2 overflow cases; retained behavioral tests | no whole invalid-JS-type claim beyond typed domain |
| wait/default probe: absent/zero/current/exited PID; deadline/poll | real source4 branch cases + patched process.kill OK/EPERM/ESRCH; existing clock/sleep behavior tests | unusual clock ranges remain typed Rust API domain |
| apply: selected session, model overrides, resume/continue/fork/noSession/sessionId/fileArgs/messages | source handoff/no-handoff differential + retained startup assertions | no old-only assertions removed |
| pending results + notices | direct DB-backed resolver and actual persisted restart separately differential; actual CLI resolves before notice; reopen same/spawn/zero/Unicode/empty/absent prompt | wider pending-batch reductions reuse agent-core helper; no exhaustive syscall failures claimed |
| exec/spawn/environment/argv | one carrier;40+56 callback cases,30 number cases,15 actual default-exit processes; real constructor/default exec and default spawn, raw/native unobserved events, ordered UTF-16 env, NUL/OS fields, stdio preservation/restore/fcntl and retained regressions | approved replacement-bootstrap FD flag gap; historical table is not whole-runtime acceptance |

At98953083 (parent8b15a5c7), `self_restart`21/21, `self_restart_state`4/4, focused default-kill classifier and actual CLI handoff all GREEN (`4d-final-{self-restart,state,kill,cli}.log`). [Shared checkpoint gates](agent-session-runtime.md#bounded4d-checkpoint): workspace0/0, scoped changed-hunk clippy0, TS/Biome/fmt. That historical checkpoint did not close the whole module; the later whole-module audit above closes self-restart only. Runtime remains separate.

## Historical core self-restart inventory (`packages/coding-agent/src/core/self-restart.ts`)

Rust: `crates/pi-coding-agent-storage/src/core/self_restart.rs`.
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
