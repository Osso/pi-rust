# skill-invocation-message.ts

Source: `coding-agent/src/modes/interactive/components/skill-invocation-message.ts`
Rust: `crates/pi-coding-agent/src/modes/interactive/components/skill_invocation_message.rs`

| Symbol / branch | Rust item | Proof |
|---|---|---|
| `customMessageBg` Box(1,1) | `BoxComponent::new(1, 1, bg)` | `interactive_components::skill_invocation_message_switches_between_collapsed_and_expanded` |
| collapsed label + name + expand hint | `update_display` collapsed branch | same test |
| expanded label + Markdown header/content | `update_display` expanded branch | same test |
| `setExpanded` | `set_expanded` | same test |
| `invalidate` rebuilds children | `Component::invalidate` | same test (render equality after collapse) |
| markdown theme constructor default | `markdown_theme: Rc<dyn MarkdownTheme>` parameter | test passes `PlainMarkdownTheme` |

Adaptation: `getMarkdownTheme()` is injected as a constructor parameter (the theme slice owns the builder). Markdown rendering itself is differentially covered by `pi-tui`.

## Native message composition

Native style callbacks and rendered lines carry raw summary/skill units directly into Markdown/Text/Box; premature `encode_node_utf8` calls are removed. `raw_summary_components` exercises collapsed/expanded states, invalidation, widths4/12/40 and high/low/pair payloads through the actual production modules. After native interface adaptation, the prior body produced U+FFFD vs frozen raw units (behavioral RED). The comparison observes native plain lines after removing SGR only; it is not a theme-color conformance proof. The standalone harness supplies the same indexed-zero foreground/background fixture as the real public Theme constructor used by the integration test; native plain-line observations remove SGR only after real composition/rendering. An earlier identity fixture was invalid at narrow widths because styling affects wrap whitespace; that fixture was corrected, not production behavior. Three raw payloads × three widths × collapsed/expanded × before/after invalidation now match the frozen classes.
