# Agents-core closure — 2026-10-25

The successor closed `coding-agent/extensions/agents-core/src/runtime.ts` (3,326 lines) against frozen `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`. Current source symbols/branches, source discrepancies, runtime gaps and command/revision ledger are in the [runtime trace](traceability/agents-core.md). This replaces the pending handoff.

## Delivered

- Pending background construction-failure UTF-16 regression is GREEN; all three existing oracle fixtures pass protected strict no-emit checking. The duplicate deterministic fixture type member was removed; no new frozen scenarios or matrices were added.
- Factory/browser/model-registry/archive/artifact-root/cancellation-timeout/recovery and the remaining runtime were audited. Removed the unused source-absent parent-journal API/module/wrappers/constants and four obsolete tests; historical JSONL readers outside this runtime are unchanged.
- Lifecycle mirror errors now reach the store's exact visible observer diagnostic. Resume reservation errors now propagate after attachment admission instead of returning successful Attached output. Each has a native failing behavioral regression and GREEN proof.
- Whole-module trace and runtime spec are current; coverage runtime row is done. Locked counts remain 686 rows / 186,679 lines. Existing control-DB/mailbox reciprocal proof remains authoritative and unchanged.

## Revision and proof

Code commits: `2daca5441`, `6a8c5ade5`, `795776802`. Parent `1e9e576a7` merged at `e97b6f691`; later closure edits are documentation only. Each code tip and the merged tree passed `cargo check -j3 --workspace --all-targets`.

Focused proof: 52 runtime/viewer/deterministic tests; 19 factory tests; five drift tests (the one ordinary ignored test is a subprocess helper invoked by its parent); one real faux-provider child; six artifact tests; two persisted wait-watcher tests. See the trace for exact commands, logs and invalidation scope. The real-child test passed again after the parent AgentSession merge. Warning-mode clippy passes with existing warning debt and zero warnings on successor changed lines. Formatting, source inventory and manual changed-line readability pass.

No agents/models, PLAN.md commit, live providers, network, push or deployment. Main owns workspace-wide/independent final acceptance; no lane branch cleanup is requested while the merge train is active.
