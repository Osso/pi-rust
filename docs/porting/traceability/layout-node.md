# layout-node.ts traceability

Source: `packages/tui/src/layout-node.ts` (51 lines). Rust:
`crates/pi-tui/src/layout_node.rs`.

Tests: `crates/pi-tui/tests/layout.rs` and
`crates/pi-tui/tests/differential_layout.rs` (all layout scenarios go through
this contract).

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `LAYOUT_NODE` symbol | removed; `Component::layout_node` default method | layout/differential |
| `LayoutViewport` | `LayoutViewport { width, height }` | layout tests |
| `StackLayoutEntry` | `StackLayoutEntry` | layout/differential |
| `StackLayoutNode` | `StackLayoutNode { kind, entries, gap, align }` | layout/differential |
| `ScrollLayoutState` | `ScrollState` (in `components/scroll_view.rs`) | scroll tests/differential |
| `ScrollLayoutNode` | `ScrollLayoutNode { component, state }` | layout/differential |
| `LayoutNode` | `enum LayoutNode { Stack, Scroll }` | layout/differential |
| `LayoutComponent` | `trait LayoutComponent` | layout/differential |
| `getLayoutNode` | `get_layout_node` | layout/differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| `typeof candidate[LAYOUT_NODE] === "function"` | `Component::layout_node` returns `Option` | layout/differential |
| stack node construction (`type`, `entries`, `gap`, `align`) | `Stack::layout_node` | layout/differential |
| scroll node construction (`component`, `state`) | `ScrollView::layout_node` | layout/differential |

## Behavioral notes

- The symbol-keyed method is replaced by a trait method. Components opt in by
  overriding `Component::layout_node`; the default is `None`, matching the
  source's absent-symbol path.
- `ScrollLayoutState` is implemented by the shared `ScrollState`; `layout.rs`
  drives layout through `Rc<RefCell<ScrollState>>` instead of holding a
  `ScrollView` reference, which avoids self-referential handles.

## Gaps / exclusions

- None in behavior. The `LayoutBox.parent` back-reference from `layout.ts` is
  not modeled; see `layout.md`.

## Discrepancies

None found.
