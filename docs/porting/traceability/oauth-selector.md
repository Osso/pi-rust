# oauth-selector.ts

Frozen: `4f9e6f8a4`, `coding-agent/src/modes/interactive/components/oauth-selector.ts`.
Native: `src/modes/interactive/components/oauth_selector.rs`.

| Branch | Native behavior | Proof |
|---|---|---|
| constructor :46–53 | stored-type callback, provider list, id-only selection, cancellation, optional status callback; blank search | whole OAuth selector differential |
| search :97–101 | provider name/id/authType only; no method metadata | query/empty/no-match/scroll differential; raw method-metadata exclusion test |
| rows :118–132 | no extra subscription/API-key badge | complete ANSI frames |
| status :152–176 | stored credential match/mismatch wins; ambient statuses only for API-key rows | stored + environment/default label/runtime/fallback/models-json-key/models-json-command/unknown differential |
| navigation/selection/cancel :182–205 | id-only callbacks and source keybindings | complete events and frames |

Removed old-only initial-search constructor argument, method-name search, auth-type badge and effective-status-as-stored interpretation. The no-match fixture now types its query through real input events instead of initializing an absent constructor parameter. Raw query units remain exact.

## Native auth boundary (approved 2026-10-22)

Frozen reads `authStorage.get(id)` synchronously while rendering. Native ModelRegistry wraps async CredentialStore, not exposed AuthStorage. Parent authorized a `Rc<dyn Fn(&str)->Option<AuthType>>` callback over stored credentials awaited before mounting; never ambient/effective auth. Native main refreshes this snapshot each time the selector opens; the optional AuthStatus callback reads runtime status at render time. An absent native status callback supplies no ambient status; production callers supply it explicitly.

## Pending re-pin item

`AuthSelectorProvider` still carries native login-flow metadata (`method_name`, `login_supported`, `status`) used by `login_options.rs` and ambient-auth integration. Selector ignores those fields. Their cleanup is main's Q14 re-pin queue, explicitly excluded from this component slice; no whole-auth-flow parity claim.
