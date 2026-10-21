# Final integration gate — ba97d64b

Verifier report copied from `/tmp/pi-rust-ba97d64b-final-gate/REPORT.md`. Later change: `a88f2fa1` (test-only self_restart marker wait; `self_restart` 20/20, clippy clean) — intersects only that target.

# Pi Rust final integration gate — ba97d64b

## Result

**PASS for the requested Rust gates and all five runtime capabilities, after an explicitly authorized correction to the verifier’s interactive invocation.** Two earlier verifier-created bubblewrap wrapper invocations failed before Python or Pi started; their commands, exits, messages and classification remain below. Do not interpret this as an error-free verification procedure.

- Scope: `/syncthing/Sync/Projects/pi-rust`, clean `master`, exact HEAD `ba97d64bd77d6c7d5c5840bd669db975e2067e88` before and after. No tracked edits, commits, or fixes.
- Workspace tests: **817/817 harnesses successful; 5461 passed, 0 failed, 43 ignored**, 0 measured, 0 filtered. 23 harnesses contain no tests. Every ignored outer-harness entry is listed below; these child-entry probes are not 43 disabled product features.
- Rust compiler/Clippy diagnostics: **0 errors, 0 warnings** across formatting/check/Clippy/test/build. Test stdout/stderr includes **3 explicitly labeled runtime warning lines plus 8 extension console.warn lines**; exact messages and source triggers below. Expected negative-case diagnostics are retained, not reclassified as compiler errors.
- Runtime proof: help/version/print/RPC/interactive all exit 0. Print uses exactly one local fixture provider request; RPC uses zero. Interactive startup/exit only; no live conversation or provider claim.
- Proof scope: requested default-feature commands only; no all-features, doctest, coverage-percentage, online-provider, or full behavioral parity claim.

## Commands / proof ledger

Every row covers the exact unchanged HEAD above. Commands 1–4 each ran exactly once in order, then build/help/version/print/RPC/interactive. No workspace suite rerun. The only follow-ups were the recorded interactive-wrapper retry and the caller-authorized corrected invocation. Nothing changed tracked source/config, so earlier Rust proof remains valid. `CARGO_NET_OFFLINE=true` for commands; runtime invocations also set `PI_OFFLINE=1` and `PI_TEST_SANDBOX_DIR=/tmp/pi-rust-ba97d64b-final-gate`.

Full combined command output was saved via Pyrun `combined_tee`; it buffers and writes the destination on completion. The workspace test auto-backgrounded, stayed active, and completed after 1303.879 seconds; it was never restarted. The announced detached output.log was absent while active; final evidence is the completed `04-test.log` and exit record.

