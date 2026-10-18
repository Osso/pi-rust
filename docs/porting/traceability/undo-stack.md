# undo-stack.ts traceability

Source: `packages/tui/src/undo-stack.ts` (28 lines). Rust: `crates/pi-tui/src/undo_stack.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class UndoStack<S>` | `struct UndoStack<S>` | `tests/undo_stack.rs` |
| `push(state)` | `push(&mut self, state: &S)` where `S: Clone` | `push_stores_a_clone_and_pop_returns_it`, `mutating_the_source_after_push_does_not_change_the_snapshot` |
| `pop()` | `pop(&mut self) -> Option<S>` | `pop_on_empty_stack_returns_none`, `pop_returns_snapshots_in_last_in_first_out_order` |
| `clear()` | `clear(&mut self)` | `clear_removes_all_snapshots` |
| `get length` | `len(&self) -> usize` | all tests |

## Branches

The source has no conditionals. `push` always clones; `pop` always removes the last element; `clear` truncates to zero.

## Behavioral notes

- `structuredClone` is replaced by `S: Clone`. The Rust trait is the caller-supplied deep copy, matching the source contract that pushed snapshots are detached from the caller's value. `tests/undo_stack.rs` proves mutation of the source after `push` does not affect the stored snapshot.
- `is_empty()` is an additional Rust convenience with the same value as `len() == 0`.

## Gaps / exclusions

- Differential testing is not applicable: the TypeScript contract is defined over `structuredClone` for arbitrary JavaScript values, which has no single Rust type. Behavior is covered by Rust unit tests over a concrete `Clone` snapshot type.
- JavaScript callers passing values that `structuredClone` rejects are excluded by `S: Clone`.

## Discrepancies

None.
