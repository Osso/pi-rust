# Storage successor checkpoint — 2026-10-25

Current follow-up: [storage-3 module audits and reciprocal proof](storage-3-proof.md). The checkpoint below remains its own bounded historical evidence.

Branch: `repin/core-tools`; baseline `c839fda09`; code through `65e23c966`. Frozen source remains the unmodified [common pin](source-lock.json). No merge, push, deployment, live provider or child agent was used. History work remains deferred. This is bounded persistence evidence, not whole-module acceptance.

## Implemented boundaries

| Boundary | Current behavior | Proof |
|---|---|---|
| Spool / manager | Archives decode in process without package assets; exact errors, exclusive0600 plaintext, callback and cleanup remain. | Missing-package source/native RED→GREEN; `repin_storage_spool`6 PASS. `repin_session_storage`27 PASS including native90s timeout, excluding5 deferred history/memory cases. Actual frozen FIFO rejection90052ms; Node still alive at external100s timeout. [Approved runtime gap and commands](traceability/session-transcript-spool.md#in-process-worker-boundary-2026-10-25). |
| Recipient persistence | TS enqueue→native claim/accept→TS read and native enqueue→TS claim/accept→native reopen preserve receipts/messages and delete rightful claims. | `suite_core_4 repin_storage_mailbox_compat`1 PASS; `agent_session_runtime_mailbox`10 PASS. The original fixture's foreign-process claim deletion was invalid under frozen ownership checks; production ownership remains unchanged. |
| Store callbacks / finalization | Result update listeners log original throws locally and continue. Reserved finalization propagates ownership/coordinator/drain errors instead of returning false or ignoring a drain rejection. | Listener parent1/three children compare source/native logs and state. Actual source finalizer + SQLite claim trigger vs native faux prompt/steering reproduces RED→GREEN after durable finalization. Store28, persistent-store3 and store-operation differential1 PASS. [Catch and finalizer trace](traceability/terminal-outbox-delivery.md). |
| Control persistence | Current reciprocal producer/read/mutation and selected legacy migrations match frozen. The former public surrogate-body RED now passes unchanged. | `suite_core_4 repin_control_reciprocal`8 PASS; `repin_control_migrations`4 PASS; `canonical_mailbox_source_surrogate_body_survives_native_projection_and_claim`1 PASS. No additional source edit or waiver. |
| Supervisor fixture | Private KB namespace plus ready real resident IPC and durable request/response replace recursive libtest autostart. Exact Continue./sender/details assertions remain. | `session_coordination_tools`16 PASS plus its ignored child invoked by the parent. The child includes six actual source/native advisory cases. No provider-backed service starts. Prompt-order regression1 PASS. |

Cargo commands use `cargo test -j4 -p pi-coding-agent --test <target> <filter> -- --nocapture`. Manager excludes `frozen_compacted_archive_history`, `persisted_history_tool`, `compacted_reverse_loading`, `storage_memory_probe_child` and `large_unicode_compacted_history`. No workspace/full-suite run was performed. Exact commands, revisions and invalidation notes: `/tmp/storage2-proof-ledger.md`; final outputs: `/tmp/storage2-*-final.log`. Earlier RED/GREEN artifacts use the same prefix.

## Gates and remaining limits

`cargo fmt --all --check` passes. Four changed oracle fixtures pass protected strict no-emit TypeScript compilation with the shared unmodified-source builder, and protected Biome checking. Manual changed-line Rust/fixture readability review found no new structural issue; numerical metrics were unavailable (`rust-code-analysis-cli` absent).

Strict clippy is **not clean**: dependency lint fails on7 existing base diagnostics; primary storage-only `--no-deps --lib -- -D warnings` reports74 diagnostics, none on changed lines. Core default scoped lint compiles with28 existing warnings. The ten touched coding-agent test targets compile under default scoped clippy; the one new fixture borrow warning was fixed in `65e23c966`. The final affected mailbox-target lint has zero changed-line findings. Complete structured outputs: `/tmp/storage2-{core,tests}-clippy.jsonl`, `/tmp/storage2-storage-strict-final.jsonl`, `/tmp/storage2-finalize-clippy-final.jsonl`. No warning suppression or unrelated lint repair was added.

Source inventory passes:686 unique source rows,186679 locked lines; statuses unchanged467 done/19 gap/47 partial/153 todo. Group-wide schema/export/branch acceptance, injected runtime crash/fsync/CAS coverage and independent final integration remain open. The former packaged-executable spool blocker, public-body counterexample and reserved-finalize error sink are no longer current blockers. [Control/store contract](../specs/control-store-repin.md) retains the broader completion requirements.