| Step / invocation | Exact command | Exit | Counts / result | Seconds | Full log |
|---|---|---:|---|---:|---|
| 1 | `cargo fmt --check` | 0 | PASS; 0 errors / warnings | 4.182 | `01-fmt.log` |
| 2 | `cargo check --workspace --all-targets` | 0 | PASS; 0 errors / warnings | 52.753 | `02-check.log` |
| 3 | `cargo clippy --workspace --all-targets --keep-going -- -D warnings` | 0 | PASS; 0 errors / warnings | 55.693 | `03-clippy.log` |
| 4 | `cargo test --workspace --all-targets --no-fail-fast` | 0 | PASS; 5461 pass / 0 fail / 43 ignored; 817 harnesses; 0 compiler warnings; 11 fixture warning lines | 1303.879 | `04-test.log` |
| 5-build | `cargo build --offline -p pi-coding-agent --bin pi` | 0 | PASS; rebuilt pi; 0 errors / warnings | 40.805 | `05-build.log` |
| 5-help | `python3 scripts/test-sandbox.py -- /syncthing/Sync/Projects/pi-rust/target/debug/pi --help` | 0 | PASS; 1/1 help invocation; 0 diagnostics | 0.372 | `06-help.log` |
| 5-version | `python3 scripts/test-sandbox.py -- /syncthing/Sync/Projects/pi-rust/target/debug/pi --version` | 0 | PASS; 1/1; version 0.84.1; 0 diagnostics | 0.102 | `07-version.log` |
| 5-print | `python3 scripts/test-sandbox.py -- python3 /tmp/pi-rust-ba97d64b-final-gate/e2e_driver.py print` | 0 | PASS; 1/1; child exit 0; local requests=1 | 0.930 | `08-print.log` |
| 5-rpc | `python3 scripts/test-sandbox.py -- python3 /tmp/pi-rust-ba97d64b-final-gate/e2e_driver.py rpc` | 0 | PASS; 1/1; get_state success; local requests=0 | 0.615 | `09-rpc.log` |
| 5-interactive | `python3 scripts/test-sandbox.py -- bwrap --bind /tmp/pi-rust-ba97d64b-final-gate /tmp/pi-rust-90efa0f2-final-gate -- python3 /tmp/pi-rust-90efa0f2-final-gate/pty_smoke.py` | 1 | ENVIRONMENT FAIL; wrapper only; Python/Pi not started | 0.064 | `10-interactive.log` |
| 5-interactive-isolated | `python3 scripts/test-sandbox.py -- bwrap --ro-bind / / --bind /tmp/pi-rust-ba97d64b-final-gate /tmp/pi-rust-90efa0f2-final-gate -- python3 /tmp/pi-rust-90efa0f2-final-gate/pty_smoke.py` | 1 | ENVIRONMENT FAIL; one isolated wrapper retry; Python/Pi not started | 0.066 | `11-interactive-isolated.log` |
| 5-interactive-corrected | `python3 scripts/test-sandbox.py -- python3 /tmp/pi-rust-ba97d64b-final-gate/pty_smoke.py` | 0 | PASS; authorized corrected invocation; child exit 0; markers 2/2; double Ctrl+C; no timeout | 1.058 | `12-interactive-corrected.log` |

## Per-target test totals

Complete target-by-target pass/fail/ignored/measured/filtered counts and exact source-log spans: [PER_TARGET.md](PER_TARGET.md). Machine-readable records, including each observed case line: [per-target.json](per-target.json). Each of 817 `Running` records has exactly one successful harness summary. Counts are outer Cargo harness counts; nested subprocess tests are not double-counted.

**Failing tests: none. Assertion failures: none. Targeted failing-test reruns: none required.** The two process failures below were verifier wrappers, not Rust test failures.

## Process failures — exact evidence and classification

| Location | Exact message | Classification / evidence | Isolation follow-up |
|---|---|---|---|
| `10-interactive.log:2` (no source file/line supplied by bwrap) | `bwrap: execvp python3: No such file or directory` | **Environment / verifier command**, not a demonstrated product regression. Verifier added a nested bwrap to alias the prior hardcoded proof path without mounting the root filesystem. Failure preceded Python/Pi startup; no interactive result artifact was produced. | One targeted wrapper retry, `11-interactive-isolated.log`, added `--ro-bind / /`: original missing-executable message did not recur; retry failed at the mount point instead. |
| `11-interactive-isolated.log:2` (no source file/line supplied by bwrap) | `bwrap: Can't create file /tmp/pi-rust-90efa0f2-final-gate: Read-only file system` | **Environment / verifier command**, not a demonstrated product regression. Nested root was read-only; old proof-path mount point was absent in private /tmp. Python/Pi still never started. | Caller explicitly authorized one corrected invocation without nested bwrap: change only proof-path constant and run original sandbox procedure. `12-interactive-corrected.log`: exit 0, child exit 0, both markers, double Ctrl+C, no timeout. Mount-point failure did not recur. |

No source changes were made to correct either process wrapper. Original failed invocations were not silently replaced in logs or exit ledger.

## Runtime warnings — every emitted warning

