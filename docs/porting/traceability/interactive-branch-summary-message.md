# components/branch-summary-message.ts

Source: `coding-agent/src/modes/interactive/components/branch-summary-message.ts`
Rust: `crates/pi-coding-agent-ui/src/modes/interactive/components/branch_summary_message.rs`

Tests: `interactive_components_c.rs::branch_summary_collapsed_and_expanded`,
differential `branchSummary` cases.

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `BranchSummaryMessageComponent` | `BranchSummaryMessageComponent` | behavioral + differential |
| `constructor(message, markdownTheme?)` | `new(BranchSummaryMessage, Option<Rc<dyn MarkdownTheme>>)` | differential |
| `setExpanded` | `set_expanded` | collapsed/expanded differential |
| `invalidate` re-render | `Component::invalidate` | rebuild path |
| `[branch]` label | `ThemeColor::CustomMessageLabel` | differential |
| expanded Markdown with `Branch Summary` header | `Markdown` child | differential |
| collapsed hint with `app.tools.expand` key | `key_text` | differential |

## Gaps

None. Default theme comes from `get_markdown_theme()`; theme colors are read through
`with_theme`, matching the source's live `theme` binding.

## Discrepancies

None found.

## Native message composition

Native style callbacks and rendered lines carry raw summary/skill units directly into Markdown/Text/Box; premature `encode_node_utf8` calls are removed. `raw_summary_components` exercises collapsed/expanded states, invalidation, widths4/12/40 and high/low/pair payloads through the actual production modules. After native interface adaptation, the prior body produced U+FFFD vs frozen raw units (behavioral RED). The comparison observes native plain lines after removing SGR only; it is not a theme-color conformance proof. The standalone harness supplies the same indexed-zero foreground/background fixture as the real public Theme constructor used by the integration test; native plain-line observations remove SGR only after real composition/rendering. An earlier identity fixture was invalid at narrow widths because styling affects wrap whitespace; that fixture was corrected, not production behavior. Three raw payloads × three widths × collapsed/expanded × before/after invalidation now match the frozen classes.
