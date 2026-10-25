# Final gate preparation

Goal: make strict all-target/all-feature clippy pass for pi-js-compat, pi-tui, pi-ai, and pi-agent-core without behavior changes; replace the three specified hostname conversions and matching test expectations with the existing Node byte decoder.

Excluded: other coding-agent changes, weakened assertions, lint suppression, network, push, deploy, and delegation. Report lints that cannot be fixed mechanically without behavior changes.

Main extended scope after the test runs: diagnose the OpenAI Completions ID assertion and input differential via history and frozen 4f9e6f8a4. Correct a frozen-proven stale test or restore frozen behavior with the existing assert; one reasoned commit per failure. Verify each affected target, strict crate clippy, formatting, and workspace check. Do not change unrelated behavior.

Main further authorized diagnosis and repair of the OAuth callback timeout: repeat the composed suite, distinguish a timing-based harness wait from a production ordering race, fix the cause (deterministic AuthUrl notification or frozen-order login repair), and commit separately. Do not raise the timeout. Verify repeated composed runs, crate clippy, formatting, and workspace check.

- [x] pi-js-compat: consolidate duplicate test modules; strict clippy; format and workspace check; commit.
- [x] pi-tui: mechanical lint fixes; strict clippy; format and workspace check; commit.
- [x] pi-ai: mechanical lint fixes; strict clippy; format and workspace check; commit.
- [x] pi-agent-core: mechanical lint fixes; strict clippy; format and workspace check; commit.
- [x] Hostname ingress: update three sites and matching tests; format and workspace check; commits per crate.
- [x] Run each requested crate's existing tests once, with logs; run inventory; report SHA, results, and skipped lints to main.
- [x] Repair the callback-listener lifetime race; record deterministic RED/GREEN and repeated composed-suite stability.

Proof ledger: command logs in /tmp/pi-repin-final-gate; record revision/scope and invalidation in the final report.

## Proof ledger (2026-10-25)

All commands ran in this worktree with `-j3`. Logs are under `/tmp/pi-repin-final-gate/`. Later changes below do not invalidate earlier unchanged-crate proof. Initial lint work changed only test wiring; the separately authorized OAuth listener repair changes pi-ai production code and requires new affected-scope proof.

