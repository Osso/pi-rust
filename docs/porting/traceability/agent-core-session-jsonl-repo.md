# jsonl-repo.ts

Source: common-pin `agent-core/src/harness/session/jsonl-repo.ts:1–177`.
Rust: `harness/session/jsonl/repo.rs`.

| Export/branch | Proof |
|---|---|
| `JsonlSessionRepo.create/open`, lazily resolved session root, native payload persistence | `lossless_provider_errors::provider_failure_survives_jsonl_repo_close_and_reopen` |
| `list/delete/fork`, missing open, repeated delete, cwd filter, equal-date order and replacement | `differential_tree_repository`; explicit/automatic IDs, native entries, parentSessionPath and first-kept fork paths compared exactly |
| Invalid-file filtering, symlink/directory/extension/header-only behavior and invalid-date order | `session_inventory`, four list modes over83accepted metadata rows and invalid/ignored fixtures |

Status: complete against the common pin, with shared approved owned-getter and stack exclusions. Invalid-file inventory filtering and Date/NaN ordering are proven below. GREEN `tree-repository-green.log`. [Storage proof and shared representation](agent-core-tree-session.md).

`session_inventory` uses80dated/invalid-date files plus invalid headers, empty files, version/parent validation failures, directory.jsonl, case-sensitive/temporary extensions, a header-valid/body-invalid file, a file symlink, a root-level ignored file and a symlinked directory. It compares all/cwd/missing/empty-cwd lists to the direct frozen module with actual filesystem reads and raw UTF-16 metadata IDs.

RED `session-inventory-red.log` had the same83-ID multiset but different ordering: Rust sort_by cannot reproduce V8 sorting with Date.parse NaN comparisons. Native list now uses the shared pinned js_array_sort with the unchanged source date-difference comparator. GREEN `session-inventory-green.log`, four complete inventories. DISCREPANCY retained: header validation accepts nonempty invalid date strings, so the comparator is non-total and the frozen order need not be globally chronological; invalid dates are not repaired or filtered. The header-only listing also intentionally includes a session whose later entry lines are invalid.


## Filesystem failures and partial fork writes

`session_jsonl_faults` injects returned not_found/permission_denied records and native TypeError rejections at root resolution, directory/file-path joins, mkdir, create/write, exists, open/read, root and child listDir, metadata/readLines, remove and both fork appends. JoinPath has only the typed returned-error boundary; native promise-rejection cases exercise all other selected methods. Exact error records and complete resulting filesystem bytes match direct frozen execution. Post-effect write/remove failures and first/second append failure prove no fabricated rollback: a failed fork can leave its header or partial entry prefix on disk. Successful controls cover create/open/delete/list/cwd-list/fork. GREEN `session-jsonl-fault-rejections.log`; no production failure-path changes needed.
