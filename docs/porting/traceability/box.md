# components/box.ts traceability

Source: `packages/tui/src/components/box.ts` (137 lines). Rust:
`crates/pi-tui/src/components/box.rs` (`BoxComponent`; `Box` collides with the
Rust prelude type).

Tests: `crates/pi-tui/tests/differential_components.rs::box_render_matches_typescript`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class Box` | `BoxComponent` | differential |
| `constructor(paddingX = 1, paddingY = 1, bgFn?)` | `BoxComponent::new(padding_x, padding_y, bg_fn)` | differential |
| `children` | `children()` | differential |
| `addChild` / `removeChild` / `clear` | `add_child` / `remove_child` / `clear` | differential (add only) |
| `setBgFn` | `set_bg_fn` | differential background-change case |
| `invalidate` | `Component::invalidate` | not directly asserted |
| `render(width)` | `Component::render` | differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| no children → `[]` | early return | differential empty-child case |
| children render at `max(1, width - paddingX*2)` with left pad | `render` | differential |
| no child lines → `[]` | early return | empty-child case |
| cache match on width/child lines/bg sample | `match_cache` | differential repeated render |
| `setBgFn` does not invalidate; sample detects change | `set_bg_fn` + `bg_sample` | differential background-change case |
| background padding vs plain padding | `apply_bg` | differential |
| top/bottom padding lines | `render` | differential |

## Behavioral notes

- The background callback is sampled with `"test"` and the sample participates
  in cache validation, matching the source.
- Differential cases compare plain and background boxes, vertical padding,
  empty children, and a background change after the first render.

## Gaps / exclusions

- `removeChild`/`clear`/`invalidate` are implemented but not covered by the
  listed tests (box.ts has no TS test file).
- JavaScript callers passing negative padding are excluded by the typed API.

## Discrepancies

None found.
