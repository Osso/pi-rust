# oauth-selector.ts

Source: `coding-agent/src/modes/interactive/components/oauth-selector.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/oauth_selector.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `AuthSelectorProvider` | `AuthSelectorProvider` (`AuthType`, `AuthCheck`, method) | `oauth_selector.rs` + differential |
| `ApiKeyAuth | OAuthAuth` method name | `AuthSelectorMethod` (`name()`) | differential `methodName` gap (tests use `None`) |
| `formatAuthSelectorProviderType` | `format_auth_selector_provider_type` | formatting test |
| title login/logout | `new` | differential both modes |
| `showAuthTypeLabels` when >1 distinct type | `auth_types` dedup | differential `[subscription]`/`[API key]` labels |
| initial search input | `initial_search_input` | code-mapped |
| fuzzy filter over name/id/type/method name | `filter_providers` + `pi_tui::fuzzy_filter` | differential `open` query |
| selection clamp | `selected_index.min(len-1)` | differential |
| 8-row window + scroll info | `update_list` | gap: needs >8 providers |
| selected/unselected row rendering | `update_list` | differential lines |
| no-status → `• unconfigured` | `format_status_indicator` | unconfigured test |
| mismatched status type → configured label | `format_status_indicator` | subscription-configured test |
| OAuth / stored credential → `✓ configured` | `format_status_indicator` | code-mapped (`OAuth` case in test) |
| env-var list → `env: ...` | `is_env_var_list` | env test |
| other source → `✓ source` | `format_status_indicator` | models.json tests |
| up/down/confirm/cancel | `handle_input` | differential |
| search input fallthrough | `handle_input` else branch | differential `open` query |
| empty provider messages | `update_list` | gap: not rendered in tests |
| focus propagation | `Focusable` | code-mapped |

Gaps: `getLoginProviderOptions` (first TS test) belongs to `interactive-mode.ts`, outside this slice. Scroll indicator (>8 providers), empty-list messages, and method-name search have no direct case. Differential covers rendering, status variants and search filtering with exact ANSI lines.
