# kill-ring.ts traceability

Source: `packages/tui/src/kill-ring.ts` (46 lines). Rust: `crates/pi-tui/src/kill_ring.rs`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class KillRing` | `struct KillRing` | `tests/kill_ring.rs`; `tests/differential_kill_ring.rs` |
| `push(text, opts)` | `push(&mut self, text: &str, prepend: bool, accumulate: bool)` | push/accumulate tests, differential |
| `peek()` | `peek(&self) -> Option<&str>` | `push_adds_new_entries_and_peek_returns_most_recent`, differential |
| `rotate()` | `rotate(&mut self)` | `rotate_moves_last_entry_to_front`, differential |
| `get length` | `len(&self) -> usize` | all tests, differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| `if (!text) return` | `if text.is_empty() { return; }` | `empty_text_is_ignored` |
| `accumulate && ring.length > 0` | `accumulate && !self.ring.is_empty()` | `accumulate_appends_forward_deletions`, `accumulate_prepends_backward_deletions` |
| prepend vs append merge | `if prepend { format!("{text}{last}") } else { ... }` | both accumulate tests |
| new entry | `else` branch | `push_adds_new_entries_and_peek_returns_most_recent` |
| `peek` on empty ring | `Vec::last()` | `empty_text_is_ignored` |
| `rotate` when `length > 1` | `if self.ring.len() > 1` | `rotate_is_a_no_op_for_zero_or_one_entry`, `rotate_moves_last_entry_to_front` |

## Behavioral notes

- `opts.accumulate` is optional in TypeScript; Rust takes an explicit `bool` (typed API excludes `undefined`).
- `is_empty()` is an additional Rust convenience; it has the same value as `len() == 0` and adds no new state.

## Gaps / exclusions

- JavaScript callers passing non-string `text` are excluded by the typed Rust API (decision 2026-08-12).

## Discrepancies

None.