| Scope / exact revision | Command | Result / evidence |
| --- | --- | --- |
| pi-js-compat / 33d7026c6 | `cargo clippy -j3 -p pi-js-compat --all-targets --all-features -- -D warnings` | PASS; tool output in this session; only shared oracle module declarations/imports changed |
| pi-tui / 5635b3fc7 | same strict clippy command with `-p pi-tui` | PASS; `tui-clippy.log` |
| pi-ai / 484d9ec9c | same strict clippy command with `-p pi-ai` | PASS; `ai-callback-clippy.log` |
| pi-agent-core / 68976aab0 | same strict clippy command with `-p pi-agent-core` | PASS; `agent-clippy.log` |
| Each code commit | `cargo fmt --check`; `cargo check -j3 --workspace --all-targets` | PASS before each commit; `*-workspace-check.log` and session output |
| pi-js-compat / 838461c09 | `cargo test -j3 -p pi-js-compat` | PASS: 162 tests; 2 existing helper ignores; `js-tests.log` |
| pi-agent-core / 838461c09 | `cargo test -j3 -p pi-agent-core` | PASS: 458 tests; `agent-tests.log` |
| pi-tui / 5635b3fc7 | `cargo test -j3 -p pi-tui` | PASS: 864 tests; 2 existing helper ignores; `tui-tests-green.log` |
| pi-ai / 838461c09 | `cargo test -j3 -p pi-ai` | FAIL: stale library ID assert; Cargo stopped before integrations; `ai-tests.log` |
| pi-ai library / 6833cb6bf | `cargo test -j3 -p pi-ai --lib` | PASS: 28 tests; stale exact ID expectations corrected, no production change; `ai-lib-green.log` |
| pi-ai integration / 5635b3fc7 | `cargo test -j3 -p pi-ai --test '*' --no-fail-fast` | 871 PASS, 2 FAIL, 4 existing helper ignores; `ai-integration-tests.log` |
| pi-ai request ID fixture / 84b2e2d30 | `cargo test -j3 -p pi-ai --test suite_core_2 openai_completions::tool_call_ids_are_normalized_for_pipe_ids -- --exact` | PASS after correcting the second stale ID fixture; `ai-replay-integration-green.log` |
| pi-ai frozen replay / 838461c09 | `cargo test -j3 -p pi-ai --test suite_core_1 differential_openai_completions::completions_replay_ids_empty_prompt_and_empty_tool_result_match_frozen -- --exact` | PASS; `ai-replay-frozen.log` |
| pi-ai callback / 5635b3fc7 plus unrelated ID-test edit | `cargo test -j3 -p pi-ai --test suite_core_2 oauth_repin_edges::browser_callbacks_reject_empty_codes_without_settling_login -- --exact` | PASS alone unchanged; `ai-callback-isolated.log`; composed run timed out waiting 2 seconds for AuthUrl at oauth_repin_edges.rs:369 |
| pi-ai docs / 84b2e2d30 | `cargo test -j3 -p pi-ai --doc` | PASS; `ai-doc-tests.log` |
| hostname relay / 84b2e2d30 | `cargo test -j3 -p pi-coding-agent --test suite_core_3 host_relay::` | PASS: 2; `hostname-relay-tests.log` |
| hostname terminal / 84b2e2d30 | `cargo test -j3 -p pi-coding-agent --test suite_core_5 terminal_current_directory::` | PASS: 5; `hostname-terminal-tests.log` |
| pi-ai / 484d9ec9c | `cargo test -j3 -p pi-ai` | PASS: 902 tests; 4 existing helper ignores; `ai-tests-final.log`; supersedes initial failures above |
| pi-ai composed stability / 484d9ec9c | `cargo test -j3 -p pi-ai --test suite_core_2`, loop 5 times | PASS: 401 tests each; `ai-composed-fixed-{1..5}.log` |
| pi-ai callback contention / 484d9ec9c | `cargo test -j3 -p pi-ai --test suite_core_2 oauth_repin_edges:: -- --test-threads=400`, loop 3 times | PASS: 21 tests each; `ai-callback-contention-fixed-{1..3}.log` |
| pi-ai callback scope / 484d9ec9c | focused OAuth edge tests before commit | PASS: original callback assertions plus deterministic close/rebind regression; `callback-close-green.log`; RED saved in `callback-close-red.log` |
| source inventory / 484d9ec9c | `python3 scripts/source-inventory.py` | PASS: 686 unique rows, 186679 lines; locked source counts unchanged |

## Disposition

- No skipped lints, new dependencies, lint suppression, or weakened assertions. The only production behavior correction in the four linted crates is the separately authorized callback-close repair. Duplicate modules compile once per suite via suite-level `#[path]` declarations and imports.
- Hostname production edits are exactly the three authorized sites; the only other coding-agent edits are the two specified test files. All use `pi_js_compat::node_fs::decode_node_path` for hostname byte ingress.
- Stale Completions ID tests: `6833cb6bf` (library) and `84b2e2d30` (integration), caused by `22a70abd9` restoring frozen prefix extraction while retaining assertions from `04eeff7b8`. Frozen evidence: openai-completions.ts:855–867. Exact assertions remain; long-prefix assertion strengthened.
- Input differential: `5635b3fc7` restores process isolation lost in `e60b2edbe`; unchanged frozen differential assertion passes. Source evidence: keys.ts:25,1162–1165 and terminal negotiation fixtures that set the global Kitty flag. No input implementation or oracle edits.
- OAuth follow-up: 13 default composed repeats passed before the repair. A 400-thread stress run exposed a callback EADDRINUSE and an unrelated frozen generator VM 500ms timeout. A native close→immediate rebind test then failed deterministically; Node's same operation passed. The shared listener now closes synchronously. The original timeout had no login-result diagnostic, so its link to the proven port race is an inference. Fixed in `484d9ec9c`, introduced in `3e6438e6e`. Full AI tests pass; five default composed repeats (401 each) and three callback contention repeats (21 each) pass after the fix. No generator or timeout assertion was changed. The 400-thread whole-suite stress failure remains recorded as an extreme-concurrency limitation, not a default-suite failure; only its callback port race was in repair scope.

