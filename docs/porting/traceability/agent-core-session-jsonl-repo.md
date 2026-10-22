# jsonl-repo.ts

Source: common-pin `agent-core/src/harness/session/jsonl-repo.ts:1–177`.
Rust: `harness/session/jsonl/repo.rs`.

| Export/branch | Proof |
|---|---|
| `JsonlSessionRepo.create/open`, lazily resolved session root, native payload persistence | `lossless_provider_errors::provider_failure_survives_jsonl_repo_close_and_reopen` |
| `list/delete/fork`, missing open, repeated delete, cwd filter, equal-date order and replacement | `differential_tree_repository`; native entries, parentSessionPath and first-kept fork paths compared exactly |

Status: partial: injected filesystem failures and invalid-file list filtering still pending. GREEN `tree-repository-green.log`. [Storage proof and shared representation](agent-core-tree-session.md).
