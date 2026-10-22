# interactive selector components traceability

## Common-pin native extension selector checkpoint

Extension selector options, title, callbacks, render and input use `JsString`; themed border callback is native. `raw_extension_dialogs.rs` compares frozen renders/callbacks for lone high/low units and an astral pair at widths 4/12/40, with invalidation and selection navigation. Theme-color bytes are excluded from this plain-line comparison; timer/toggle/raw empty-option proofs are not claimed by this checkpoint.

Sources (all under `packages/coding-agent/src/modes/interactive/components/`):

| TypeScript module | Rust module | Tests |
|---|---|---|
| `dynamic-border.ts` | `components/dynamic_border.rs` | rendered by every selector differential |
| `keybinding-hints.ts` | `components/keybinding_hints.rs` | `tests/components.rs::formats_key_hint_text` |
| `countdown-timer.ts` | `components/countdown_timer.rs` | `tests/components.rs::countdown_timer_ticks_and_expires` |
| `thinking-selector.ts` | `components/thinking_selector.rs` | differential `thinking` case |
| `theme-selector.ts` | `components/theme_selector.rs` | differential `theme` case |
| `show-images-selector.ts` | `components/show_images_selector.rs` | differential `show-images-*` cases |
| `extension-selector.ts` | `components/extension_selector.rs` | differential `extension` case; toggle + countdown behavioral |
| `approval-selector.ts` | `components/approval_selector.rs` | differential `approval` case |
| `trust-selector.ts` | `components/trust_selector.rs` | differential `trust-*` cases |
| `sandbox-selector.ts` | `components/sandbox_selector.rs` | differential `sandbox-*` cases |
| `user-message-selector.ts` | `components/user_message_selector.rs` | differential `user-message-*` cases; auto-cancel behavioral |
| `session-selector-search.ts` | `components/session_selector_search.rs` | differential `coding-agent-session-search-oracle.ts`; `tests/components.rs` |

Differential harness: `oracle/coding-agent-components-oracle.ts` + `tests/differential_components.rs`
(12 components, 27 cases, every step renders at the scripted width and records callback events).
The Rust side runs in a probe child with `PI_PACKAGE_DIR`/`PI_CODING_AGENT_DIR` fixtures; both sides
force `trueColor` capabilities and initialize the `dark` theme.

## Exported symbols

| TypeScript export | Rust item |
|---|---|
| `DynamicBorder` (coding-agent subclass) | `dynamic_border() -> pi_tui DynamicBorder` |
| `formatKeyText` | `format_key_text(&str, KeyTextFormatOptions)` |
| `keyText` / `keyDisplayText` | `key_text` / `key_display_text` |
| `keyHint` / `rawKeyHint` | `key_hint` / `raw_key_hint` |
| `CountdownTimer` | `CountdownTimer::{new, poll, next_deadline, dispose, is_disposed, remaining_seconds}` |
| `ThinkingSelectorComponent` | `ThinkingSelectorComponent::{new, get_select_list}` |
| `ThemeSelectorComponent` | `ThemeSelectorComponent::{new, get_select_list}` |
| `ShowImagesSelectorComponent` | `ShowImagesSelectorComponent::{new, get_select_list}` |
| `ExtensionSelectorComponent` | `ExtensionSelectorComponent::{new, dispose, poll_countdown, next_countdown_deadline, title_text, base_title}` |
| `ApprovalSelectorComponent` | `ApprovalSelectorComponent::{new, scope}`; `ApprovalSelectorSelection`, `ApprovalScope` |
| `TrustSelectorComponent` | `TrustSelectorComponent::new`; `TrustSelection` |
| `SandboxSelectorComponent` | `SandboxSelectorComponent::{new, scope}`; `SandboxSelectorSelection`, `SandboxScope` |
| `UserMessageSelectorComponent` / `UserMessageList` | same names + `get_message_list`, `auto_cancel_deadline`, `poll_auto_cancel` |
| `parseSearchQuery`, `matchSession`, `filterAndSortSessions`, `hasSessionName`, `hasSessionMessages` | same names; `ParsedSearchQuery`, `MatchResult`, `SortMode`, `NameFilter`, `SearchToken`, `SearchTokenKind`, `SearchMode` |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| Selected/current/checkmark styling in every selector | same `theme.fg` calls | differential renders |
| `k`/`j` aliases next to `tui.select.*` | same | differential key steps |
| Approval `Tab` scope toggle | `ApprovalScope` swap | differential `approval` |
| Sandbox scope cycle global→project→session and per-scope selection | `SandboxScope::next` + `selected_index_for_scope` | differential `sandbox-*` |
| Sandbox session "inherit" option | `SESSION_INHERIT_OPTION` | differential `sandbox-session-override` |
| Trust saved/inherited decision detection | `is_saved_option`, `format_decision` | differential `trust-*` |
| User-message scrolling window and truncation | `UserMessageList::render` | differential `user-message-narrow` |
| Empty user-message auto-cancel (100 ms) | `poll_auto_cancel` | `tests/components.rs` |
| Extension countdown title updates | `CountdownTimer` + `set_text` | `tests/components.rs` |
| Extension `app.tools.expand` toggle | `on_toggle_tools_expanded` | `tests/components.rs` |
| Search: quoted phrases, unclosed quotes, regex mode, name filter, recent/relevance/threaded | `parse_search_query`, `match_session`, `filter_and_sort_sessions` | differential search oracle (21 cases) |