All warnings below occurred inside sandboxed tests whose assertions passed. Classification is **environment/fixture-triggered**, not evidence of a product regression. No failing assertion or failing harness accompanied them. No targeted rerun was performed for a passing test merely to reproduce a warning. Node warnings omit their own emitting source file/line; exact test trigger locations are given instead of inventing a Node location.

| Log location | Exact message | Source trigger / classification evidence |
|---|---|---|
| `04-test.log:1415` | `(node:1977346) Warning: Setting the NODE_TLS_REJECT_UNAUTHORIZED environment variable to '0' makes TLS connections and HTTPS requests insecure by disabling certificate verification.` | `crates/pi-ai/tests/differential_google_api.rs:1012` explicitly sets the variable for the local Google Vertex oracle fixture; lines 1417–1420 show both Google differential tests passed. Network namespace prevents host/external access. |
| `04-test.log:2233` | `(node:1987816) ExperimentalWarning: VM Modules is an experimental feature and might change at any time` | `crates/pi-ai/tests/model_data_tools.rs:352` and `:415` explicitly enable `--experimental-vm-modules`; native model-data checker differential fixtures. 17/17 target tests pass at log:2238. |
| `04-test.log:7090` | `Warning: No project session found with id 'chosen-id'; creating a new session with that id.` | Emitted by `crates/pi-coding-agent/src/main.rs:1576`; `crates/pi-coding-agent/tests/main.rs:401` exercises creation with an absent exact session ID. Passing fixture behavior, not missing live state. |
| `04-test.log:6386` | `Extension shortcut 'ctrl+c' from /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/a.ts conflicts with built-in shortcut. Skipping.` | Pinned TypeScript `packages/coding-agent/src/core/extensions/runner.ts:653` via `console.warn` at :642. Conflicting shortcut fixture: `crates/pi-coding-agent/tests/extensions.rs:410–413`; matching runner test passes at log:6394. |
| `04-test.log:6387` | `Extension shortcut 'ctrl+p' from /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/b.ts conflicts with built-in shortcut. Skipping.` | Pinned TypeScript `packages/coding-agent/src/core/extensions/runner.ts:653` via `console.warn` at :642. Conflicting shortcut fixture: `crates/pi-coding-agent/tests/extensions.rs:410–413`; matching runner test passes at log:6394. |
| `04-test.log:6388` | `Extension shortcut conflict: 'ctrl+v' is built-in shortcut for app.clipboard.pasteImage and /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/c.ts. Using /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/c.ts.` | Pinned TypeScript `packages/coding-agent/src/core/extensions/runner.ts:661` via `console.warn` at :642. Conflicting shortcut fixture: `crates/pi-coding-agent/tests/extensions.rs:410–413`; matching runner test passes at log:6394. |
| `04-test.log:6389` | `Extension shortcut conflict: 'ctrl+shift+x' registered by both /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/d.ts and /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/e.ts. Using /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/e.ts.` | Pinned TypeScript `packages/coding-agent/src/core/extensions/runner.ts:669` via `console.warn` at :642. Conflicting shortcut fixture: `crates/pi-coding-agent/tests/extensions.rs:410–413`; matching runner test passes at log:6394. |
| `04-test.log:6390` | `Extension shortcut 'ctrl+c' from /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/a.ts conflicts with built-in shortcut. Skipping.` | Pinned TypeScript `packages/coding-agent/src/core/extensions/runner.ts:653` via `console.warn` at :642. Conflicting shortcut fixture: `crates/pi-coding-agent/tests/extensions.rs:410–413`; matching runner test passes at log:6394. |
| `04-test.log:6391` | `Extension shortcut 'ctrl+p' from /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/b.ts conflicts with built-in shortcut. Skipping.` | Pinned TypeScript `packages/coding-agent/src/core/extensions/runner.ts:653` via `console.warn` at :642. Conflicting shortcut fixture: `crates/pi-coding-agent/tests/extensions.rs:410–413`; matching runner test passes at log:6394. |
| `04-test.log:6392` | `Extension shortcut conflict: 'ctrl+v' is built-in shortcut for app.clipboard.pasteImage and /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/c.ts. Using /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/c.ts.` | Pinned TypeScript `packages/coding-agent/src/core/extensions/runner.ts:661` via `console.warn` at :642. Conflicting shortcut fixture: `crates/pi-coding-agent/tests/extensions.rs:410–413`; matching runner test passes at log:6394. |
| `04-test.log:6393` | `Extension shortcut conflict: 'ctrl+shift+x' registered by both /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/d.ts and /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/e.ts. Using /tmp/pi-test-93wv7o2o/tmp/pi-ext-oracle-IUdaEM/extensions/bundle/e.ts.` | Pinned TypeScript `packages/coding-agent/src/core/extensions/runner.ts:669` via `console.warn` at :642. Conflicting shortcut fixture: `crates/pi-coding-agent/tests/extensions.rs:410–413`; matching runner test passes at log:6394. |

