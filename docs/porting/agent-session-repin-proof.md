# Agent-session lane proof ledger

Frozen reference: `4f9e6f8a4`, unchanged. Branch: `repin/agent-session`. All Cargo commands run from `/home/osso/.worktrees/pi-rust-repin-agent-session` with `-j3 -p pi-coding-agent`, under the configured test sandbox. No live providers, push or deployment. Parent `4eff32510` was merged as directed.

## Current capability scope

| Module | State |
| --- | --- |
| agent-session-services.ts | Whole-module audit complete; defaults, construction/reload/registration/flags and session forwarding mapped |
| coding-agent compaction.ts / branch-summarization.ts / index.ts | Whole-module audit complete; threshold, protected cut, preparation, generation, materialization and result formats mapped |
| agent-session.ts | Partial: committed model/queue/retry/tree/export/compaction fixes below; constructor/tool hooks, runtime coordination, background/idle compaction and remaining lifecycle branches still require complete audit |

## Commands and evidence

Every test entry abbreviates `cargo test -j3 -p pi-coding-agent --test <target> <filter> -- --nocapture`. Logs retain full commands/output via Pyrun capture; red logs are development evidence, not acceptance.

| Revision | Target / filter | Result and scope | Log under `/tmp/` |
| --- | --- | --- | --- |
| a5da9b412 | agent_session_services / all4 | GREEN4; actual-frozen empty/absent/explicit agentDir and ordered flag diagnostics | repin-agent-session-services-green.log |
| 748d4005b | agent_session / cycle_model | GREEN2; auth-only explicit scope, missing catalog model, provider/id equality and forward/backward cycling | repin-agent-session-cycle-green.log |
| 1bfcf9b8a | agent_session / quota_fallback_visits | GREEN1; three account-owned provider streams, ordered visit once and final model | repin-agent-session-quota-green2.log |
| c184b3e1f | agent_session / publishes_multi_agent_activity_across_agent_and_tool_events | GREEN1; provider/tool/store publication includes shared argument detail | repin-agent-session-activity-green.log |
| 88eefa03d | agent_session / idle_steer_returns | GREEN1; provider gate proves nonblocking steering return against frozen | repin-agent-session-steer-green.log |
| 92ee0b301 | agent_session_compaction_extensions / manual_compaction_preflight | GREEN2; cancel-only preflight and no legacy summary override | repin-agent-session-preflight-green.log |
| 3e7e4507b | agent_session_compaction_extensions / resumed_manual_compaction | GREEN1; streaming input accepted before resumed provider completes | repin-agent-session-compaction-lock-green.log |
| 92260f731,474fbdfdf | agent_session / navigate_tree_summary_requires_auth; navigate_tree_empty_extension | GREEN1 each; required auth; empty summary/label persistence | repin-agent-session-tree-auth-green.log; repin-agent-session-tree-empty-green.log |
| 7a7997515 | agent_session / jsonl_export_directory_target | GREEN1 with3 paths; complete Node filesystem error fields and unchanged existing file bytes | repin-agent-session-export-green.log |
| d7d1cfcc1 | agent_session / navigate_tree_current_leaf | GREEN1; no-op navigation while provider is active, leaf captured before abort persists a response | repin-agent-session-tree-streaming-green2.log |
| 0b0f33e1a | agent_session_compaction_extensions / overflow_recovery_resets | GREEN1; two requests recover independently; persisted identity beats timestamp0 | repin-agent-session-recovery-flags-green2.log |
| b48c48228,04bb3f5fc | suite_session_1 / differential_should_compact; differential_find_cut_point_and_turn_start | GREEN1 each; percentage/model/reserve precedence, protected suffix and custom turn classification | repin-compaction-threshold-green.log; repin-compaction-cut-green.log |
| 94a7fb9a5,055e9d35b | agent_session_compaction_extensions / percentage_threshold_counts | GREEN1; exact usage anchor plus trailing custom content triggers preflight, not late post-run compaction; follow-up removes unused assignment without changing behavior | repin-compaction-threshold-trailing-green.log |
| 86fb54ca7 | suite_session_1 / compaction:: |36 passes and1 stale-budget fixture failure; source implementation already matches frozen | repin-compaction-helpers-focused.log |
| 45bbbe64e | suite_session_1 / protected_suffix_ignores_old_budget | GREEN1; corrected obsolete fixture, exact index/start/split plus equality for both ignored budgets | repin-compaction-cut-unit-green.log |
| 86fb54ca7 | suite_session_1 / compaction_generation:: | GREEN15; native history/update/split/materialization and frozen generation scenarios | repin-compaction-generation-focused.log |
| 45bbbe64e | suite_core_1 / branch_summarization:: | GREEN14; native collection/preparation/generation | repin-compaction-branch-native-green.log |
| 58cab8a2e | suite_session_1 / differential_compaction_generation:: | GREEN4 grouped matrices: branch8, summary4, compact4, materialization4; complete requests and result/error shapes | repin-compaction-generation-final.log |
| 0671e013f | suite_session_1 / differential_last_assistant_usage | GREEN1 with3 vectors; skip aborted/error/zero usage and retain last valid usage | repin-compaction-last-usage.log |

## Invalidation and final checks

Production compaction generation last changed at `3e5afeb9f`; later fixture extensions/import/lock changes do not invalidate the runtime result evidence above. Session percentage calculation last changed at `055e9d35b`; the expression-only warning fix preserves the tested calculation. Older session matrices cover disjoint source paths; no whole-session acceptance claim follows from them.

Warning-mode `cargo clippy -j3 -p pi-coding-agent-core --lib --message-format=json` at `58cab8a2e`: exit0,225 inherited warnings, zero changed-line findings. Targeted package Clippy for `suite_session_1`, `suite_core_1`, `agent_session`, `agent_session_services`, and `agent_session_compaction_extensions` exposed two new fixture warnings; lexical lock scoping and oracle type reuse fixed them. Final affected suite Clippy at `880c13455`: exit0, zero changed-line findings against `4eff32510`. Full diagnostics: `repin-agent-session-clippy2.*`, `repin-agent-session-test-clippy{,2,3}.*`. No warning suppression was added.

`cargo fmt --all --check` passed before every commit. Source inventory passes686 files/186679 lines with locked counts. No workspace suite, denied-warning final gate or independent verification was run in this lane; main owns integration.

## Mechanical cross-lane fallout

- Removed obsolete preflight `compaction` from `tools/core/extensions/types.rs` and its cancellation fixture, as explicitly approved by parent.
- Removed `CompactionResult.usage` constructor fields in remote-compaction and interactive test fixtures; removed the old root usage-summary export.
- Renamed summary options in lossless/acquisition fixtures; propagated the existing message identity into `agent_session/restriction.rs` without changing restriction decisions.

Two regex_replace tool attempts failed before writing because this lane is outside the harness cwd. Exact literal edits were then applied through the edit tool with absolute paths. No source worktree was changed.

## Oracle fixture readability follow-up

The agent-session oracle's added scenarios are extracted into named helpers in `oracle/agent-session-scenarios.ts`; the root keeps session setup and its original multi-turn scenario. Extraction uses the pinned TypeScript parser only on this repository's fixture statements, never on frozen source. Request/result projections and scenario bodies are unchanged; Biome formatting is applied to the changed fixtures. The auto-hook fixture clears its session cell after the hook to avoid a self-reference cycle. This routing refactor requires bounded scenario/legacy re-execution; earlier source/runtime behavior evidence is retained but not substituted for that compilation/execution proof. Planned checks: the focused compaction-extension target, original legacy differential, and the changed cycle/tree/steer/export/quota scenarios.
