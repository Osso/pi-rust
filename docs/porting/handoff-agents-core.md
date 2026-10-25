# Agents-core handoff — 2026-10-25

Stopped by main's agent-cap instruction. Worktree `/home/osso/.worktrees/pi-rust-repin-agents-core`, branch `repin/agents-core`. Parent `1ebc46773db099b8898cdb082c2a57c3060d9ff8` is merged. No pushes, deployment, live providers or storage-internal changes.

## Remaining row

`coding-agent/extensions/agents-core/src/runtime.ts` remains **partial** in coverage.md. All other agents-core source rows were already closed. The successor read the whole frozen runtime and repaired the predecessor's open list plus additional realistic branch differences. Do not mark this row done from this handoff alone.

## Implemented

- Factory parent registry, ask_questions/browser read augmentation, change_working_directory exclusion, configurable browser root and archive restore; removed attached settings/policy override.
- Removed recovery journal admission. Added factory artifact-root routing and configurable cancellation settlement. Propagated child transcript, terminalization, acknowledgement and construction-publication failures.
- Wait delivery receipts now defer mailbox acceptance; bridge waits release claims. Coordination prompt and captured terminal order are preserved. Fixed mailbox target error text, empty target-session/list filters, bridge callback scope, detached-terminal mirroring and old header-parent fallback.
- Requested-profile validation, guidance ordering and initial model/effort metadata match frozen public spawn. Model fallback uses the parent model and medium default effort, clamped to capabilities.
- Last compiling change preserves exact UTF-16 in background construction-failure UI notifications and propagates failed-background publication errors. Its failing frozen/native test observed U+D800 becoming U+FFFD. **GREEN for this last change is still pending.**

## Proof and current limits

- Current merged tree: `cargo check -j3 --workspace --all-targets` PASS, `/tmp/agents-core-2-merged-check.log`. This includes the last background change. Rust and oracle formatting ran before this check.
- Before the last parent merge: 54 targeted agents_core tests PASS (`/tmp/agents-core-2-final-runtime.log`); 19 factory tests plus real child end-to-end PASS (`/tmp/agents-core-2-final-factories2.log`); five drift tests PASS with one subprocess-only ignored helper (`/tmp/agents-core-2-final-factories.log`), and the background UI lifetime case passed again (`/tmp/agents-core-2-background-lifetime.log`). Two persisted wait-watcher tests passed (`/tmp/agents-core-2-wait-order-green.log`). Six artifact tests passed earlier (`/tmp/agents-core-2-artifact-green.log`).
- Warning-mode targeted clippy passed before the last merge/change: `/tmp/agents-core-2-extensions-clippy.log`, `/tmp/agents-core-2-targeted-clippy.log`. Existing crate/suite warnings remain; newly introduced err-expect/borrow/test-module placement warnings were repaired. No warning suppressions added.
- Strict protected oracle typecheck passed before the last oracle scenario: `/tmp/agents-core-2-oracle-typecheck.log`. The new backgroundFailure scenario needs the same scoped no-emit check. The oracle uses actual unmodified frozen modules; scenario extraction was exercised by the 54-test/factory proof.
- Last RED: `/tmp/agents-core-2-background-utf16-red.log`; last native fix is committed with this handoff. Detailed earlier RED/GREEN artifacts are listed in agents-core.md and the untracked PLAN.md proof ledger. `rust-code-analysis-cli` is unavailable; changed code was read manually.

## Next steps

1. Run the new background UTF-16 test to GREEN and the protected strict oracle typecheck. After the parent merge, run only affected focused targets, not full suites: `suite_core_1 agents_core`, `agents_core_session_factories`, `agents_core_end_to_end`, relevant drift and artifact subsets. Consult existing proof before rerunning unchanged scope.
2. Finish current-revision targeted warning-mode clippy/readability. New compile/format checks remain required before each commit. The whole-module reading is complete; do not reopen pathological/OS-fault/numeric/runtime-race hunting excluded by AGENTS.md.
3. Replace historical/stale sections of `docs/porting/traceability/agents-core.md` with a current whole-runtime symbol/branch map. It still contains old split-module headings and obsolete open/gap statements. Preserve valid earlier source-backed proofs. `docs/specs/agents-core-runtime.md` exists; its browser checkbox can be checked from the passing actual factory read differential.
4. Run `python3 scripts/source-inventory.py`, preserve locked source counts, update coverage.md runtime.ts to done only after the remaining proof and trace cleanup, commit and report the finished row set to main. No independent agents were spawned.
