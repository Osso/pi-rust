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
