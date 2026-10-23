# repo-utils.ts

Source: common-pin `agent-core/src/harness/session/repo-utils.ts:1–51`.
Rust: `harness/session/repo_utils.rs`.

| Export/branch | Proof |
|---|---|
| `createSessionId`, `createTimestamp` | Frozen deterministic clock/UUID storage differential |
| `getEntriesToFork`: current leaf, before/at target, missing target, empty ID | `differential_tree_session::frozen_storage_facade_and_fork_contracts_match` |

[Shared representation and proof artifacts](agent-core-tree-session.md). Repository orchestration proof tracked in its owning modules.

[Native IO promise-rejection propagation](harness-nodejs-env.md#native-io-rejection-carrier) preserves rejected Node failures separately from returned errors/diagnostics; deleted-cwd direct frozen caller matrix is the proof.

Repository orchestration is complete in `differential_tree_repository`: explicit-ID and automatic-ID memory/JSONL create/fork/open/list/delete workflows compare exact metadata, IDs, entries, ordering and errors. GREEN `session-repository-auto-id-proof.log`; generated samples come from a fresh frozen module instance. Result/throw mapping remains typed and no compatibility wrapper was added.