## Behavioral notes / deviations

- `CountdownTimer` replaces `setInterval` with `poll(now)` + `next_deadline()`, matching the ported
  `Loader` pattern; ticks and expiry are identical, but the TUI loop must call `poll`.
- `UserMessageSelectorComponent`'s empty-list `setTimeout` is exposed as `poll_auto_cancel`.
- `formatKeyPart`'s macOS `alt` → `option` rename is compiled with `cfg!(target_os = "macos")`.
- Search regex uses `fancy-regex` (lookaround and backreferences work). Error text for invalid patterns
  is Rust's, not V8's; only error presence is behavioral (`filterAndSortSessions` returns `[]`).
  Case-insensitive matching follows Rust Unicode folding rather than JS `/i`.
- Search score uses UTF-16 offsets (`pi_js_compat::utf16_len`) to match `String#indexOf`/`search`.
- `truncateToWidth` with a negative width (component narrower than 2 columns) uses `saturating_sub`,
  while JS passes the negative value through; unreachable at realistic widths.
- Selector components expose their inner list so the interactive mode can set focus, mirroring
  `getSelectList()`/`getMessageList()`.

## Gaps / exclusions

- Timer-driven renders (`CountdownTimer`, empty-list auto-cancel) are behavioral tests only; the
  differential harness renders synchronously.

---

# Additional interactive components (second group)

Sources (all under `packages/coding-agent/src/modes/interactive/`):

| TypeScript module | Rust module | Tests |
|---|---|---|
| `model-search.ts` | `model_search.rs` | exercised through scoped-models search differential |
| `components/agent-switcher.ts` | `components/agent_switcher.rs` | differential `agent-switcher-*` |
| `components/oauth-selector.ts` | `components/oauth_selector.rs` | differential `oauth-*` |
| `components/login-dialog.ts` | `components/login_dialog.rs` | differential `login-*` |
| `components/scoped-models-selector.ts` | `components/scoped_models_selector.rs` | differential `scoped-*` |

## Exported symbols

