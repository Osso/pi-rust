# Frozen tree-session checkpoint

Source: [common pin](../source-lock.json), `packages/agent-core/src/harness/session/`.

## Representation and approvals

`SessionTreeEntry` keeps a native `JsObject` as authoritative storage; `known()` and `message()` are typed read projections. Unknown entry types/fields, absent versus null arbitrary payloads, and unpaired UTF-16 units survive persistence. Frozen v3 trees replace old v4 lanes, sequence counters, operation records, retained-tail records and test conformance scaffolding; no compatibility path remains.

Approved by user 2026-10-22 (parent coordination): session getters return owned copies; JavaScript mutation-through-returned-reference is not ported. Frozen consumer audit found no built-in caller depending on such mutation (`session-identity-consumers.txt` and `session-identity-contexts.txt` in `/tmp/pi-rust-repin-delta-agent-core/`). Arbitrary external listeners remain outside this approval's parity claim.

## Proof matrix

| Capability | Proof | Remaining |
|---|---|---|
| Memory append/index/leaf/path/labels | `repin_tree_session`; `differential_tree_session::frozen_storage_facade_and_fork_contracts_match` | Cyclic malformed parent graphs are not exercised (frozen traversal does not terminate). |
| Session facade message/thinking/model/tools/custom/custom-message/label/name/compaction/move/branch/context | Same differential, native surrogate text and null details | Broader nested/multiple-compaction context matrix pending. |
| Fork selection before/at/current/missing/empty ID | Same differential | Repository orchestration mapped separately. |
| JSONL header/entry validation, parse failures and native unknown payloads | `frozen_jsonl_validation_and_raw_round_trips_match`, 15 file fixtures | Injected filesystem failure matrix pending. |
| Provider failure and raw custom message disk close/reopen | `lossless_provider_errors::provider_failure_survives_jsonl_repo_close_and_reopen` | — |
| Memory/JSONL repository list/open/fork/delete/replacement, native payloads and equal-date ordering | `differential_tree_repository` RED→GREEN | Injected filesystem failures and invalid-file list filtering pending. |

DISCREPANCY preserved: duplicate IDs stay in the append log while the index takes the latest value. RED: old storage rejected the second append (`tree-session-red.log`); GREEN: `repin_tree_session`. Context compaction lookup separately preserves frozen first-ID lookup behavior; no uniqueness repair added.

Checkpoint proof artifacts: `/tmp/pi-rust-repin-delta-agent-core/tree-session-green.log`, `tree-differential.log`, `tree-native-boundaries-dev.log`. No final full-suite or whole-package parity claim.
