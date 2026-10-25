# Goal review evidence

Goal review inputs follow the [frozen module](../porting/source-lock.json), mapped in [traceability](../porting/traceability/goal-review-evidence.md).

## What it must do

- [ ] Retain nonempty non-extension input as durable evidence and latest per-session userRequest; a new request clears prior userAnswers.
- [ ] Retain successful nonblank end_turn reasons untrimmed and blocking ask_questions text answers, excluding errors, queued and cancelled results.
- [ ] Merge request/answer context and nonempty conversationEvents into review payloads with frozen overwrite behavior and exact UTF-16.
- [ ] Return the reviewed evidence count; consume only that prefix of the same goal, preserving late evidence and clearing the field when empty.

## How it works

- [Module traceability](../porting/traceability/goal-review-evidence.md)

## Implementation inventory

- `crates/pi-coding-agent-extensions/src/bundled_extensions/goal/goal_review_evidence.rs` — controller and evidence assembly.
- `crates/pi-coding-agent-extensions/src/bundled_extensions/goal/goal_state.rs` — existing durable goal boundary.

## Tests asserting this spec

`crates/pi-coding-agent/tests/goal_review_evidence.rs`; `oracle/goal-review-evidence-oracle.ts`.

## Known gaps (current cycle)

- [ ] Execute focused GREEN after the implementation commit.

## Out of scope

Independent async scheduling races and adversarial input shapes follow the shared parity exclusions.
