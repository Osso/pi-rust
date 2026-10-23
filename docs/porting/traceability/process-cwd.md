# Node process cwd/chdir

[Contract](../../specs/process-cwd.md). Node26.11.1 built-in `lib/internal/bootstrap/switches/does_own_process_state.js`, locked by [version, hash and lines](../node-process-cwd-source-lock.json). Extracted from the installed reference with `process.binding("natives")`, not inferred from OS behavior.

Native: `crates/pi-js-compat/src/process_cwd.rs`. Direct OS reads, no cache, mutex or extra dependency. [User decision 2026-10-23](../../../AGENTS.md) excludes Node/V8/libuv runtime oddities, explicitly including process.cwd caching; frozen TS logic and user-visible error records remain in scope.

| Reference branch | Native representation | Proof |
|---|---|---|
| wrappedCwd :140–143 caches successful rawMethods.cwd | **approved gap:** direct OS read; warmed deleted directory rejects rather than returning a stale path | updated direct-read behavioral RED→GREEN; three cached-source cases explicitly assert native cold ENOENT/uv_cwd metadata |
| wrappedChdir :126–130 validates string, calls raw method, then clears cache | typed string API, direct native chdir; cache mechanism intentionally absent | existing 26-operation Node/native sequence retains successful changes, failed native error fields and subsequent direct reads |
| native cwd byte ingress | shared decode_node_path; shared uv_cwd error record | exact UTF-16 values and name/message/code/errno/syscall/path/dest comparison |
| native chdir UTF-8 C-string input | encode_node_utf8, truncate at first NUL before OS call | existing/missing NUL prefix and lone-surrogate destinations; native returned cwd is the decoded actual path |
| chdir failure with existing old cwd | old physical cwd and destination retained in native error | ENOENT, ENOTDIR and empty destination full-record differential |
| failed chdir with deleted old physical cwd | deterministic empty old path | **approved gap 2026-10-23**, [project rule](../../../AGENTS.md); discrepancy below; other error fields remain exact and subsequent native cwd reads reject |
| caller propagation | canonical getter; wrappers retain native source | actual cold SDK/FileSettingsStorage/startup themes/executable/restart/Architect/bwrap/generator calls all match the reference ENOENT/uv_cwd record |
| lazy Node path.resolve for absolute Architect start | no process cwd read for absolute input | actual frozen readProjectSpec succeeds with a cold deleted process cwd; native eager-read RED then GREEN |

## Workspace consumer inventory

Production wiring and narrow error-carrier adaptations:

| Path | Scope |
|---|---|
| `pi-coding-agent/src/utils/paths.rs` | process_cwd delegates to the shared authority |
| `pi-agent-core/src/harness/env/nodejs.rs` | relative Node resolver retains native NodeRejection |
| `pi-coding-agent/src/core/{sdk,settings_manager,session_manager}.rs` | canonical defaults; delete `/` fallback and preserve native error through SettingsFailure/SessionManagerError |
| `pi-coding-agent/src/utils/executable.rs` | canonical executable-lookup cwd |
| `pi-coding-agent/src/cli/startup_ui.rs` | delete `.` fallback; canonical startup-theme cwd |
| `pi-coding-agent/src/core/{self_restart,agent_session_runtime}.rs` | restart cwd and CallbackError source preserved through default restarter; other existing IO sources remain attached |
| `pi-coding-agent/src/architect/project_spec.rs` | canonical relative cwd, no `/` fallback, HandlerError carrier and lazy absolute-path branch |
| `pi-coding-agent/src/bundled_extensions/bwrap/worker.rs` | frozen fs-worker.cjs:7 truthy workspace/default cwd; native callback error reaches worker stderr via shared formatting; frozen :308–311 stringifies whole error, not error.message |
| `pi-ai/src/model_catalog_generator.rs` | canonical relative --json-output base; GeneratorErrorDetails retains native source and known cwd metadata |
| `pi-coding-agent/examples/extensions/{bash_spawn_hook,built_in_tool_renderer,ssh,titlebar_spinner}.rs` | shared getter and HandlerError native source; Node basename for title |
| `pi-coding-agent/examples/rpc_extension_ui.rs` | invocation cwd and native CLI error formatting |
| `pi-coding-agent/examples/sdk/{01_minimal,02_custom_model,03_custom_prompt,04_skills,05_tools,06_extensions,07_context_files,08_prompt_templates,09_api_keys_and_oauth,10_settings,11_sessions,12_full_control,13_session_runtime,common,error,resources}.rs` | canonical resource default; CallbackError propagation through sample helpers and real entrypoint wrapper |

All paths above are under `crates/`. Test fixture chdir calls in `pi-agent-core/tests/{differential_node_env,repin_node_legacy_wsl}.rs` and `pi-coding-agent/tests/{bash_stdin,bundled_extensions_run_plan,differential_config_install_paths,differential_read_classification,example_bash_spawn_hook,example_built_in_tool_renderer,example_ssh,process_cwd,session_executable_availability}.rs` now exercise the same shared authority too. Raw physical OS snapshots remain fixture operations.

The remaining direct `std::env::current_dir` calls outside the authority are test-only physical OS snapshots, expected fixture paths or saved restoration directories. `Command::current_dir` remains subprocess setup, not process.chdir. The before/after grep inventories are retained in the proof artifacts; no production direct getter/setter remains outside the authority.

## Approved runtime differences

### Node cwd cache