| TypeScript export | Rust item |
|---|---|
| `getModelSearchCandidates(item)` | `get_model_search_candidates(&ModelSearchItem) -> Vec<String>` |
| `AgentSwitcherComponent` | `AgentSwitcherComponent::{new, get_list}` |
| `AuthSelectorProvider`, `formatAuthSelectorProviderType` | `AuthSelectorProvider` (+ `with_api_key_auth`/`with_oauth_auth`), `format_auth_selector_provider_type(AuthType)` |
| `OAuthSelectorComponent` | `OAuthSelectorComponent::{new, get_search_input}`; `AuthSelectorMode` |
| `LoginDialogComponent` | `LoginDialogComponent::{new, signal, show_auth, show_device_code, show_manual_input, show_prompt, show_details, show_info, show_waiting, show_progress, get_input}`; `LoginError` |
| `ScopedModelsSelectorComponent` | `ScopedModelsSelectorComponent::{new, get_search_input}`; `ModelsConfig`, `ModelsCallbacks`, `EnabledIds` ([common-pin reconciliation](interactive-scoped-models-selector.md)) |

## Branches

| TypeScript branch | Rust branch | Tests |
|---|---|---|
| Agent tree prefix build (`├─`/`└─`/`│  `) and root detection | `build_agent_rows` | differential `agent-switcher-tree` |
| Closed agents hidden/toggled, hidden-count footer | `agents()`, `show_closed_agents` | differential `agent-switcher-closed` |
| Agent scrolling window and page up/down | `render_visible_rows` | differential `agent-switcher-scroll` |
| Inactive selection callback | `confirm_selection` | differential `agent-switcher-closed` |
| OAuth status indicator variants (unconfigured, type mismatch, OAuth/stored, env list, raw source) | `format_status_indicator`, `is_env_source_list` | differential `oauth-mixed` |
| OAuth fuzzy search and backspace re-filter | `filter_providers` | differential `oauth-search` |
| OAuth empty/no-match messages | `update_list` | differential `oauth-logout-empty`, `oauth-no-match` |
| Login dialog content builders and prompt resolution | `show_*`, `oneshot` resolver | differential `login-*` |
| Login cancel (escape) | `cancel` | differential `login-*` |
| Scoped model enable/disable/clear/provider/reorder/save | `toggle`, `enable_all`, `clear_all`, `toggle_provider`, `reorder` | differential `scoped-*` |
| Scoped footer counts incl. unavailable ids and dirty marker | `get_footer_text` | differential `scoped-subset` |
| Ctrl+C clears search, then cancels | `handle_input` | differential `scoped-search` |

## Behavioral notes / deviations

- `AuthSelectorProvider.method` is reduced to `method_name` (the only field the source reads).
- `LoginDialogComponent` prompt promises are `tokio::sync::oneshot::Receiver<Result<String, LoginError>>`;
  `signal()` returns a `CancellationToken`. The dialog exposes `show_auth` but the differential test avoids
  it because both sides would launch a browser.
- `ScopedModelsSelectorComponent` callbacks are synchronous `Rc<dyn Fn(EnabledIds)>`; the source allows
  async callbacks, so callers must spawn work themselves.
- `ModelsCallbacks`/`ModelSelector` timer + `AbortController` refresh flow belongs to the model-runtime
  integration and is not ported here.
- Added `pi_tui::components::input::Input::as_any_mut` so containers can read/update the embedded input
  (mirrors the `Text` downcast hook); needed by the login dialog and both search selectors.

## Gaps / exclusions

- `ModelSelectorComponent` (`model-selector.ts`), `SettingsSelectorComponent`, `ConfigSelectorComponent`,
  `TreeSelectorComponent`, and `SessionSelectorComponent` are not ported in this slice; they depend on the
  model runtime, settings mutation surface, and session tree APIs. Tracked as `todo` in `coverage.md`.
- `updateModels`/`setRefreshStatus` on the scoped-models selector and `showAuth` on the login dialog are
  implemented but not covered by the differential harness.

Frozen4f9e6f8a4 search text: id/name/firstMessage/cwd, never subsequent messages. differential_session_search fixture now supplies firstMessage to actual pinned module and covers exclusion of obsolete later-message text (RED before production).

Bounded merged evidence and unresolved consumer/classification limits: [session storage integration](session-storage-integration.md). No fresh gate or whole-module promotion.
