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

The first routing integration run (`04b1f65a1`, `/tmp/repin-agent-session-compaction-oracle-refactor.log`) is not a passing gate: 16 passed /7 failed /1 private child probe ignored. Seven inherited native scenarios seeded only two context messages, so the re-pinned protected-three-message suffix returned “session too small” before hooks/provider/callback assertions. The common native seed now matches the actual-source fixture's concrete seven-message history; redundant per-test seeding is removed. All original outcome assertions remain intact.

Correction: the first seed edit was rejected (non-unique repeated loop); `5b6ac13ef` contains only the preceding diagnosis note, not the native seed change. Its unchanged-source re-execution is invalid/redundant evidence. The subsequent fixture change applies all three exact edits and checks the edit result before formatting/commit.

## Committed checkpoint: auto-hook boundaries and fixture integration

- Native changes: `5ead5d3c0` runner identity/ancestor installation; `f23fb799a` auto-hook abort/persistence and elapsed duration. Two auto-hook cases fail at the actual native boundary before the fix and pass after it; source outcomes are asserted independently before native comparison.
- `f000f8917`: common seven-message native seed matches frozen-scenario setup; all original assertions retained. `cargo test -j3 -p pi-coding-agent --test agent_session_compaction_extensions -- --nocapture`: **23 passed**, private child probe ignored by default and explicitly run by the passing callback-history test. Log `/tmp/repin-agent-session-compaction-seed-applied.log`.
- `f000f8917` bounded routing rechecks: cycle1, tree5, steer1, export1, quota1 passed. Exact commands/results: `/tmp/repin-agent-session-refactor-results.json` and its per-scope logs. `428f83fd8` extracts only baseline fixture orchestration; original multi-turn differential passes1 (`/tmp/repin-agent-session-baseline-extraction.log`). These oracle-only changes do not invalidate native compilation/clippy proof.
- Warning-mode `cargo clippy -j3 -p pi-coding-agent --test agent_session_compaction_extensions --message-format=json` at `f000f8917`: exit0, 496 inherited diagnostics, **zero primary diagnostics on lines added since60589c908**. `/tmp/repin-agent-session-final-clippy.{jsonl,stderr}`. Not a workspace warning-free gate.
- Strict no-emit pinned-source compilation: agent-session, services, coding-agent compaction and branch-summarization pass4, using the shared builder and existing ES2024 no-emit approval for frozen TUI `/v`; `/tmp/repin-agent-session-source-typecheck.log`. This is source-module proof, not a claim that the dynamic runtime oracle harness is strictly typed.

Readability: inspected changed native identity/cache/abort/duration paths and fixtures; no new warning suppression or new deep production nesting. `rust-code-analysis-cli` is unavailable, so manual review plus TypeScript parser-assisted function/branch inspection was used. Named scenario helpers keep each fixture below200 body lines; the remaining245-line baseline/setup combination was split without changing its statements and re-proven at428f83fd8. Existing large source-shaped agent-session methods remain; no whole-file readability/whole-module completion claim. Biome formatted changed TypeScript fixtures and `cargo fmt --all --check` passed before every code commit. Inventory at428f83fd8: PASS686 rows /186679 lines; coverage remains484 done /21 gap /50 partial /131 todo. Agent-session remains **partial**; services and the two coding-agent compaction modules remain closed at their recorded checkpoints.

## Prompt steering row set after parent de46a2789

Parent de46a2789 fast-forwarded cleanly (it already contains the prior native fix). Current row commits:8dbfe43ac actual-run completion boundary;0262b1d83 real frozen persistent-runtime differential;037e29fc0 shared-oracle import and parent JsString API cleanup. `cargo check -j3 --workspace --all-targets` passes at each new Rust scope (latest `/tmp/repin-agent-session-prompt-clean-workspace-check.log`). The focused runtime-mailbox integration target passes12 at0262b1d83 (`/tmp/repin-agent-session-prompt-mailbox-integrated.log`); after mechanical fixture cleanup the changed differential passes1 again (`/tmp/repin-agent-session-prompt-final-green.log`). Production prompt behavior and other12-test proof scope are unchanged by that cleanup. Actual frozen source-only final projection also passes (`/tmp/repin-agent-session-source-prompt-lifecycle-final.log`).

