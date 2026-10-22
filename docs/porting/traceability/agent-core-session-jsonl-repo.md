# jsonl-repo.ts

Source: common-pin `agent-core/src/harness/session/jsonl-repo.ts:1–177`.
Rust: `harness/session/jsonl/repo.rs`.

| Export/branch | Proof |
|---|---|
| `JsonlSessionRepo.create/open`, lazily resolved session root, native payload persistence | `lossless_provider_errors::provider_failure_survives_jsonl_repo_close_and_reopen` |
| `list/delete/fork`, missing-file/open failures, sort/cwd filters | Implemented; direct repository differential pending |

Status: partial. [Storage proof and shared representation](agent-core-tree-session.md). No full repository parity claim.
