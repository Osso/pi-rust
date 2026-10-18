# agent-switcher.ts

Source: `coding-agent/src/modes/interactive/components/agent-switcher.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/agent_switcher.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `MAIN_THREAD_AGENT_ID`, `MAX_VISIBLE_AGENTS` | constants | behavioral + differential |
| `formatLifecycle` | `format_lifecycle` (`lifecycle_name` + `_`→space) | differential lines |
| `formatAgentId` (`<=12`, else 11 + `…`) | `format_agent_id` (`utf16_len`) | code-mapped (ids in tests are short) |
| `buildAgentRows` roots/children order | `build_agent_rows` | child-nesting test + differential |
| tree prefixes `└─`/`├─`/`│  ` | `append_agent` | child-nesting test |
| initial selection = active selected row + 1, else 0 | `AgentSwitcherList::new` | differential selected `agent-2` |
| active-only agent list | `agents` (`is_active_lifecycle`) | hides-inactive test |
| main-thread row + `current` + selectedBg | `render_main_thread_line` | main-thread tests |
| agent row slot/row index/name/id/lifecycle/type/selected | `render_agent_line` | render test + differential |
| 10-row window + scroll indicator | `render_visible_rows` + render | gap: needs >10 rows |
| hidden/showing closed status | `render` | hides/toggles tests |
| up/down/pageUp/pageDown | `handle_input` | selects test + differential |
| `app.agent.toggleClosed` | `handle_input` | toggle test |
| confirm → main/agent id | `confirm_selection` | main + select tests |
| inactive confirm → `onInactiveSelect` | `confirm_selection` | toggle test |
| cancel | `on_cancel` | code-mapped |
| dialog layout + key hints | `AgentSwitcherComponent::new` | differential lines |

Gaps: >10 agents scroll indicator and `formatAgentId` truncation have no direct case. Differential runs with the coding-agent keybinding table (default `ctrl+h` for `app.agent.toggleClosed`).