### Other error-shaped fixture output (not failed assertions)

The complete non-harness output is reproduced with exact original log line numbers in [DIAGNOSTICS.md](DIAGNOSTICS.md). This includes all warning messages above and full native-fetch stack traces. None was suppressed by rerunning a command.

| Log lines | Classification / evidence |
|---|---|
| `04-test.log:1443–1473`, `:2128–2157` | Native-fetch refused-loopback and truncated-response fixtures; exact internal Node file:line stack traces remain in DIAGNOSTICS.md. Enclosing `tests/differential_http_failures.rs` and `tests/model_catalog_generator_native_http.rs` targets pass both tests; errors are tested inputs. |
| `04-test.log:1576–1577`, `:1586–1587` | Malformed SSE JSON fixtures in passing OpenAI completions/responses differentials. |
| `04-test.log:3580–3581` | Stubbed claude-memory malformed-result and exit-3 fixtures; `claude_memory_enrich_matches_typescript` passes at :3582. No live claude-memory helper was executed; sandbox binds its recording stub. |
| `04-test.log:6188–6192` | Expected invalid regex (`error: unclosed character class`) and nonexistent-path rg errors; test `real_rg_results_truncation_artifacts_errors_and_renderers_match` passes at :6193. |
| `04-test.log:7080`, `:7082–7087`, `:7092` | Injected main CLI errors (`boom`, conflicting flags, invalid session ID, injected SetRawMode failure, missing-session lookup); adjacent named behavior tests pass. Exact strings retained in DIAGNOSTICS.md. |
| `04-test.log:7961` | `oops`: child-stderr fixture; `collects_child_stderr` passes at :7962. |
| `04-test.log:8140` | `/bin/sh: line 1: /tmp/pi-test-96tb1omy/tmp/.tmpg7g9Ux/delayed-marker: No such file or directory`. **Fixture cleanup artifact, not a failed test**. `crates/pi-coding-agent/tests/self_restart.rs:455–462` starts `sleep 0.2; printf done > <TempDir>/delayed-marker` with non-waiting restart at :475, then drops TempDir. This diagnostic is consistent with the child outliving directory cleanup; the test only asserts immediate return, not marker creation. The prior `self_restart` harness passed 20/20 (:8132); this late child stderr arrived during `self_restart_exec_env`, whose three tests also passed (:8143). The message contains the prior harness's private directory `pi-test-96tb1omy`, not the current harness's `pi-test-715jle_i`. No isolated rerun: no failed test and caller prohibited extra broad repetitions. This is a test-hygiene limitation, not proof that the late child write succeeds. |

## Sandboxed end-to-end capability matrix

