# Interactive login options and command

Frozen authority: `interactive-mode.ts:6706–6751,6753–6809,7013–7022` at common pin4f9e6f8a4.

- `/login` displays the two fixed authentication choices, subscription then API key. Selecting a choice opens its provider selector; canceling that provider selector returns to the method selector.
- `/login <id>` accepts only an exact OAuth provider ID after command argument trimming. Unknown IDs display `Unknown OAuth provider: <id>` with unchanged UTF-16 text. No name matching, case folding, API-key shortcut or fuzzy fallback.
- Login provider options retain each OAuth provider and each eligible distinct model provider. OAuth/API-key options for the same ID remain distinct. Optional auth-type filtering precedes stable display-name locale sorting.
- API-key eligibility follows the frozen built-in display-name map, then excludes other built-ins and OAuth-only custom IDs.
- Logout options derive from truthy stored credentials, not effective/runtime/environment auth, and retain credential type and display-name order.
- Frozen has no login argument completion provider or completion grouping/search/description API.

`login_options.rs` extracts methods from interactive-mode; no frozen `login-options.ts` module exists. Typed native APIs exclude invalid JavaScript call argument types. Malformed persisted credential types require separate classification; no parity claim is made for that boundary.

Proof: actual frozen InteractiveMode methods via `interactive-login-oracle.mts`, native capture UI and live registry/storage fixtures in `tests/support/interactive_login.rs`. OAuth callback flow, authentication completion, catalog refresh and whole-mode acceptance are separate proof scopes.
