# agent-selection-banner.ts

Source: `coding-agent/src/modes/interactive/components/agent-selection-banner.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/agent_selection_banner.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| no store / no selected agent → `[]` | `AgentSelectionBannerComponent::render` | `interactive_components::agent_selection_banner_renders_the_selected_agent` (None store) |
| `Agent {id}: {name} ({lifecycle})` | `selected_agent_text` | same test (`waiting for input`) |
| `_` → space | `format_lifecycle` | same test |
| accent + bold, truncated to width | `theme.fg` + `theme.bold` + `truncate_to_width` | same test |

No differential: the source constructs a `MultiAgentStore` with its own runtime; the Rust test drives the ported store directly at the same boundary.

## Q18 native UTF-16

Render/input, labels, IDs and selection callbacks use `JsString`; preview normalization and ID truncation operate on UTF-16 units. Tests in `raw_error_consumer_components` written **unexecuted**, library compiler-blocked. No internal Node UTF-8 encoding.