Warning-mode targeted clippy exits0 after cleanup (`/tmp/repin-agent-session-prompt-clippy-final.{jsonl,stderr}`);510 inherited diagnostics remain and the new prompt fixture has zero warnings. The three fixture warnings from the first post-merge check were fixed, not suppressed. Manual changed-line Rust/TS readability audit: named effectful fixture helper, one shared oracle module, no new production nesting/API/carrier, no suppression; no whole-file readability claim. fmt/Biome pass. Inventory after parent merge:686 rows /186679 lines,609 done /25 gap /29 partial /23 todo. Agent-session whole-module audit stays partial.

Image-content proof follow-up at3b1f4f3b6 exposed a fixture startup race, not a value mismatch: one of two source children failed with “database is locked” in frozen configureSharedSqliteDatabase before AgentSession construction completed. Both requests inherited one sandbox-global default control DB. The oracle now sets each SessionManager's explicit per-request control DB before constructing the frozen AgentSession. This isolates independent fixtures using the real source configuration boundary; no source SQLite retry, process serialization, assertion relaxation or product workaround is added. `/tmp/repin-agent-session-images-content-green.log` is failed/incomplete evidence and is superseded only after the isolated run passes.

After per-request DB isolation, the expanded user-content assertion caught another fixture mismatch: the native timeout-extension helper activates all tools by default while the source image fixture explicitly disables them. Repeated plain-stop Faux replies therefore produced an end-turn correction in native content. Both image tests now explicitly disable active tools, matching source and keeping this fixture focused on image propagation. No correction behavior or assertion is weakened; the exact four user-content arrays remain required. `/tmp/repin-agent-session-images-isolated.log` is not passing evidence. Original baseline differential passes after DB isolation (`/tmp/repin-agent-session-isolated-baseline.log`).

## Optional-image row set proof

At376a48910, both actual-frozen core image tests pass2: omitted/empty/populated input-handler values; absence/empty/transform-to-empty/populated before-agent-start values and exact four user-message contents. `/tmp/repin-agent-session-images-final-green.log`. Real RPC process proof passes1 at488941fa0 with identical image-report bytes and normalized protocol output (`/tmp/repin-agent-session-images-rpc-green.log`); later edits did not change RPC code or its fixture. Original multi-turn baseline passes1 after source DB isolation (`/tmp/repin-agent-session-isolated-baseline.log`). Failed expanded probes remain documented above, not counted as passing evidence.

Latest Rust scope passes `cargo check -j3 --workspace --all-targets` before commit (`/tmp/repin-agent-session-images-tools-workspace-check.log`). fmt/Biome pass. Final focused agent-session clippy exits0, with zero diagnostics on added Rust lines since503d24efb; earlier combined agent-session/modes/harness clippy likewise exits0 with zero added-line diagnostics. Manual Rust/TS changed-line audit found no new warning suppression, carrier abstraction, deep production nesting or hidden side-effect helper. Initial print/interactive callers are mechanically propagated and compile; no new mode-level extension-event proof is claimed. Inventory remains686 rows /186679 lines (609 done /25 gap /29 partial /23 todo). Whole-module row remains partial until both audit halves close.

## Saved-compaction event row set

Code at9221088e4 restores awaited saved-entry events and frozen controller lifetime; parent-approved field serialization reuses the existing tagged SessionEntry codec. Four actual-frozen gated cases cover manual/auto success and cancellation. Payloads are compared as complete serialized ExtensionEvent envelopes after only explicit ID/time/duration normalization; original frozen records decode/re-encode through the native event type. All four pass again at009176c4d (`/tmp/repin-agent-session-saved-events-final-green.log`). Full focused compaction-extension integration at9221088e4 passes27 with one private child probe ignored at top level and exercised by the passing callback-history test (`/tmp/repin-agent-session-saved-events-integrated.log`). Later edits affect only the test gate's equivalent conditional, docs, and spec status.

