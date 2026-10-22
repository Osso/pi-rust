# jsonl-storage.ts

Source: common-pin `agent-core/src/harness/session/jsonl-storage.ts:1–293`.
Rust: `harness/session/jsonl/{storage,types}.rs` plus shared tree state.

| Export/branch | Proof |
|---|---|
| `JsonlSessionStorage.create/open`, header/entry parsing and required field validation | `differential_tree_session::frozen_jsonl_validation_and_raw_round_trips_match`, 15 file fixtures |
| Empty/blank input, malformed JSON, wrong version, invalid ID/parent/leaf | Same differential |
| Native unknown entry payload round trip | Same differential |
| Append, disk close/reopen and provider-error payload | `lossless_provider_errors::provider_failure_survives_jsonl_repo_close_and_reopen` |

[Shared representation and discrepancy](agent-core-tree-session.md). Partial: injected filesystem failure/append-order matrix pending.
