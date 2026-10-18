# components/branch-summary-message.ts

Source: `coding-agent/src/modes/interactive/components/branch-summary-message.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/branch_summary_message.rs`

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
