# Agents status — frozen 4f9e6f8a4

Source `extensions/agents-status/src/index.ts`; native `bundled_extensions/agents_status.rs`.

| Export / branch | Native proof |
| --- | --- |
| `collectAgentRows`, isVisible, compareAgents, lifecycle priorities | Actual frozen differential: active/terminal/expired, root/child/grandchild, invisible parent becomes root, urgency ordering and stable updatedAt ties |
| `describeAgentKind` | Same frozen model/type text, model present/absent |
| `describeActivity`, elapsedMs, lifecycleIcon | Differential covers all seven lifecycles, running tool detail, steering thinking, failed error and terminal durations |
| singleLine/padColumn/widestColumn/renderAgentRow/renderAgentsStatus | 24 frozen rendering cases: empty, one row, tree and terminal-only sets across widths 0/1/8/24/60/120; exact ANSI/text, CJK widths, newline collapse, five-row cap and more indicator |
| `createAgentsStatusComponent`, default extension | Pending widget registration, update subscription, 1-second refresh, targeted component redraw and disposal |

RED `/tmp/agents-status-render-red2.log`: native empty renderer omitted a live row; GREEN `/tmp/agents-status-render-green.log` passes the actual frozen matrix. Theme style prefixes are injected from the independently ported native theme; the actual source renderer chooses every role/style, and output is compared without stripping ANSI.

`format_elapsed_duration` moved without behavior change to `pi-coding-agent-base/utils/elapsed_time.rs`; the old UI module re-exports that one authority. This preserves independent UI/extension builds and adds no dependency. Parent explicitly authorized this relocation on 2026-10-25.

Gap: cyclic/adversarial agent graphs, numeric extremes and runtime scheduling fault shapes follow the project parity boundary.
