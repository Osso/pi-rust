# components/input.ts traceability

## Q2 frozen revalidation

Frozen input.ts (465 lines) audited whole. Input/options/callbacks/focus; get/setValue, input routing, paste state, submit/cancel/undo/delete/kill/yank/movement/Kitty/control rejection, masking/window/cursor rendering map to input.rs and existing native/differential input matrix. Width<=2 returns only `> `; added width2 native/frozen lifecycle regression, killed off-by-one mutant. Rust-only cursor restoration hook supports reconstructed extension widgets. UTF8 byte cursor and grapheme positions remain typed representation boundaries, not arbitrary JS UTF16 split-string parity. [Q2 proof](../q2-tui-components-proof.md).


Source: `packages/tui/src/components/input.ts` (465 lines). Rust:
`crates/pi-tui/src/components/input.rs`.

Tests: `crates/pi-tui/tests/input.rs` (every listed TS test ported),
`crates/pi-tui/tests/differential_input.rs` (13 scripted edit/render scenarios
against the pinned TypeScript component).

## Final-gate fixture isolation (2026-10-25)

Consolidation `e60b2edbe` moved this differential into `suite_core` with terminal negotiation fixtures. Those fixtures set the process-global Kitty flag, suppressing legacy Alt+d/Alt+y while the fresh frozen oracle starts with Kitty disabled (`keys.ts:25,1162–1165`). The unchanged 13-case assertion failed in the composed suite but passed alone (`/tmp/pi-repin-final-gate/tui-input-isolated.log`). Restore the differential's standalone Cargo target so both sides start with the same terminal state. Production, oracle, cases, and assertions remain unchanged; neither failure is in the documented baseline flake list.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `interface InputOptions` | `InputOptions { mask }` | mask test, differential |
| `class Input implements Component, Focusable` | `struct Input` | all |
| `constructor(options?)` | `Input::new(InputOptions)` | all |
| `onSubmit` / `onEscape` | public `Option<InputSubmitFn>` / `Option<InputEscapeFn>` | submit/escape paths, differential |
| `focused` | `Focusable::focused` / `set_focused` | CJK/mask render tests |
| `getValue` | `get_value` | all |
| `setValue` | `set_value` | all |
| `handleInput` | `Component::handle_input` | all |
| `invalidate` | `Component::invalidate` (no-op) | source has no cached state |
| `render(width)` | `Component::render` | render tests, differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| bracketed paste start marker resets buffer | `handle_paste_chunk` | `undoes_paste_atomically`, differential |
| paste end marker extracts content + reprocesses remainder | same | paste differential |
| `tui.select.cancel` → `onEscape` | `handle_input` | differential escape case |
| undo keybinding | `undo` | undo tests, differential |
| submit keybinding or `"\n"` | `handle_input` | submit test |
| backspace / forward delete by grapheme | `previous_grapheme_start`, `next_grapheme_end` | undo tests, differential |
| delete word backward/forward with kill-ring accumulation | `delete_word_backwards`, `delete_word_forward` | kill-ring tests |
| delete to line start/end (prepend/append) | `delete_to_line_start`, `delete_to_line_end` | kill-ring tests |
| yank / yank-pop rotation | `yank`, `yank_pop` | kill-ring tests, differential |
| cursor left/right by grapheme | `handle_input` | differential CJK case |
| cursor line start/end | `handle_input` | differential |
| cursor word left/right | `find_word_backward` / `find_word_forward` | unicode word tests |
| Kitty CSI-u printable decode | `decode_kitty_printable` | differential `\x1b[97u` |
| control-character rejection (C0, DEL, C1) | `has_control_characters` | differential `\u{1}` |
| word-character undo coalescing | `last_action == TypeWord` | undo tests |
| whitespace starts a new undo unit | `is_whitespace_char` | `undoes_spaces_one_at_a_time` |
| masked value/cursor rendering | `get_masked_value`, `get_masked_cursor` | mask test, differential |
| horizontal scrolling (near start/end/middle, end-of-value reservation) | `render` window calculation | CJK render tests, differential |
| fake cursor + hardware cursor marker | `CURSOR_MARKER` when focused | CJK render tests |
| prompt-width guard (`availableWidth <= 0`) | `checked_sub` early return | not reachable with the listed widths |

## Behavioral notes

- The TypeScript cursor is a UTF-16 code-unit index; Rust uses byte indices.
  Ordinary movement tests cover astral/CJK graphemes, but `setValue` can preserve
  an offset inside a replacement scalar or grapheme; this is not covered by
  those movement tests. Independent F1 reproduces an invalid UTF8 cursor.
- The masked display maps each grapheme to `•` (3 UTF-8 bytes) but cursor
  columns are recomputed through `slice_by_column`, matching the source's
  one-cell-per-bullet behavior.
- `Number.isFinite`/negative-width callers are excluded by the typed API.

## Gaps / exclusions

- `render` widths smaller than the prompt are typed as `usize`; the
  `availableWidth <= 0` guard exists but the listed tests never reach it.

## Discrepancies

F1 remains open: `handle_input("é"); set_value("界"); render(10)` panics because
replacement retains a byte offset instead of the frozen UTF16 offset. Source
also permits `a` → `😀`, leaving cursor UTF16 index1 inside a surrogate pair;
render separates the halves with ANSI bytes, and inserting `!` yields a value
with lone surrogates. Current Rust String API cannot express those strings.
User chose “Widen Input to UTF-16” on 2026-10-21; no representation gap or approximation is approved. Native migration/proof is in progress; this historical checkpoint is not current acceptance. Production is
unchanged; mixed BMP/astral/combining regression patch and exact frozen outputs
are saved under `/tmp/pi-rust-repin-tui-components-fix/` (see REPORT.md).
