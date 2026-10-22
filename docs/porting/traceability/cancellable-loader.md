# components/cancellable-loader.ts traceability

## Q2 frozen revalidation

Frozen components/cancellable-loader.ts audited whole. Existing exported methods, constructor defaults, rendering/input branches and invalidation match the frozen implementation within typed numeric/String boundaries; native target plus differential_components re-run against 4f9e6f8a4. Loader frame intervals remain explicitly driver-owned poll/deadline adaptation; CancellableLoader maps AbortSignal to CancellationToken and preserves repeated cancel callbacks. See [Q2 proof](../q2-tui-components-proof.md).


Source: `packages/tui/src/components/cancellable-loader.ts` (40 lines). Rust:
`crates/pi-tui/src/components/cancellable_loader.rs`.

Tests: `crates/pi-tui/tests/differential_components.rs::cancellable_loader_matches_typescript`
(escape/ctrl+c abort, message updates, render lines, render count).

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class CancellableLoader extends Loader` | `CancellableLoader` (composes `Loader`) | differential |
| `constructor(ui, spinnerColorFn, messageColorFn, message?, indicator?)` | `CancellableLoader::new(...)` | differential |
| `onAbort` | public `on_abort` | differential |
| `signal` (AbortSignal) | `signal() -> CancellationToken` | differential (`is_aborted`) |
| `aborted` | `is_aborted()` | differential |
| `handleInput` | `Component::handle_input` | differential |
| `dispose` | `dispose()` (stops the loader) | differential stop at end |
| inherited `render`/`stop`/`setMessage`/`setIndicator` | delegating methods | differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| cancel keybinding → abort controller + `onAbort` | `handle_input` | differential |
| abort idempotence (`AbortSignal` stays aborted) | `CancellationToken` | differential (two cancel keys) |
| loader rendering/message updates | delegation to `Loader` | differential |

## Behavioral notes

- `AbortSignal` maps to `tokio_util::sync::CancellationToken` (workspace
  convention). `signal()` returns a clone that callers can await.
- `dispose()` stops the animation timer, matching the source.

## Gaps / exclusions

- `Loader`'s driver-owned frame timer applies here too; differential does not
  advance frames.

## Discrepancies

None found.
