# Baseline triage successor proof — 2026-10-25

Scope: `/tmp/repin-triage-2-prompt.txt`; branch `repin/baseline-triage`, starting at `146eb5622`. Code/proof scope ends at `c09bcaff9`; later documentation-only changes do not invalidate it. All commands use this worktree, `-j3`, and bounded targets. No agents, push, deployment, live providers, frozen edits or merges.

## Baseline target disposition

All 38 assigned targets are recorded individually in [the baseline ledger](test-layout-baseline-failures.md#successor-baseline-recheck-2026-10-25): 22 already pass at the parent baseline, three frozen-absent targets are retired, eight retained failures are repaired here, five failures were explicitly transferred by main. Retired targets are not claimed as tests. AI, interactive lib, coordination and version-check are excluded as assigned.

| Repaired target | Exact boundary / proof |
| --- | --- |
| differential_core_leaves | lossless config-key fixture serialization; `/tmp/triage2-core-leaves-green2.log` (1 repaired test; other 15 already passed) |
| example_qna | frozen ModelRegistry fixture API; `/tmp/triage2-final-example_qna.log` (2 tests) |
| differential_config_lossless_package | approved NUL env-name difference asserted separately, all realistic fields exact; `/tmp/triage2-config-gap-proof.log` (1 test plus existing child helper) |
| differential_interactive_components | no obsolete dispose call; full renderer state; identical finishedAt input; `/tmp/triage2-final2-differential_interactive_components.log` (1 repaired test; other 10 already passed) |
| differential_tree_selector | global keybinding fixture isolation; `/tmp/triage2-final-differential_tree_selector.log` (5 tests plus existing child helper) |
| example_bash_spawn_hook | remove old-only prompt metadata; `/tmp/triage2-bash-spawn-green.log` (1 test) |
| main_process | pinned package bytes and frozen absent AI_AGENT expectation; `/tmp/triage2-suite_core_3-green.log` (19 tests passed) plus `/tmp/triage2-final-suite_core_3.log` (remaining repaired test passed) |
| raw_editor | existing approved editor delta selected with HEAD/hash verification; `/tmp/triage2-final-suite_interactive.log` (2 tests) |

Transferred, not fixed here: compaction cut-point (agent_267); supervisor request/service (agent_265); remote compaction/enrich shutdown handler (agent_278). Their integration evidence belongs to those owners.

## Example-module coverage

All 17 initially todo/partial `coding-agent/examples/**` rows are now done, each linked to an individual module trace or existing single-module trace. Native/runtime proof remains bounded; no whole-workspace acceptance is inferred.

| Module group | Source/behavior covered | Current proof |
| --- | --- | --- |
| SDK02/09/12 | model selection/output/disposal; four auth/session configurations; explicit no-discovery options | `/tmp/triage2-example_sdk_{custom_model,api_keys_and_oauth,full_control}-audit.log`: 4/1/3 tests; `/tmp/triage2-sdk-empty-final.log` adds the exact empty-availability branch (1 test) |
| custom Anthropic / GitLab | complete registration, OAuth, conversion/transport, stream lifecycle and error forwarding | `/tmp/triage2-example_{custom_anthropic,gitlab_duo}-audit.log`: 13/6 tests; GitLab child helper is existing |
| WAD, modal, Qna, questionnaire, reload, subagent, RPC UI | whole-module symbols/branches in linked row traces; filesystem, actual child/RPC transport and native UI fixtures retained | per-module `/tmp/triage2-*-audit.log`; modal `/tmp/triage2-suite_tool_data-final-dev.log` (2); Qna (2); questionnaire `/tmp/triage2-questionnaire-frozen-final.log` (2); baseline reload (2), subagent (6) |
| plan-mode | commands/tools/context/choice/completion/restoration; exact generated raw todo UI/messages/JSONL | RED `/tmp/triage2-plan-lossless-red2.log`; final `/tmp/triage2-current-example_plan_mode.log` (3) |
| handoff/summary/status/overlay | existing raw text through conversation selection/prompt generation; valid block-array JS interpolation; no passive fixture projection | RED `/tmp/triage2-conversation-text-red3.log` and `/tmp/triage2-message-blocks-red.log`; final conversation 2, handoff 1, summary 1, status 1, overlay 10 |

New conversation tests are modules of `suite_tool_data`, not new test binaries. `differential_coding_agent_truncate::oracle` visibility widened mechanically to reuse the existing suite helper. Existing native extension, ownership, locale and wire-default approvals remain explicit in traces; no new semantic deviation or dependency.

## Final checks and proof validity

- `cargo fmt --all --check`: PASS before every logical commit.
- Targeted warning-mode clippy: `/tmp/triage2-clippy-proof.log` (baseline targets) and `/tmp/triage2-all-examples-clippy-final.log` (17 relevant test roots), both exit 0; `/tmp/triage2-sdk-empty-clippy.log` additionally checks the final SDK02 fixture. Existing warnings remain; comparing first diagnostic locations with exact `git diff --unified=0 146eb5622 HEAD` changed lines finds zero changed-line warnings. No warning suppression was added.
- Strict pinned tsgo of the new conversation oracle through the protected source-tsconfig builder: `/tmp/triage2-conversation-oracle-typecheck.log`, exit 0.
- Manual Rust/TS changed-line readability audit: existing carriers and pure transformations used, explicit boundary names/errors, no new deep nesting, suppressions, fallback paths or runtime infrastructure. Existing large fixture/oracle bodies were not redesigned.
- `python3 scripts/source-inventory.py`: `/tmp/triage2-inventory-final.log`, PASS 686 unique rows / 186679 locked lines; 550 done / 21 gap / 39 partial / 76 todo. No example todo/partial rows remain.

The final current tests were only rerun after relevant source/fixture changes. Initial unchanged-module proofs remain valid. Full workspace tests, strict final clippy and independent integration verification remain main-owned; this lane did not run or claim them.