Changed-line readability audit: reviewed import/path moves, exact test-value corrections, the three decoder calls, and callback listener ownership. The listener mutex covers one nonblocking poll or synchronous close, never an await; the accept helper keeps the close/accept control flow explicit. No new suppressions.


## Remaining-crate final gate (2026-10-25): final report

Goal: run the remaining component suites in the requested order, diagnose real failures against history and frozen 4f9e6f8a4, commit each correction separately, rerun affected targets, and run full facade stability/acceptance suites. Excluded: rerunning the four previously passed crates, production changes without frozen evidence, network/live providers, push/deploy, delegation, relaxed assertions, new ignores and suppression.

- [x] Fast-forward parent 1d4e3fbba before the ordered component runs.
- [x] Run all thirteen component suites once with `cargo test -j4 -p CRATE --no-fail-fast`.
- [x] Complete the initial facade suite, diagnose failures, and rerun affected targets after corrections.
- [x] Run additional full facade stability suites; retain every failure and uncertainty.
- [x] Fast-forward the integrated parent and pass its explicitly requested full acceptance run.
- [x] Record counts, fixes, proof scope and remaining instability for main.

### Acceptance

**PASS at code revision `9f0e586031910fccdc626af6149eab71a430f446`.** Command: `cargo test -j4 -p pi-coding-agent --no-fail-fast`. Cargo exit 0, tee exit 0, 1558.9 seconds. **3818 passed / 0 failed / 113 existing ignores** across 266 top-level libtest target summaries. Additional harness=false proof: 22 real bash lifecycle histories, bwrap full-access detached lifecycle, and fresh Pyrun foreground/detached lifecycle all PASS.

Evidence: `/tmp/pi-repin-final-gate/remaining-facade-acceptance.log`, `.exit.json`, `.revision`. All long facade runs used real live tee after discovering that Pyrun's combined_tee helper buffers until command completion. Counts use only the last summary of each top-level Cargo target, not nested child-helper summaries. This final commit changes only this ledger, so it does not invalidate acceptance code proof.

### Component results

Initial component runs began at 1d4e3fbba. Later test-only corrections have the affected-target proof below. Logs are `/tmp/pi-repin-final-gate/remaining-NAME.log`; all doc tests passed with zero test cases.

| Crate | Initial pass/fail | Latest affected proof | Existing ignores |
| --- | --- | --- | --- |
| pi-coding-agent-base | 5/0 | unchanged | 0 |
| pi-coding-agent-data | 7/0 | unchanged | 0 |
| pi-coding-agent-models | 1/0 | unchanged | 0 |
| pi-coding-agent-packages | 12/0 | unchanged | 0 |
| pi-coding-agent-settings | 0/0 | integration behavior covered by facade acceptance | 0 |
| pi-coding-agent-tool-support | 5/0 | unchanged | 0 |
| pi-coding-agent-storage | 12/1 | 13/0 full lib, remaining-storage-lib-green.log | 0 |
| pi-coding-agent-tools | 23/0 | unchanged component target; facade tests also pass | 0 |
| pi-coding-agent-ui | 4/0 | unchanged component target; integrated facade covers parent changes | 0 |
| pi-coding-agent-core | 3/0 | unchanged | 1 |
| pi-coding-agent-extensions | 56/0 | unchanged component target; facade tests also pass | 0 |
| pi-coding-agent-interactive | 172/1 | 173/0 full lib, remaining-interactive-green.log | 7 |
| pi-orchestrator | 68/0 | unchanged | 4 |
| pi-coding-agent | see run ledger | **3818/0 integrated acceptance** | 113 |

Previously passed pi-js-compat, pi-tui, pi-ai and pi-agent-core suites were not rerun. Parent merges included separately owned library/test lint work; no lint commit was reverted or blamed without evidence.

### Full facade run ledger

