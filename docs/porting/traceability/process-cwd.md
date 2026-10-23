# Node process cwd/chdir

[Contract](../../specs/process-cwd.md). Node26.11.1 built-in `lib/internal/bootstrap/switches/does_own_process_state.js`, locked by [version, hash and lines](../node-process-cwd-source-lock.json). Extracted from the installed reference with `process.binding("natives")`, not inferred from OS behavior.

Native: `crates/pi-js-compat/src/process_cwd.rs`. No extra dependency or per-consumer cache.

| Reference branch | Native representation | Proof |
|---|---|---|
| wrappedCwd :140–143 caches successful rawMethods.cwd | synchronized optional JsString cache; failed reads do not assign | frozen SessionManager warm/cold deleted-cwd RED/GREEN; cold error metadata also survives its native wrapper |
| wrappedChdir :126–130 validates string, calls raw method, then clears cache | typed string API; clear only after OS success | 26-operation Node/native sequence: successful clear, failed retention, warm deleted directory, cold deleted directory, recovery |
| native cwd byte ingress | shared decode_node_path; shared uv_cwd error record | exact UTF-16 values and name/message/code/errno/syscall/path/dest comparison |
| native chdir UTF-8 C-string input | encode_node_utf8, truncate at first NUL before OS call | existing/missing NUL prefix and lone-surrogate destinations; native returned cwd is the decoded actual path |
| chdir failure with existing old cwd | old physical cwd and destination retained in native error | ENOENT, ENOTDIR and empty destination full-record differential |
| failed chdir with deleted old physical cwd | deterministic empty old path | **pending user approval**, discrepancy below; other fields and subsequent warm cache are asserted against Node |
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
| `pi-coding-agent/examples/extensions/{bash_spawn_hook,built_in_tool_renderer,ssh,titlebar_spinner}.rs` | cached getter and HandlerError native source; Node basename for title |
| `pi-coding-agent/examples/rpc_extension_ui.rs` | invocation cwd and native CLI error formatting |
| `pi-coding-agent/examples/sdk/{01_minimal,02_custom_model,03_custom_prompt,04_skills,05_tools,06_extensions,07_context_files,08_prompt_templates,09_api_keys_and_oauth,10_settings,11_sessions,12_full_control,13_session_runtime,common,error,resources}.rs` | canonical resource default; CallbackError propagation through sample helpers and real entrypoint wrapper |

All paths above are under `crates/`. Test fixture chdir calls in `pi-agent-core/tests/{differential_node_env,repin_node_legacy_wsl}.rs` and `pi-coding-agent/tests/{bash_stdin,bundled_extensions_run_plan,differential_config_install_paths,differential_read_classification,example_bash_spawn_hook,example_built_in_tool_renderer,example_ssh,process_cwd,session_executable_availability}.rs` now use the authority too, so fixture setup cannot leave a stale cache.

The remaining direct `std::env::current_dir` calls outside the authority are test-only physical OS snapshots, expected fixture paths or saved restoration directories. `Command::current_dir` remains subprocess setup, not process.chdir. The before/after grep inventories are retained in the proof artifacts; no production direct getter/setter remains outside the authority.

## Discrepancy: deleted old cwd during failed chdir

**Status: pending user approval.** When raw cwd lookup fails inside Node's failed native chdir, the reported old path is unstable. One reference probe emitted an empty old path; the multi-step probe emitted a stale prefixed path unrelated to the actual/cache path. Native emits an empty old path and exact corresponding message rather than reproducing unstable data.

The differential explicitly marks that one operation, asserts native empty path/message, compares every other error field, checks Node's own observed path/message relation, and then compares subsequent cache results. No general error normalization or assertion deletion is used. Captures: `process-chdir-node-shapes.jsonl`, `process-cwd-primitive-observed.json`.

NUL-prefix truncation is classified separately as observed Node native-binding behavior and preserved literally, not silently fixed into path validation.

## Proof ledger

Artifacts under `/tmp/pi-rust-repin-lossless-interactive/`:

- `process-cwd-cache-{red,green}.log`: genuine warm/cold process-lifetime boundary; RED also exposed lost cold native metadata in SessionManager's wrapper.
- `process-cwd-primitive-pending-gap-green.log`: complete primitive sequence except the explicitly asserted pending old-path discrepancy.
- `process-cwd-consumer-proof.log`: frozen warm/cold + eight actual native consumers GREEN.
- `process-cwd-absolute-architect-{red,green}.log`: frozen absolute-path laziness and native correction.
- `process-cwd-parent-sdk-baseline.log`: all six unrelated SDK example targets reproduce at unchanged parent bb562e5c, without cwd changes;226 owns their stale model-runtime oracles/producers. Custom-prompt date/newline RED is separately assigned to226. Successful sample targets and the original failures remain in `process-cwd-carrier-development-2.log`.

Full workspace/checkpoint gates are recorded after commit. This does not accept the broader runtime recovery module or erase final-gate clippy debt.
