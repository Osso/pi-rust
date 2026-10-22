# components/dynamic-border.ts and spacer.ts traceability

## Q2 frozen revalidation

Frozen components/dynamic-border.ts audited whole. Existing exported methods, constructor defaults, rendering/input branches and invalidation match the frozen implementation within typed numeric/String boundaries; native target plus differential_components re-run against 4f9e6f8a4. Loader frame intervals remain explicitly driver-owned poll/deadline adaptation; CancellableLoader maps AbortSignal to CancellationToken and preserves repeated cancel callbacks. See [Q2 proof](../q2-tui-components-proof.md).


Sources: `packages/tui/src/components/dynamic-border.ts` (15 lines),
`spacer.ts` (28 lines). Rust:
`crates/pi-tui/src/components/dynamic_border.rs`, `spacer.rs`.

Tests: `crates/pi-tui/tests/dynamic_border.rs` (all three TS tests ported),
`crates/pi-tui/tests/differential_components.rs::spacer_dynamic_border_and_truncated_text_match_typescript`.

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class DynamicBorder` | `DynamicBorder` | all |
| `constructor(format = identity)` | `DynamicBorder::default()`, `DynamicBorder::new(format)` | TS tests + differential |
| `invalidate` | `Component::invalidate` (no-op) | n/a |
| `render(width)` | `Component::render` | all |
| `class Spacer` | `Spacer` | differential |
| `constructor(lines = 1)` | `Spacer::new(lines)`, `Default` | differential |
| `setLines` | `set_lines` | differential |
| `render` | `Component::render` | differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| border width `max(1, width)` | `width.max(1)` | `renders_at_least_one_cell` |
| custom formatter applied to the whole border | `BorderFormatter` | `applies_a_custom_formatter` |
| spacer renders `lines` empty strings | `vec![String::new(); lines]` | differential |
| `setLines` changes subsequent renders | `set_lines` | differential |

## Gaps / exclusions

- None; both modules are fully covered by the listed tests plus differential.

## Discrepancies

None found.