| Run / exact starting revision | Pass/fail/ignored | Exit / evidence |
| --- | --- | --- |
| Original buffered attempt / 1d4e3fbba | no aggregate proof | User interruption killed collector; orphan Cargo stopped; remaining-facade-aborted.txt. A stalled detached-bash payload was terminated only to release failed-test teardown; stack/intervention evidence retained. |
| 1 / 0bfc513d3 | 3804/13/113 | 101; remaining-facade-1.log. One stalled RPC fixture harness was signalled to permit no-fail-fast traversal; not counted as passing. |
| 2 / d40cc808b | 3816/1/113 | 101; remaining-facade-2.log; 1468.6s. Artifact cleanup retained a directory once. |
| 3 / b88fc55b0 | 3817/0/113 | 0; remaining-facade-3.log; 1394.8s. |
| 4 / b88fc55b0 | 3812/5/113 | 101; remaining-facade-4.log; 1518.5s. Subsequent affected-target fixes/proof below. |
| Acceptance / 9f0e58603 | **3818/0/113** | **0**; remaining-facade-acceptance.log; 1558.9s. |

Every completed run also passed the three custom lifecycle binaries. Revisions are immutable starting code snapshots; fixes committed during a run have separate affected-target proof, never retroactively turn its failures into passes. Main explicitly requested the last acceptance run after integrating cec1eb768 and other lanes' changes.

### Fixes and frozen evidence

| Commits | Cause and correction | Frozen evidence / affected proof |
| --- | --- | --- |
| 15805d52b | Cancellation payload fixture used expired October 21 time after 918b53966 restored retention. Sample live time; raw-field assertions unchanged. | session-control-db.ts:4320–4323 checks cancellation freshness; storage lib 13/0. |
| f56423da7 | OAuth selector fixture registration raced other modes resetting global OAuth providers. Run its unchanged callback differential in an exact self-child. | model-registry.ts:389 reset; auth-storage.ts:405–407 rejects unknown providers. Forced reset reproduces original mount failure; interactive full lib 173/0. |
| 2d3ad6bd2; parent 0bfc513d3; 761bc887c; 3f6511e70 | Detached bash, Pyrun process and controller fixtures used July/October 18 cancellation clocks after 918b53966. Use live clocks without changing lifecycle/revision assertions. Parent removed a redundant conversion/closure. | Same frozen freshness check; detached_bash_runner 13/0, detached_pyrun_runner_process 8/0, suite_core_1 303/0. Initial bash failure left Tokio teardown waiting on its endless payload; eu-stack and intervention logs retained. |
| ff176d2f2, superseded by 0ab05b5fe | Mailbox fixtures refreshed/replaced the same global faux stream. A competing fixture deterministically reproduced the first timeout. Later repeats exposed the same pattern in question-result and steering-delivery cases. Hold one async registry-fixture guard through every TestEnv lifetime; delete redundant one-case self-child wrapper. | compat.ts:121–132 replaces by API; model-registry refresh resets APIs. Whole mailbox target 12/0, remaining-mailbox-guard-green.log; integrated acceptance passes. |
| 4cf694f9b | RPC UI test drove fixed 750/250/250ms sleeps. Escape could cancel a still-pending dialog instead of exiting. Wait for actual RPC Chat, Enter a value and You entered: Ada frames. | rpc-demo.ts:86–88 awaits input before notification. Old result/output/cleanup assertions unchanged; entire target 7/0. |
| 56b1ca156 | SDK fixture retained JSON quotes after 4e68c35cc restored String(value) error text. Correct exact text and SDK trace. | settings-manager.ts:198–204; SDK target 15/0. |
| 9b1ac52fa | Bundled fast fixture retained old provider-name list after eba61811e restored frozen warning. Correct exact warning only. | codex-fast/src/index.ts:114; bundled_commands module 25/0. |
| aa878003a | Main authorized caching all six fixed Normalizer regexes after the first run. Use LazyLock, preserving patterns/replacements/order. Test support only. | Unchanged large-output differential passes (21.95s); full accepted suite_modes 214/0 (29.44s). Old composed suite_modes was 140.13s, not a standalone before/after benchmark. |
| e7701c8b6 + 24ea3f72b | Mode PI_PACKAGE_DIR fixture copied only themes after 92bf3ebb1 correctly restored asset reads. Copy real export templates including nested vendor directory. | Frozen export-html/index.ts reads template assets; two unchanged RPC export assertions previously got ENOENT. suite_modes 214/0. |
| 260e31f92 | Cache test still treated retired samplingParams as a known typed field after c6761397e. Retain all four real known-field null rejection assertions; assert exact presence of unknown samplingParams:null after roundtrip. | Frozen types.ts:711–735 has no samplingParams. suite_modes 214/0; no production changes. |
| d40cc808b | RPC pipe-error child close at 200ms and second write at 300ms raced startup. Both writes could precede fd closure. Child now consumes first request, pauses/closes fd 0 and acknowledges stdin_closed; both drivers wait for it. | rpc-client.ts:129–135 rejects pending requests on stdin error. Same assertion and 2-second native deadline; full suite_modes 214/0. |
| 24d972c79 | Python write_text created then filled a watched diagnostic file; observer could correctly read empty text. Split creation/write gives deterministic RED. Atomically publish complete fixture bytes for the complete-diagnostic test. | Pyrun detached-evaluation.ts:520–525 returns generic error for empty files; separate empty-file test stays. suite_core_1 303/0, remaining-runner-publication-green.log. |
| ad9caa791 | Native/source descendant-prefix difference audited before classification. Both await exit plus EOF or rearmed 100ms idle grace. Approved fixture now signals complete descendant output before parent exit. | Frozen child-process.ts:16,81–97,116–126; native base child_process.rs:250–270,372–427; local bash uses shared streaming waiter. Exact equality retained; one-line race gap in bash trace. |
| cec1eb768 | Independent 50ms abort timers raced shell printf startup. Both drivers now abort on the first captured data callback, as main approved. | Abort result and exact output assertion unchanged; no timeout increase; suite_tools 173/0, remaining-bash-synchronized-green.log. |

