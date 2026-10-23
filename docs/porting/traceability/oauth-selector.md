# oauth-selector.ts

Frozen: `4f9e6f8a4`, `coding-agent/src/modes/interactive/components/oauth-selector.ts`.
Native: `src/modes/interactive/components/oauth_selector.rs`.

| Branch | Native behavior | Proof |
|---|---|---|
| constructor :46–53 | owned AuthStorage, provider list, id-only selection, cancellation, optional status callback; blank search | whole OAuth selector differential |
| search :97–101 | provider name/id/authType only; no method metadata | query/empty/no-match/scroll differential; raw method-metadata exclusion test |
| rows :118–132 | no extra subscription/API-key badge | complete ANSI frames |
| status :152–176 | stored credential match/mismatch wins; ambient statuses only for API-key rows | stored + environment/default label/runtime/fallback/models-json-key/models-json-command/unknown differential |
| navigation/selection/cancel :182–205 | id-only callbacks and source keybindings | complete events and frames |

Removed old-only initial-search constructor argument, method-name search, auth-type badge and effective-status-as-stored interpretation. The no-match fixture now types its query through real input events instead of initializing an absent constructor parameter. Raw query units remain exact.

## Native auth boundary (approved 2026-10-22)

Frozen reads `authStorage.get(id)` synchronously while rendering. Native owns the synchronous shared AuthStorage authority, not a precomputed credential snapshot. The optional GetAuthStatus callback reads registry status at render time; absent callback queries AuthStorage.get_auth_status exactly as frozen. Stored credentials are checked before ambient status. Earlier snapshot-callback wording described a retired implementation and is not current proof.

## Whole-module Q13 revalidation

All 208 frozen lines compared: focus reaches search Input; constructor provider snapshot/empty search/title/borders; fuzzy key name/id/authType; selected-index clamping; centered eight-row viewport/count/no-match/empty messages; every stored/ambient status branch; up/down bounds, confirm/cancel and pass-through search input; inherited invalidation of children. `differential_components::oauth_selector_matches_typescript` GREEN1/1 across six scenarios in `/tmp/pi-rust-repin-lossless-interactive/test-oauth-selector-q13-green.log`. This is current pinned proof, not a carry-forward from old pin.

## Q15 carrier cleanup

AuthSelectorProvider now contains only id, native display name and AuthType, matching the frozen valid-input shape. Old-only completion type-label helpers are removed with their callers. Whole login flow remains separately tracked. Truthy persisted credentials whose type is unknown/missing cannot be represented by AuthType in logout provider options. The [approved gap](../../../AGENTS.md),2026-10-23, keeps their explicit logout-list error rather than widening this carrier; this is a persisted-data gap, not a typed-call-input exclusion. Actual source option assembly passes raw type through, omitting authType in JSON when undefined; the four-case approved-gap test records that discrepancy and asserts the native error.
