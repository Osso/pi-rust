# components/stack.ts, v-stack.ts, h-stack.ts traceability

Sources: `packages/tui/src/components/stack.ts` (154 lines),
`v-stack.ts` (33), `h-stack.ts` (44). Rust:
`crates/pi-tui/src/components/stack.rs`, `v_stack.rs`, `h_stack.rs`.

Tests: `crates/pi-tui/tests/layout.rs` (grow/shrink/minimum/gap/zero-width/
composition), `crates/pi-tui/tests/differential_layout.rs` (allocation,
alignment, nesting, scroll composition).

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `StackEntryOptions` | `StackEntryOptions` (+ `apply_to`) | layout tests |
| `StackEntry` | `StackEntry` alias of `StackLayoutEntry` | layout tests |
| `StackChild` | `StackLayoutEntry` / `ComponentRef` (typed core) | layout tests |
| `StackOptions` | `StackOptions { gap, align }` | layout tests |
| `abstract class Stack` | `Stack` | layout/differential |
| `Stack.addChild/removeChild/clear` | `Stack::add_child/remove_child/clear` | not exercised by listed tests |
| `visibleStackEntries` | `visible_stack_entries` | gap/invisible differential |
| `allocateStackSizes` | `allocate_stack_sizes` | layout/differential |
| `VStack` | `VStack` | layout/differential |
| `HStack` | `HStack` | layout/differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| `normalizeSize` (undefined/non-finite → fallback, floor, clamp 0) | typed `usize` options | excluded invalid inputs |
| entry options copied only when provided | `Option` fields | differential |
| `isStackEntry` (`"render" in child`) | not needed: entries are typed | n/a |
| vstack render: measure at viewport width, slice/pad to sizes | `VStack::render` | gap test, differential |
| hstack render: intrinsic widths, allocate, re-render, align offsets, composite | `HStack::render` | compose/zero-width, differential |
| zero width hstack child paints nothing | `HStack::render` empty rendered lines | zero-width test |
| `clampSize` min/max | `clamp_size` | shrink/min tests |
| grow candidates and weights | `is_candidate`, `weight` | grow tests |
| shrink weighted by current size | `weight(Shrink)` | shrink test |
| capacity clamps and no-progress guard | `distribute` | layout/differential |
| basis `"auto"`/undefined uses intrinsic | `StackBasis::Auto`/`None` | differential |
| gap total and natural height | `layout_vstack`/`VStack::render` | gap test |

## Behavioral notes

- TypeScript's `StackChild` union (`Component | StackEntry`) is not modeled;
  callers wrap plain components in `StackLayoutEntry::new`.
- `maxSize` defaults to `usize::MAX`, matching `Number.MAX_SAFE_INTEGER` for the
  integer domain the typed API accepts.

## Gaps / exclusions

- `addChild`/`removeChild`/`clear` exist but are not covered by the listed TS
  tests; no differential scenario mutates a stack after construction.

## Discrepancies

None found.