[Approved gap 2026-10-23](../../../AGENTS.md): native reads cwd directly. Existing Node warm/cold fixtures record the source behavior rather than reproduce its cache. After a warmed directory is deleted, source returns its old path while native must emit exactly the cold Node ENOENT/uv_cwd record. Failed chdir cannot resurrect that cached path. Recovery notices consequently cannot rely on Node's cached previous-path value; runtime recovery hook integration is still separate work, not proof claimed here.

`process-cwd-native-direct-red.log` proves the old emulated cache violated the new native-behavior requirement. Cache/mutex/poison handling was then deleted; tests assert both the source cached path and the native failure explicitly.

### Deleted old cwd during failed chdir

**Status: approved gap 2026-10-23**, recorded in [AGENTS.md](../../../AGENTS.md). When raw cwd lookup fails inside Node's failed native chdir, the reported old path is unstable. One reference probe emitted an empty old path; the multi-step probe emitted a stale prefixed path unrelated to the actual/cache path. Native emits an empty old path and exact corresponding message rather than reproducing unstable data.

The differential explicitly marks that one operation, asserts native empty path/message, compares every other error field, checks Node's own observed path/message relation, and then asserts subsequent direct native cwd failures under the separate approved cache gap. No general error normalization or assertion deletion is used. Captures: `process-chdir-node-shapes.jsonl`, `process-cwd-primitive-observed.json`.

NUL-prefix truncation is classified separately as observed Node native-binding behavior and preserved literally, not silently fixed into path validation.

## Proof ledger

Artifacts under `/tmp/pi-rust-repin-lossless-interactive/`:

- `process-cwd-cache-{red,green}.log`: historical warm/cold boundary before the user's cache exclusion; error-carrier evidence retained, cache implementation/proof superseded by direct-read tests.
- `process-cwd-primitive-pending-gap-green.log`: complete primitive sequence with the explicitly asserted, subsequently approved old-path discrepancy.
- `process-cwd-consumer-proof.log`: historical warm/cold + eight actual native consumers GREEN; direct-read change requires current focused integration proof.
- `process-cwd-absolute-architect-{red,green}.log`: frozen absolute-path laziness and native correction.
- `process-cwd-parent-sdk-baseline.log`: all six independent SDK example targets reproduce at unchanged parent bb562e5c, without cwd changes;226 owns reconciliation. The custom-prompt date/newline RED was also assigned to226. Successful sample targets and original failures remain in `process-cwd-carrier-development-2.log`; parent subsequently changed those source oracles, so current results require focused proof.

## Current bounded checkpoint proof

| Scope | Result / artifact |
|---|---|
| Direct native cwd requirement | Genuine RED→GREEN at fcf157f4; `process-cwd-native-direct-{red,green}.log`; 26 operations with explicit approved cache/stale-buffer differences |
| Actual native consumers and frozen Architect laziness | 3/3 tests, including eight carrier consumers; `process-cwd-native-final-consumers.log` |
| Architect / bwrap behavior | Architect17/17, differential_bwrap4/4, actual worker unit8/8; `process-cwd-native-final-{consumers,worker}.log` |
| Self-restart target | 18/20. Same two notice-reopen failures at pre-support99aea1f8, supportefc04568 and fcf157f4; `process-cwd-self-restart-baseline-{99aea1f8,efc04568}.log`. Current cwd, spawn/exec and IO-source cases pass. These predate this work and remain for the ordered runtime audit |
| SDK samples | 13 targets:7 pass, same six targets RED as unchanged bb562e5c baseline; `process-cwd-native-final-sdk.log`. Custom-prompt now passes after226's source reconciliation; other six remain226-owned |
| Shared compile | fcf157f4 workspace all-targets:0errors/0warnings; `process-cwd-native-final-workspace.{jsonl,log}`. Later edits only remove identity conversions/redundant borrows, express fixture permissions as octal and merge parent's ls slice; final integration command is recorded in the external ledger |
| Oracle checks | Strict pinned tsc for MTS, pinned Biome for both files, Node syntax check for MJS:0errors; `process-cwd-checkpoint-{typecheck,biome,javascript-syntax}.log`. Oracles unchanged since these checks |
| Clippy | Strict four-package attempt is debt-blocked (including existing HarnessEvent large_enum_variant and unrelated ai test findings). Coding-Agent strict/default diagnostic runs retain broader pre-existing debt; **not a clean full-clippy claim**. Owned11 worker redundant borrows,12 identity SDK conversions and prior owned octal-permission finding corrected without suppressions. Parent db25851f's SessionManager borrow is not owned here. See `process-cwd-native-{clippy,coding-clippy,clippy-all-consumers}*` and owned-hunk extraction |

Behavioral proof predating identity-conversion/redundant-borrow removal remains valid: these edits change neither values nor execution order; permission0→0o000 is identical. Main owns independent integration/final acceptance. This does not accept broader runtime recovery or erase final-gate debt.

## Readability audit

Manual changed-line audit; metrics executable unavailable. Direct authority has three short functions and no cache/lock/error-poison branch. Carriers retain source until explicit output boundaries; error-wire test fixtures use named fields. Native examples only widen carriers and remove identity conversions; no new suppressions or alternate paths. Oracle MTS/MJS each has a single short request/operation sequence.

Retained pre-existing `[LENGTH]` worker operation dispatch, Architect read_project_spec and SDK orchestration are not refactored in this carrier slice. `[LENGTH/NESTING]` GeneratorError::console_error gains a bounded native-property formatting arm inside the existing formatter; no general formatter redesign is claimed or authorized.
