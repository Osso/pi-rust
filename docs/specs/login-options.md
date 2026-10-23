# Interactive login options and command

Frozen authority: `interactive-mode.ts:6706–6751,6753–6809,7013–7022` at common pin4f9e6f8a4.

- `/login` displays the two fixed authentication choices, subscription then API key. Selecting a choice opens its provider selector; canceling that provider selector returns to the method selector.
- `/login <id>` accepts only an exact OAuth provider ID after command argument trimming. Unknown IDs display `Unknown OAuth provider: <id>` with unchanged UTF-16 text. No name matching, case folding, API-key shortcut or fuzzy fallback.
- Login provider options retain each OAuth provider and each eligible distinct model provider. OAuth/API-key options for the same ID remain distinct. Optional auth-type filtering precedes stable display-name locale sorting.
- API-key eligibility follows the frozen built-in display-name map, then excludes other built-ins and OAuth-only custom IDs.
- Logout options derive from truthy stored credentials, not effective/runtime/environment auth, and retain credential type and display-name order.
- Frozen has no login argument completion provider or completion grouping/search/description API.

`login_options.rs` extracts methods from interactive-mode; no frozen `login-options.ts` module exists. Typed native APIs exclude invalid JavaScript call argument types. [Approved gap](../../AGENTS.md),2026-10-23: truthy persisted auth.json records with unknown/missing type fail explicitly in the logout list; their raw type is not widened into AuthSelectorProvider. Frozen passes the raw value through (undefined omits authType in JSON), proved separately from valid-record parity.

Proof: actual frozen InteractiveMode methods via `interactive-login-oracle.mts`, native capture UI and live registry/storage fixtures in `tests/support/interactive_login.rs`. OAuth callback flow, authentication completion, catalog refresh and whole-mode acceptance are separate proof scopes.

## Authentication completion and count

Completion refreshes the registry first. Only an unknown previous model triggers default selection. Default-provider membership uses frozen `in`, including inherited properties; missing provider models are checked before matching the default ID. Inherited function/object values are interpolation-only via the [shared authority](object-prototype-lookups.md), never treated as string IDs. Preserve success status alongside a default-selection error, unchanged current model when no default is selected, exact UTF-16 display text and auth path, and unique-provider footer count from current viewed/scoped candidates.

The test-only registry override leaves existing capture-mode callers unchanged and supplies actual models.json-backed registries. Source proofs invoke actual InteractiveMode completion/count/notification methods and actual ModelRegistry. Session setter, footer invalidation and editor-border hooks remain structural fixture dependencies; notification helpers are not replaced. Real frozen Container/Text/Spacer/Theme render full chat frames. Native uses the real AgentSession and capture UI; compare complete UTF-16 output, including ANSI, padding, blank lines and notification prefixes. Warning scheduling and remaining completion branches stay separately tracked.

## OAuth select callback

Frozen `showOAuthLoginSelect` restores the login dialog before resolving. Cancel and a label with no matching option resolve undefined, not a Login-cancelled rejection. Selecting an empty-string option ID returns that string, not undefined. Native optional replies preserve this distinction; text-prompt cancellation still rejects. `interactive-login-callback-oracle.mts` exercises the actual source login/dialog/select methods; native tests drive the queued UI and compare full selector frames, returned values, outer error and restored editor.

## OAuth callback-server manual input

Frozen `showLoginDialog` creates one manual-code promise per login. `onAuth` mounts the redirect input immediately only for callback-server providers, before any request to read its value. `onManualCodeInput` returns that same settlement, without adding/replacing UI. Nonempty input resolves once; an empty submission leaves it pending; prompt cancellation rejects with `Login cancelled`. Repeated readers observe the same exact UTF-16 value. Native shared settlement is separate from the dialog's replaceable prompt future. Browser launches in differential proofs use an owned, isolated xdg-open fake; no live authorization occurs.