| Capability | Concrete proof | Missing / excluded coverage |
|---|---|---|
| Help | `06-help.log`: exit 0, usage/options/examples/tool names. | No provider execution. |
| Version | `07-version.log`: exit 0, `0.84.1`. | No update/network checks. |
| Print | `08-print.log`, `print.result.json`: explicit `-p hello`, `-ne --offline --model demo/demo-model --api-key test`; stdout exactly `hello\n`, stderr empty; exactly one local `/v1/chat/completions` SSE fixture request. | Local fixture only, not a live provider. |
| RPC | `09-rpc.log`, `rpc.result.json`: matching `final-gate-state` response to `get_state`, command matches, success=true, messageCount=0, child exit 0, stderr empty, zero provider requests. | Handshake only; no prompt/stream/tool conversation. |
| Interactive | `12-interactive-corrected.log`, `interactive.result.json`: no-argument freshly built pi in 40×120 PTY; both `Ctrl+C twice to exit` and feature hint observed; double Ctrl+C sent; child exit 0; no timeout; stderr empty; 11746 raw stdout bytes. | Startup and exit only, not interactive provider/tool use. |

Sandbox implementation: `.cargo/config.toml` routes test executables through `scripts/cargo-runner.sh` / `scripts/test-sandbox.py`. Step-5 processes used that same test-sandbox explicitly. It supplies private HOME/XDG/agent/state, private network namespace with loopback fixtures only, read-only repository/reference mounts, private devpts, and a recording claude-memory stub. No direct host Node/tsx/Pi runtime invocation; no host Pi config, credentials or live provider access. Cargo commands ran offline. Host `~/.config/pi` was neither inspected nor modified by these verification commands.

### Driver reuse / authorized path-only change

`e2e_driver.py` is byte-identical to `/tmp/pi-rust-90efa0f2-final-gate/e2e_driver.py` (SHA-256 `15e80445721711bebe3ad1cfa39b73cc784ae4ffeddddf2cbdbb8a19873258f0`). It already derives its output directory from the sandbox agent path. `pty_smoke.original.py` preserves exact prior driver bytes (SHA-256 `c04621117d445466deebcb050c0e84e2d171f8d378678a10cfeedc4706f94c2f`). After the two wrapper failures, caller explicitly instructed: “copy pty_smoke.py into the new proof dir changing only the proof-directory path constant (record the diff), then run it once” via the ordinary test-sandbox. That single corrected invocation passed.

Exact sole driver diff, also saved as [pty_smoke.diff](pty_smoke.diff):

```diff
--- prior/pty_smoke.py
+++ current/pty_smoke.py
@@ -10,7 +10,7 @@
 import termios
 import time
 
-proof = Path('/tmp/pi-rust-90efa0f2-final-gate')
+proof = Path('/tmp/pi-rust-ba97d64b-final-gate')
 environment = dict(os.environ)
 (proof / 'environment.json').write_text(json.dumps(environment, indent=2))
 binary = '/syncthing/Sync/Projects/pi-rust/target/debug/pi'
```

Previous proof files were not overwritten. No repo/runtime source was changed. Private state, requests, stdout/stderr and environment artifacts remain in this proof directory.

## Every ignored outer-harness test (43)

Explicit ignore reasons are copied verbatim. Four entries use bare `#[ignore]`; their bodies/callers establish isolated environment child probes. Evidence for config-status probes: `tests/support/config_value_status_probe.rs:38–42` invokes `--ignored --exact config_status_process_probe_child`; package-manager parent probes call `env_probe::run_probe` at `tests/package_manager_git.rs:238` and `:283`. These files are under `crates/pi-coding-agent/`. Child subprocess output is captured by parents, so no claim is made that each outer ignored line itself executed.