No production behavior was changed. Source-file edits in rpc_mode.rs and storage agents.rs are inside tests. Every correction was formatted, passed targeted strict clippy where applicable, and passed `cargo check -j3 --workspace --all-targets` before commit. Logs `remaining-ISSUE-{fmt,clippy,check}.log` identify each scope. Changed TypeScript fixtures passed strict protected pinned-compiler checks; child MJS syntax passed. Rust/TS changed-line readability audits found no new suppressions or speculative abstractions.

Superseded development attempts remain honest RED evidence: flat export copy failed on vendor directory (remaining-modes-green.log, 50 setup failures); first pipe fixture used process.stdin.destroy(), which did not close fd 0 (remaining-pipe-ready-green.log, despite its filename). Both were corrected, then affected suites passed. No failed attempt is counted as proof.

### Historical instability retained, not claimed fixed

- **Artifact cleanup:** run 2 retained an old artifact once. b88fc55b0 adds failure-only path, source/native direction, lifecycle and current matching process-reference diagnostics; original assertion unchanged. Entire affected suite 473/0 and 15 exact six-case repeats passed, then full runs 3, 4 and acceptance passed this case. The diagnostic did not trigger again, so the original holder/root cause remains unknown; no /proc holder was established.
- **Frozen RPC surface:** run 4 hit the existing 20-second source observation deadline with empty stderr. The exact case and full affected facade lib (27/0) passed. b6f99b815 records order, exit status and before-import/UI-bound/observation-started stage on future failures; deadline and assertion condition unchanged. Acceptance passed, so no new failure stage was captured. This is instrumentation, not a claimed root-cause fix.

These do not invalidate the observed clean integrated acceptance, but a single clean acceptance run does not prove they cannot recur. No assertions were weakened, no new ignores were added, no timeout was raised, and no unexplained failure was suppressed.

### Final verification

Source inventory: `remaining-acceptance-inventory.log` PASS; 686 unique rows / 186679 locked source lines. Final ledger-only commit runs formatting and workspace all-target check with logs `remaining-final-{fmt,check}.log`; it does not change the accepted code. No push, deployment, network/live provider calls, stash or model delegation occurred. All requested command logs remain under `/tmp/pi-repin-final-gate/`.
