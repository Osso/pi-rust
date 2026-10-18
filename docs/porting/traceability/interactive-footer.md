# components/footer.ts

Source: `coding-agent/src/modes/interactive/components/footer.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/footer.rs`

Tests: `crates/pi-coding-agent/tests/interactive_components_c.rs` (footer_*),
differential `footerHelpers` + `footer` cases (token/cwd helpers and full render lines).

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `formatTokens` | `format_tokens` | helper differential + behavioral |
| `formatCwdForFooter` | `format_cwd_for_footer` | helper differential |
| `FooterComponent` | `FooterComponent` | differential + behavioral |
| `setSession` | `set_session` | API surface |
| `setAutoCompactEnabled` | `set_auto_compact_enabled` | differential (auto indicator) |
| `setSessionOverride` / `clearSessionOverride` | same-named methods | differential |
| `invalidate` clears cached data | `Component::invalidate` | render path |
| `getSessionData` (usage totals, latest cache hit rate, session name, context usage) | `get_session_data` | differential entries |
| assistant usage | assistant arm | differential ↑/↓/R/W/CH |
| toolResult usage | tool result arm | differential |
| branch summary / compaction usage | `add_entry_usage` arms | source parity (not in differential entries) |
| context window/percent fallbacks | render | differential |
| pwd + branch + session name | render | behavioral (override) |
| stats parts (executable, tokens, cache, cost/sub, context, xp) | render | differential + behavioral |
| model + effort + provider prefix | render | behavioral |
| width truncation and dim part splitting | render | differential width 40 + behavioral width 16 |
| extension status lines | render | differential + behavioral |

## Gaps

- `branch_summary`/`compaction` usage totals are implemented but not exercised by the
  differential fixture (AgentSession fixtures append messages only); covered by code review.
- `usingSubscription` is exercised through `ModelRuntime`; the differential passes
  `usingSubscription: false`.

## Discrepancies

None found.