| Test (target-specific) | Definition | Reason | Log line |
|---|---|---|---:|
| `tests/config_paths.rs::config_probe_child` | `crates/pi-coding-agent/tests/config_paths.rs:16` | child probe entry point | 4016 |
| `tests/config_selector_cli.rs::config_selector_probe_child` | `crates/pi-coding-agent/tests/config_selector_cli.rs:189` | child probe entry point | 4048 |
| `tests/config_value_status.rs::config_status_process_probe_child` | `crates/pi-coding-agent/tests/config_value_status.rs:103` | Isolated environment probe, invoked by parent with --ignored --exact; no reason string in #[ignore]. | 4077 |
| `tests/desktop_notification.rs::desktop_notification_probe_child` | `crates/pi-coding-agent/tests/desktop_notification.rs:148` | child process probe | 4233 |
| `tests/differential_components.rs::components_probe_child` | `crates/pi-coding-agent/tests/differential_components.rs:687` | child process probe | 4576 |
| `tests/differential_config.rs::config_probe_child` | `crates/pi-coding-agent/tests/differential_config.rs:26` | child probe entry point | 4596 |
| `tests/differential_config_auth_state.rs::config_probe_child` | `crates/pi-coding-agent/tests/differential_config_auth_state.rs:27` | child probe entry point | 4606 |
| `tests/differential_config_auth_state.rs::trust_resources_probe_child` | `crates/pi-coding-agent/tests/differential_config_auth_state.rs:33` | child probe entry point | 4607 |
| `tests/differential_config_value_references.rs::config_status_process_probe_child` | `crates/pi-coding-agent/tests/differential_config_value_references.rs:13` | Isolated environment probe, invoked by parent with --ignored --exact; no reason string in #[ignore]. | 4618 |
| `tests/differential_config_value_references.rs::config_value_reference_oracle::resolve_process_probe_child` | `crates/pi-coding-agent/tests/differential_config_value_references/mod.rs:10` | child probe entry point | 4619 |
| `tests/differential_export_html.rs::differential_export_probe` | `crates/pi-coding-agent/tests/differential_export_html.rs:78` | child process probe | 4706 |
| `tests/differential_export_html.rs::differential_missing_file_probe` | `crates/pi-coding-agent/tests/differential_export_html.rs:145` | child process probe | 4707 |
| `tests/differential_system_prompt.rs::system_prompt_probe_child` | `crates/pi-coding-agent/tests/differential_system_prompt.rs:11` | child probe entry point | 5267 |
| `tests/differential_tools_index.rs::experimental_sampling_probe` | `crates/pi-coding-agent/tests/differential_tools_index.rs:133` | child process probe | 5317 |
| `tests/experimental.rs::experimental_probe_child` | `crates/pi-coding-agent/tests/experimental.rs:51` | child process probe | 6280 |
| `tests/export_html.rs::export_agent_session_probe` | `crates/pi-coding-agent/tests/export_html.rs:207` | child process probe | 6329 |
| `tests/export_html.rs::export_file_probe` | `crates/pi-coding-agent/tests/export_html.rs:28` | child process probe | 6330 |
| `tests/export_html.rs::export_in_memory_probe` | `crates/pi-coding-agent/tests/export_html.rs:185` | child process probe | 6331 |
| `tests/export_html.rs::export_session_probe` | `crates/pi-coding-agent/tests/export_html.rs:75` | child process probe | 6332 |
| `tests/interactive_crash.rs::terminal_stop_child` | `crates/pi-coding-agent/tests/interactive_crash.rs:359` | PTY child entry point | 6788 |
| `tests/interactive_crash.rs::worker_panic_child` | `crates/pi-coding-agent/tests/interactive_crash.rs:276` | PTY child entry point | 6789 |
| `tests/output_guard.rs::output_guard_probe_child` | `crates/pi-coding-agent/tests/output_guard.rs:39` | child process probe | 7421 |
| `tests/package_manager_git.rs::git_dependencies_install_without_dev_dependencies_by_default_probe` | `crates/pi-coding-agent/tests/package_manager_git.rs:249` | Isolated environment probe, invoked by parent with --ignored --exact; no reason string in #[ignore]. | 7530 |
| `tests/package_manager_git.rs::offline_mode_skips_missing_package_installs_probe` | `crates/pi-coding-agent/tests/package_manager_git.rs:296` | Isolated environment probe, invoked by parent with --ignored --exact; no reason string in #[ignore]. | 7531 |
| `tests/startup_ui.rs::startup_input_probe_child` | `crates/pi-coding-agent/tests/startup_ui.rs:279` | child probe entry point | 8548 |
| `tests/startup_ui.rs::startup_selector_probe_child` | `crates/pi-coding-agent/tests/startup_ui.rs:243` | child probe entry point | 8549 |
| `tests/theme.rs::theme_detection_probe` | `crates/pi-coding-agent/tests/theme.rs:70` | child process probe | 8673 |
| `tests/theme.rs::theme_export_probe` | `crates/pi-coding-agent/tests/theme.rs:324` | child process probe | 8674 |
| `tests/theme.rs::theme_global_probe` | `crates/pi-coding-agent/tests/theme.rs:502` | child process probe | 8675 |
| `tests/theme.rs::theme_language_probe` | `crates/pi-coding-agent/tests/theme.rs:706` | child process probe | 8676 |
| `tests/theme.rs::theme_missing_builtin_probe` | `crates/pi-coding-agent/tests/theme.rs:733` | child process probe | 8677 |
| `tests/theme.rs::theme_picker_probe` | `crates/pi-coding-agent/tests/theme.rs:397` | child process probe | 8678 |
| `tests/theme.rs::theme_resolved_probe` | `crates/pi-coding-agent/tests/theme.rs:596` | child process probe | 8679 |
| `tests/theme.rs::theme_scrollbar_probe` | `crates/pi-coding-agent/tests/theme.rs:430` | child process probe | 8680 |
| `tests/theme.rs::theme_validation_probe` | `crates/pi-coding-agent/tests/theme.rs:618` | child process probe | 8681 |
| `tests/theme_controller.rs::theme_controller_auto_probe` | `crates/pi-coding-agent/tests/theme_controller.rs:93` | child process probe | 8698 |
| `tests/theme_controller.rs::theme_controller_detect_probe` | `crates/pi-coding-agent/tests/theme_controller.rs:125` | child process probe | 8699 |
| `tests/theme_controller.rs::theme_controller_explicit_probe` | `crates/pi-coding-agent/tests/theme_controller.rs:51` | child process probe | 8700 |
| `tests/theme_controller.rs::theme_controller_rebind_probe` | `crates/pi-coding-agent/tests/theme_controller.rs:153` | child process probe | 8701 |
| `tests/timings.rs::timings_probe_child` | `crates/pi-coding-agent/tests/timings.rs:42` | child process probe | 8722 |
| `tests/trust_manager.rs::trust_resources_probe_child` | `crates/pi-coding-agent/tests/trust_manager.rs:20` | child probe entry point | 8849 |
| `tests/uncaught.rs::crash_handler_child` | `crates/pi-coding-agent/tests/uncaught.rs:152` | child process entry point | 8863 |
| `tests/uncaught.rs::uncaught_handler_child` | `crates/pi-coding-agent/tests/uncaught.rs:60` | child process entry point | 8866 |

## Artifacts / integrity

`steps.jsonl` records exact argv, exits, durations and log paths. `initial-state.txt` / `final-state.txt` record clean master and unchanged full HEAD, observed via git before/after. `PER_TARGET.md`, `per-target.json`, `ignored.json` and `DIAGNOSTICS.md` are derived solely from the saved command output and inspected source; no test was rerun to generate them. All files were read/parsed in full; diagnostics were additionally searched via the grep tool.

No product regression was demonstrated by this run. Warnings and the late marker-write artifact are disclosed rather than called “zero runtime warnings.” The result does not establish that every requested compatibility feature is implemented; it establishes only these exact verification commands and bounded runtime capabilities.
