# components/alt-screen-flash.ts traceability

Source: `packages/tui/src/components/alt-screen-flash.ts` (51 lines). Rust:
`crates/pi-tui/src/components/alt_screen_flash.rs`.

Tests: `crates/pi-tui/tests/differential_components.rs::alt_screen_flash_matches_typescript`
(render before/after flash, truncation, dispose, render count).

## Exported symbols

| TypeScript export | Rust item | Tests |
|---|---|---|
| `class AltScreenFlashContainer` | `AltScreenFlashContainer` | differential |
| `constructor(requestRender)` | `AltScreenFlashContainer::new` | differential |
| `flash(message, durationMs = 1000)` | `flash`, `flash_default`, `flash_at` | differential |
| `dispose` | `dispose` | differential |
| `invalidate` | `Component::invalidate` (no-op) | n/a |
| `render(width)` | `Component::render` | differential |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| flash appends entry + requests render | `flash_at` | differential render count |
| message truncated with `""` ellipsis and wrapped in reverse video | `render` | differential width 12 |
| timer removal (`setTimeout`, unref) | driver-owned `poll(now)` + `expires_at` | gap: no listed TS test; expiry not differentially compared |
| dispose clears all entries | `dispose` | differential |
| negative duration clamped to 0 | `Duration` (typed non-negative) | typed exclusion |

## Behavioral notes

- Entries render in insertion order, each `\x1b[7m {message} \x1b[27m`.
- Expiry is driver-owned: `poll(now)` removes entries whose deadline passed and
  requests a render. The differential compares only synchronous flash/render/
  dispose behavior because the oracle cannot control timers.

## Gaps / exclusions

- Expiry timing is not differentially compared; no listed TS test covers it.
- JavaScript callers passing negative durations are excluded by the typed API.

## Discrepancies

None found.