Workspace all-targets compilation passes for the final Rust scope before commit (`/tmp/repin-agent-session-saved-events-final-workspace-check.log`). fmt/Biome pass. Final targeted clippy exits0 with zero primary diagnostics on added lines since4daf7ec8c (`/tmp/repin-agent-session-saved-events-final-clippy.{jsonl,stderr}`). Changed-line manual readability audit: explicit async emission and matching-controller cleanup; pure field serializer; no new carrier/API/dependency or suppression. The frozen first-summary lookup discrepancy is classified/preserved, not silently fixed. No whole agent-session completion claim.

## Active-tool manual continuation row set

At2f07e4896, the corrected actual-frozen tool-lifetime differential passes1 (`/tmp/repin-agent-session-manual-tool-green.log`). A real registered tool remains pending while compaction aborts its actual signal; the test then releases it and awaits both original prompt and compaction. Both runtimes now report saved-event retry=true and compaction-end retry=true, make exactly2 provider requests, and finish with “resumed.” The earlier native-only exclusion gave retry=false, one request and empty final text; genuine RED is red5, not the setup/compiler probes.

`cargo check -j3 --workspace --all-targets` passes for this exact Rust scope before commit (`/tmp/repin-agent-session-manual-tool-workspace-check2.log`). fmt/Biome pass. Targeted clippy exits0 with zero added-line primary diagnostics since30732d7fe (486 inherited diagnostics; `/tmp/repin-agent-session-manual-tool-clippy.{jsonl,stderr}`). Manual changed-line readability audit found no suppression, new carrier/API or added production branch; the obsolete parameter and suppression were removed with the exclusion. Inventory686/186679 passes; whole agent-session remains partial.

## Bash lifecycle checkpoint, 2026-10-25

Native REDs: `/tmp/repin-agent-session-bash-audit-red.log` reports command starting with a newline and signals `[true,true]` instead of `[false,true]`; `/tmp/repin-agent-session-bash-lifetime-red.log` reports commit observer `[false]` instead of `[true]`. After the source slot/finalizer/prefix fix, `agent_session / bash_parity::` passes3 in `/tmp/repin-agent-session-bash-audit-green.log`. Scope: current bash implementation and these three tests; later unrelated edits do not invalidate this proof. No new frozen differential was added under the parent lighter-proof directive.

## Management and thinking lifecycle checkpoint, 2026-10-25

`management_lifecycle::` RED2 in `/tmp/repin-agent-session-management-red4.log`: naming listener remains empty at return; materialization authentication leaves isCompacting=true. Earlier red/red2 are compiler setup failures; red3's materialization case used a registered scripted auth override and reached generation, so it is not auth-finalizer evidence. The corrected fixture changes the active/checkpoint provider together to an unconfigured provider and confirms the actual auth error.

`/tmp/repin-agent-session-management-green.log` passes6: the two new native cases plus existing frozen missing-index, sync-prefix, pending remainder and real-effort cases. Real effort proves proactive mode, delegation tool and active status before return. A subsequently removed unused import does not change runtime scope. This closes the previous thinking GREEN-pending notes; no new frozen differential was added.

## HTML renderer checkpoint, 2026-10-25

`agent_session / html_renderers::` fails1 with a null rendered-tool payload before the fix (`/tmp/repin-agent-session-html-renderers-red.log`) and passes1 after retaining the registered definition (`/tmp/repin-agent-session-html-renderers-green.log`). The real turn executes a custom tool; the decoded actual HTML export must contain its call renderer and both collapsed/expanded result renderers. No persistence codec changes or new frozen matrix.

## Tool-result relocation checkpoint, 2026-10-25

`agent_session / tool_relocation::` genuine RED1 is `/tmp/repin-agent-session-tool-relocation-red2.log`: only `agent-end:relocation=false`, instead of prepare/persisted → relocation end → activation. Earlier red.log is a fixture compiler error, not parity evidence. GREEN1 is `/tmp/repin-agent-session-tool-relocation-green.log`; the prepare callback yields, observes the persisted actual tool result, and the complete public flow uses exactly one provider call. The fix invokes the already-existing relocation API; serialization is unchanged.
