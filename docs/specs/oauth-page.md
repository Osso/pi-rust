# OAuth result pages

Port frozen `packages/ai/src/utils/oauth/oauth-page.ts` at `4f9e6f8a4`. [Traceability](../porting/traceability/oauth-page.md) maps the exports.

## What it must do

- [x] Return the exact frozen success/error HTML, including headings, styles, logo and whitespace.
- [x] Escape ampersands, angle brackets and both quote characters in source order; omit absent or empty details.
- [x] Interpolate escaped text once. Literal template-marker text in messages/details must remain text.

## How it works

- [Module traceability](../porting/traceability/oauth-page.md).

## Implementation inventory

- `crates/pi-ai/src/auth/oauth/oauth_page.rs` — page rendering.

## Tests asserting this spec

- `crates/pi-ai/tests/oauth_page.rs` — escaping and optional details.
- `crates/pi-ai/tests/differential_oauth/mod.rs` — complete HTML comparisons with frozen source.

## Known gaps (current cycle)

None within the approved scope.

## Out of scope

Lone UTF-16 units beyond the existing scalar input carrier follow the approved 2026-10-25 gap policy. No gap applies to literal template-marker text.
