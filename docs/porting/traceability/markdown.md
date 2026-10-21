# markdown.ts — Q2 frozen re-pin

Authority: [4f9e6f8a4](../source-lock.json), 937 lines. Rust: `crates/pi-tui/src/components/markdown/{mod,lexer,tokens}.rs`. Contract/proof: [components](../../specs/tui-components.md), [Q2](../q2-tui-components-proof.md).

| Frozen symbol / branch | Rust / behavioral proof |
|---|---|
| DefaultTextStyle | color/background/bold/italic/strike/underline; differential_markdown ANSI style cases and native markdown styling tests |
| MarkdownTheme | all 14 required styles, optional code highlighter and indent; native markdown plus render differential |
| MarkdownOptions | preserveOrderedListMarkers / preserveBackslashEscapes only; native plus differential option corpus |
| constructor / setText / setPadding / setTheme / invalidate / render | Rust methods; `repin_components::component_lifetimes_match_the_frozen_modules` directly compares frozen snapshots and callback side effects |
| same text/padding/theme guards | no cache invalidation; lifecycle differential and concrete highlighter output counters |
| per-token caching | next token type, content width and serialized token fingerprint; unchanged top-level code token reused during streaming; explicit invalidation/theme change discards token caches |
| default style prefix | cached sentinel prefix; lazy default context avoids calling default foreground style inside heading/quote contexts; exact callback counts in lifecycle differential |
| strict strikethrough / HTML comments / partial closing fences | lexer + renderer; native markdown tests and frozen token/render corpus |
| heading, paragraph, text, code, list, table, blockquote, hr, HTML, space, unknown-token text | render_token; native 69-test markdown target and differential_markdown corpus |
| escape/text/nested paragraph/strong/em/del/code/link/image/br/HTML inline branches | render_inline_tokens; token and ANSI render differentials, nested style restoration and hyperlink capability tests |
| list markers, tasks, loose/nested lists, code/quote children, continuation indentation | render_list; native list matrix and frozen render corpus |
| table natural widths, wrapping, padding, separators, narrow widths | render_table; native and frozen render corpus |
| image protocol lines | skip width wrapping/margins; markdown_isimageline_regression plus render differential |

The Rust marked lexer is an internal implementation of the frozen third-party parser, not an exported Marked facade. Tokens are serialized only for differential tests. Removed later-only LaTeX tokens/extensions/renderer, public Marked/Tokens facade, transform/renderLatex options and obsolete math/transform tests. Frozen `$x$` is literal Markdown text. User/assistant consumers no longer install transforms; broader consumer lifetime re-pin remains Q13.

Typed boundaries: Rust UTF8 strings/byte offsets replace JS UTF16; Intl dictionary segmentation uses approved ICU4X adaptation. Theme functions remain typed callbacks. Driver-owned animation concerns belong to editor/loader, not Markdown.

Mutants disabling reuse, invalidating on edits, violating theme identity, or eagerly invoking default foreground styling are killed by lifecycle behavioral tests. Full Q2 evidence and approved domain exclusions are linked above.
