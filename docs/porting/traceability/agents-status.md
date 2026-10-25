# Agents status — frozen 4f9e6f8a4

Source `extensions/agents-status/src/index.ts`; native `bundled_extensions/agents_status.rs`.

| Export / branch | Native proof |
| --- | --- |
| `collectAgentRows`, isVisible, compareAgents, lifecycle priorities | Actual frozen differential: active/terminal/expired, root/child/grandchild, invisible parent becomes root, urgency ordering and stable updatedAt ties |
| `describeAgentKind` | Same frozen model/type text, model present/absent |
| `describeActivity`, elapsedMs, lifecycleIcon | Differential covers all seven lifecycles, running tool detail, steering thinking, failed error and terminal durations |
| singleLine/padColumn/widestColumn/renderAgentRow/renderAgentsStatus | 24 frozen rendering cases: empty, one row, tree and terminal-only sets across widths 0/1/8/24/60/120; exact ANSI/text, CJK widths, newline collapse, five-row cap and more indicator |
| `createAgentsStatusComponent`, default extension | Native RED/GREEN proves conditional registration, update subscription, 1-second timer, idle suppression, last-visible clear, explicit/double disposal and no post-dispose callbacks. Requests carry a stable component identity, never an unconditional full redraw |

RED `/tmp/agents-status-render-red2.log`: native empty renderer omitted a live row; GREEN `/tmp/agents-status-render-green.log` passes the actual frozen matrix. Theme style prefixes are injected from the independently ported native theme; the actual source renderer chooses every role/style, and output is compared without stripping ANSI.

`format_elapsed_duration` moved without behavior change to `pi-coding-agent-base/utils/elapsed_time.rs`; the old UI module re-exports that one authority. This preserves independent UI/extension builds and adds no dependency. Parent explicitly authorized this relocation on 2026-10-25.

## Widget host integration

`ExtensionUiTui` carries the component identity through `InteractiveUiTuiBackend`/`UiRequest` to the actual `ExtensionComponentAdapter`. The UI thread calls existing `TUI.request_component_render` on that same ComponentRef, preserving its own unmapped-region behavior. A real capture-terminal/mapped-region test updates the target text without rendering an unrelated widget. This is not a full-render approximation.

The host previously never called widget dispose on replacement/clear and constructed the new widget first. Real UI RED observed constructed/constructed instead of constructed/disposed/constructed. The host now disposes before the replacement factory, and disposes all widgets before clear. Native test covers the exact observable order. Frozen authority: interactive-mode.ts:2562–2612.

First-party registration adds agents-status after agents-mailbox, with the same store; source main.ts:720–722. No new dependencies. Fifteen test backends mechanically implement the newly exposed source TUI request method.

Proof: `/tmp/agents-status-{widget-red,widget-green2,register-red,all-focused-green,widget-lifetime-red3,widget-lifetime-green,ui-target-green2}.log`. Current focused status tests: 3 pass; actual UI host tests: 2 pass.

Gap: cyclic/adversarial agent graphs, numeric extremes and runtime scheduling fault shapes follow the project parity boundary.
