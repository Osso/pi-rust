# Session backend conformance factory

Source: `packages/agent/src/harness/session/testing/conformance.ts` at [source-lock](../source-lock.json).
Rust: `crates/pi-agent-core/src/harness/session/testing/{conformance,cases}.rs`.

## Export and resource scope

`createSessionBackendConformance` maps to public `create_session_backend_conformance`.
The returned factory-backed cases retain source group/name/order and are reusable by external backends.
Each awaited run creates one fresh fixture, runs assertions against its repository, and awaits disposal before returning.
Construction is lazy. Backend errors retain their concrete `SessionError` codes; assertion failures are returned after cleanup.
Factory failure performs no disposal. Cleanup failure after success propagates; cleanup failure after a case error preserves both failures in source suppression order.

Proof: `crates/pi-agent-core/tests/session_backend_conformance.rs` (five external-consumer tests),
`crates/pi-agent-core/tests/differential_session_conformance.rs`, and
`crates/pi-session-sqlite/tests/public_session_conformance.rs`.
The SQLite consumer uses existing `tests/support::Fixture`, closes its repository, and owns its temporary directory.
No backend storage implementation, dependency, live Pi state, or existing backend tests changed.

## Exact source cases

28 native cases, two approved typed exclusions. No representable source case missing.
The memory consumer runs all 28 twice (56 isolated fixtures); SQLite runs all 28.
The pinned helper differential executes the real public TS factory and all 30 TS cases, compares its full ordered manifest (including the two explicit exclusions), and compares the 28 applicable run/lifecycle outcomes. No event or backend-output fields are removed.

| Source # | Group | Exact source case name | Native item / proof |
|---:|---|---|---|
| 1 | entries and lanes | assigns parents and one sequence across every mutation | `cases::assigns_parents_and_one_sequence_across_every_mutation`; memory + SQLite + TS differential |
| 2 | records and log | commits records and lane moves as separate mutations | `cases::commits_records_and_lane_moves_as_separate_mutations`; memory + SQLite + TS differential |
| 3 | entries and lanes | rejects duplicate ids without changing state | `cases::rejects_duplicate_ids_without_changing_state`; memory + SQLite + TS differential |
| 4 | entries and lanes | isolates lanes while sharing the tree | `cases::isolates_lanes_while_sharing_the_tree`; memory + SQLite + TS differential |
| 5 | queries and facts | rejects invalid queries before empty reads | `cases::rejects_invalid_queries_before_empty_reads`; memory + SQLite + TS differential |
| 6 | queries and facts | supports bounded filtered and cursor-based queries | `cases::supports_bounded_filtered_and_cursor_based_queries`; memory + SQLite + TS differential |
| 7 | records and log | keeps lane names permanent with their recovery records | `cases::keeps_lane_names_permanent_with_their_recovery_records`; memory + SQLite + TS differential |
| 8 | records and log | persists queue cancellation without consuming its target | `cases::persists_queue_cancellation_without_consuming_its_target`; memory + SQLite + TS differential |
| 9 | records and log | filters records by lane type run sequence and order | `cases::filters_records_by_lane_type_run_sequence_and_order`; memory + SQLite + TS differential |
| 10 | records and log | filters operation starts by operation kind | `cases::filters_operation_starts_by_operation_kind`; memory + SQLite + TS differential |
| 11 | records and log | tracks and enforces one open operation per lane | `cases::tracks_and_enforces_one_open_operation_per_lane`; memory + SQLite + TS differential |
| 12 | records and log | does not let an earlier finish close a later start | `cases::does_not_let_an_earlier_finish_close_a_later_start`; memory + SQLite + TS differential |
| 13 | records and log | scopes open operations by lane and limit | `cases::scopes_open_operations_by_lane_and_limit`; memory + SQLite + TS differential |
| 14 | validation and immutability | returns immutable open-operation records | `cases::returns_immutable_open_operation_records`; memory + SQLite + TS differential |
| 15 | queries and facts | keeps latest-value facts and computes ledger statistics across lanes | `cases::keeps_latest_value_facts_and_computes_ledger_statistics_across_lanes`; memory + SQLite + TS differential |
| 16 | queries and facts | clears session names durably | `cases::clears_session_names_durably`; memory + SQLite + TS differential |
| 17 | validation and immutability | returns immutable copies from reads | `cases::returns_immutable_copies_from_reads`; memory + SQLite + TS differential |
| 18 | entries and lanes | validates lane lifecycle and targets | `cases::validates_lane_lifecycle_and_targets`; memory + SQLite + TS differential |
| 19 | entries and lanes | binds lane views without caching leaves | `cases::binds_lane_views_without_caching_leaves`; memory + SQLite + TS differential |
| 20 | entries and lanes | appends provisioned entries with their existing ids | `cases::appends_provisioned_entries_with_their_existing_ids`; memory + SQLite + TS differential |
| 21 | entries and lanes | persists tool-result termination decisions | `cases::persists_tool_result_termination_decisions`; memory + SQLite + TS differential |
| 22 | validation and immutability | rejects non-JSON entries before storage mutation | Typed exclusion; TS execution only |
| 23 | validation and immutability | rejects non-JSON records before storage mutation | Typed exclusion; TS execution only |
| 24 | entries and lanes | linearizes concurrent writes across two lanes | `cases::linearizes_concurrent_writes_across_two_lanes`; memory + SQLite + TS differential |
| 25 | repository and forks | creates lists and opens sessions | `cases::creates_lists_and_opens_sessions`; memory + SQLite + TS differential |
| 26 | repository and forks | deletes sessions idempotently | `cases::deletes_sessions_idempotently`; memory + SQLite + TS differential |
| 27 | repository and forks | forks one branch with selected facts and no records | `cases::forks_one_branch_with_selected_facts_and_no_records`; memory + SQLite + TS differential |
| 28 | repository and forks | forks a complete tree with lanes and facts | `cases::forks_a_complete_tree_with_lanes_and_facts`; memory + SQLite + TS differential |
| 29 | repository and forks | forks before an entry without modifying the source | `cases::forks_before_an_entry_without_modifying_the_source`; memory + SQLite + TS differential |
| 30 | repository and forks | validates the default fork target | `cases::validates_the_default_fork_target`; memory + SQLite + TS differential |

