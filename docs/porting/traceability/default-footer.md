# `default-footer` extension traceability

## Native representation checkpoint

Render pipeline and helpers retain native pwd/branch/session-name/selected-agent/status/effort strings through style, truncation and final lines. Early `encode_node_utf8` on display/session names is removed. `raw_text_tests::selected_agent_label_retains_unmatched_units` runs the actual module for high/low/pair names. `/tmp/q18-default-footer-native-green1.log` is GREEN using narrowly documented context/store/theme fixtures; only helper behavior and module compilation are proven there, not live context, full render, store, registration or subscription semantics. Superseded by the whole-module re-pin and full-render proof below.

Source: `packages/coding-agent/extensions/default-footer/src/index.ts`. Rust:
`crates/pi-coding-agent-extensions/src/bundled_extensions/default_footer/mod.rs`.

| TypeScript symbol / branch | Rust item | Proof |
|---|---|---|
| `default` factory (`session_start` → `ui.setDefaultFooter`) | `default_footer_extension`, `register_small_extensions` | `default_footer_extension.rs::extension_registers_default_footer_factory_on_session_start` |
| `DefaultFooterExtensionOptions.multiAgentStore` | `DefaultFooterExtensionOptions.multi_agent_store` | same registration test; selected-agent test |
| `DefaultFooterAgentLifecycleCounts` | `DefaultFooterAgentLifecycleCounts` | behavioral count test |
| `DefaultFooterComponentInput` | `DefaultFooterComponentInput` | behavioral and differential cases |
| `DefaultFooterSelectedAgent` | `DefaultFooterSelectedAgent` | behavioral selected-agent test; differential `selected_agent` |
| `createDefaultFooterComponent` (`dispose`, `render` 3-line layout) | `create_default_footer_component`, `DefaultFooterComponent` | behavioral `renders_stats_*`, `extension_status_lines_*`, `pwd_line_*`; differential all cases |
| `formatTokens` | reused `interactive::components::footer::format_tokens` | differential `full_stats`, `narrow` |
| `formatCost` (`cost $x.xx`, zero omitted) | `format_cost` | differential `full_stats`, `child_override`; zero usage case |
| `footerSession` (`getSessionOverride` precedence, ctx fallback) | `footer_session` | differential `child_override`; behavioral pwd override test |
| `formatContextUsage` (known percent, unknown percent, model window fallback) | `format_context_usage` | differential `full_stats`, `unknown_context`; behavioral unknown-context test |
| `formatAgentCounts` (sum > 0) | `format_agent_counts` | differential `full_stats`; behavioral zero-count test |
| `formatAgentLifecycle` (`_` → space) | `format_agent_lifecycle` | differential `selected_agent` |
| `formatSelectedAgent` (slot suffix + sanitize) | `format_selected_agent` | differential `selected_agent`; behavioral selected-agent test |
| `formatCwd` (`~` substitution, outside-home unchanged) | reused `format_cwd_for_footer` | differential pwd lines |
| `countMatchingLifecycle` | `count_matching_lifecycle` | behavioral count test |
| `countDefaultFooterAgents` | `count_default_footer_agents` | behavioral count test |
| `sessionUsage` (assistant input/output/cost.total) | `session_usage` | differential `full_stats`, `child_override` |
| `formatStats` (executable, selected agent, counts, in/out/cost/context) | `format_stats` | differential all; behavioral stats tests |
| `formatRightSide` (model id, effort suffix, provider prefix when >1 provider) | `format_right_side` | differential `provider_count`, `selected_agent`; behavioral effort test |
| `formatStatsLine` (padding + truncation) | `format_stats_line` | differential `narrow`; behavioral narrow test |
| `formatPwdLine` (override suppresses branch, session name suffix) | `format_pwd_line` | differential `full_stats`, `child_override`; behavioral pwd test |
| `selectedAgentForFooter` / `selectedAgentSlotIndex` | `selected_agent_for_footer`, `selected_agent_slot_index` | behavioral selected-agent test; factory wires both |
| `extensionStatusLines` | `extension_status_lines` | differential `full_stats`; behavioral status grouping test |
| `dispose` unsubscribes branch listener | `DefaultFooterComponent::dispose` | subscription stored in component; factory also adds render subscription |
| `getExecutableName` present/absent | `FooterDataProvider::get_executable_name` formatting branch | behavioral `shows_configured_executable_name` (child process with `PI_EXECUTABLE_NAME`) |

## Discrepancies / gaps

- `FooterDataProvider` is a Rust type rather than a TypeScript structural
  interface; tests inject the same values through its setters and a git
  fixture. No production fallback differs.
- `Theme::fg` takes `ThemeColor::Dim` instead of the string `"dim"`; rendered
  ANSI output matches the TS theme.


Callable-status prerequisite (2026-10-24): extensionStatusLines calls the Result-bearing provider at the same frozen :244 read point. render_checked propagates callback/context errors; the ExtensionUiComponent adapter routes otherwise-unobserved render failure to the existing uncaught authority. Existing text fixtures only wrap Text; no layout/format change. [Contract](../../specs/extension-status.md).

## Whole-module re-pin audit (2026-10-25)

All exports/helpers above were compared to unmodified `4f9e6f8a4c0065d81b10e7177b28d5082f0bc30d`, including lifecycle selection/counting, current/override session stats and pwd, home formatting, model/effort/provider, width clipping, callable status failures, subscriptions/disposal and registration.

The frozen `setBackgroundCompactionStatus` and `registerBackgroundCompactionStatus` are now ported: start sets `background-compaction` to `compacting context`, end/shutdown/startup clear it; every update requests render before startup installs its footer. `background_compaction_status_lifecycle_matches_frozen` compares the exact ordered UI call trace against the actual TS extension; it failed before the implementation and passes after it. Event types/dispatch/emission come from parent commits 78a227f9d/60589c908, merged through 1a02ec893, not a lane-specific event API.

Proof: 13 behavioral tests including lifecycle source oracle and callback failure; original six-case full-render matrix; `footer_render_keeps_raw_session_and_selected_agent_text` compares complete rendered lines carrying high/low unmatched units. Current ledger: [bundled-ext-b](../bundled-ext-b-proof.md).

Gap: pathological inputs, OS/runtime faults, numeric extremes and platform-specific path semantics follow the approved parity bar. No live TUI deployment is claimed.
