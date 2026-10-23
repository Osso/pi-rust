# jsonl-storage.ts

Source: common-pin `agent-core/src/harness/session/jsonl-storage.ts:1–293`.
Rust: `harness/session/jsonl/{storage,types}.rs` plus shared tree state.

| Export/branch | Proof |
|---|---|
| `JsonlSessionStorage.create/open`, header/entry parsing and required field validation | `differential_tree_session::frozen_jsonl_validation_and_raw_round_trips_match`, 15 file fixtures |
| Empty/blank input, malformed JSON, wrong version, invalid ID/parent/leaf | Same differential |
| Native unknown entry payload round trip | Same differential |
| Append, disk close/reopen and provider-error payload | `lossless_provider_errors::provider_failure_survives_jsonl_repo_close_and_reopen` |

[Shared representation and discrepancy](agent-core-tree-session.md). Status: complete against the common pin, with the shared approved owned-getter and stack exclusions.

[Native IO promise-rejection propagation](harness-nodejs-env.md#native-io-rejection-carrier) preserves rejected Node failures separately from returned errors/diagnostics; deleted-cwd direct frozen caller matrix is the proof.

## Numeric native projection audit

`session_number_decode` opens real files containing overflow JSON numbers and negative zero, then compares raw entries/context against the frozen module with pre-wire numeric tags. Native user/assistant/tool timestamp/usage fields are asserted before output serialization; custom/compaction fields remain in the raw AST. RED found AgentMessage stringify/decode collapsing numbers and silently discarding valid typed payloads; [shared correction](js-json.md#typed-union-and-checked-adapter-follow-up). This is three numeric read/context scenarios, not closure of the remaining injected-filesystem-failure matrix.

## Filesystem failure and state-order proof

`session_jsonl_faults` compares direct frozen storage/repository executions with actual native files. It injects returned not_found/permission_denied failures and rejected TypeErrors with unmatched UTF-16 messages at create/write, open/read, metadata/readLines, append and leaf append. Successful controls run through every operation. Failure records retain exact name/code/message/cause and paths; rejected Errors propagate without SessionError wrapping.

Mutation cases compare live entries/leaf/label, reopened entries/leaf/label, exact disk bytes before retry, retry outcome and post-retry state. Failures injected after real write/append prove disk side effects precede in-memory updates: failed persistence does not mutate the live tree, while reopening sees bytes already written. Retry retains the source duplicate-entry/leaf behavior; no rollback or repair is added. GREEN `session-jsonl-fault-rejections.log`; production storage needed no change. Returned-failure-only predecessor logs are `session-jsonl-faults-green{1,2}.log`.