## Exact typed exclusions

Approved invalid-JS-runtime-input exclusion; no new deviation:

- Case 22's custom data `{value: undefined}`, `[undefined]`, `{value: 1n}`, `{value: NaN}`, `{value: new Map()}`, and a self-referential object cannot be constructed as `serde_json::Value`. The whole case is omitted from native registrations, not replaced with a passing no-op. Its valid-after-failure sequence assertion belongs to that unrepresentable-input case; no native input is substituted.
- Case 23's `tool_started.effectiveArgs` containing `{value: undefined}` or `{value: 1n}` likewise cannot be constructed. Same explicit whole-case exclusion.
- Case 5's negative `findOpenOperations` limit `-1` cannot be supplied to `Option<usize>`. Zero-limit validation, negative chronological cursor/log queries, and invalid operation-kind/type combinations remain executed. No other branch in case 5 is excluded.

Rust-owned values replace JS reference mutation; native read-copy assertions mutate returned records, entry data, log data, and reconstructed backend-native metadata, then verify unchanged stored values. Safe-integer timestamp bounds remain asserted. Concurrent lane cases poll all writes together and compare observed completion order to committed sequence order; they do not assume a fixed lane scheduling order.

No source discrepancy identified. No new dependencies.

## Development proof ledger

All commands run from `/home/osso/.worktrees/pi-rust-session-conformance` with `timeout 900`; each invocation's stdout/stderr saved once. Base revision: `d5b755f`.

| Command | Exact scope | Result / artifact |
|---|---|---|
| `cargo test -p pi-agent-core --test session_backend_conformance` | Initial consumer test, before public module | RED: E0432 missing `session::testing`; `/tmp/session-conformance-red.log` |
| Same command | Initial native implementation, before missing import corrections | Compile failure; `/tmp/session-conformance-green-1.log` |
| Same command | Import-corrected initial implementation | GREEN: 4/4; `/tmp/session-conformance-green-2.log`; superseded by later source changes |
| `cargo test -p pi-agent-core --test differential_session_conformance` | Differential consumer before oracle entrypoint | RED: missing oracle module / EOF; `/tmp/session-conformance-differential-red.log` |
| `cargo test -p pi-agent-core --test session_backend_conformance --test differential_session_conformance` | Full source assertions + initial helper differential | GREEN: 5/5; `/tmp/session-conformance-green-3.log`; superseded by typed-error propagation changes |
| `cargo test -p pi-session-sqlite --test public_session_conformance` | Initial downstream adapter and 28 public cases | GREEN: 1/1; `/tmp/session-conformance-sqlite-green-1.log`; core error-propagation changes require final targeted rerun |
| `cargo test -p pi-agent-core --test session_backend_conformance case_failure_still_disposes` | New concrete backend-error preservation assertion, before propagation fix | RED: backend error converted to assertion text; `/tmp/session-conformance-error-red.log` |
| `cargo test -p pi-agent-core --test session_backend_conformance --test differential_session_conformance` | Fallible case bodies, concrete errors preserved, assertion cleanup test | GREEN: 6/6; `/tmp/session-conformance-green-4.log`; subsequent private metadata passthrough removal/import cleanup awaits final targeted proof |

Slice commit precedes final targeted tests. Final proof is reported by commit/revision in the delivery message; no broad/check/clippy/readability/final-gate/deployment runs are authorized for this slice.
