//! Port of `packages/tui/test/markdown.test.ts`.

#[path = "support/markdown_theme.rs"]
mod markdown_theme;
#[path = "support/virtual_terminal.rs"]
mod virtual_terminal;

use std::cell::RefCell;
use std::rc::Rc;

use pi_js_compat::js_json::JsString;
use pi_tui::TUI;
use pi_tui::components::markdown::{DefaultTextStyle, Markdown, MarkdownOptions, MarkdownTheme};
use pi_tui::terminal_image::{TerminalCapabilities, reset_capabilities_cache, set_capabilities};
use pi_tui::tui::{Component, TuiExt, component};

use self::markdown_theme::{CYAN, ChalkTheme, GRAY, MAGENTA, YELLOW, apply as apply_chalk};
use self::virtual_terminal::concrete;

fn lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn markdown(text: &str) -> Markdown {
    Markdown::new(text, 0, 0, Box::new(ChalkTheme), None, MarkdownOptions::default())
}

fn markdown_with(
    text: &str,
    padding_x: usize,
    padding_y: usize,
    default_style: Option<DefaultTextStyle>,
    options: MarkdownOptions,
) -> Markdown {
    Markdown::new(text, padding_x, padding_y, Box::new(ChalkTheme), default_style, options)
}

fn render(markdown: &mut Markdown, width: usize) -> Vec<JsString> {
    markdown.render(width)
}

fn strip_sgr(line: &JsString) -> JsString {
    let mut result = JsString::default();
    let mut chars = line.units().iter().copied().peekable();
    while let Some(ch) = chars.next() {
        if ch != 27 {
            result.push_unit(ch);
            continue;
        }
        if chars.peek() != Some(&91) {
            result.push_unit(ch);
            continue;
        }
        chars.next();
        let mut sequence = JsString::default();
        let mut terminated = false;
        for next in chars.by_ref() {
            if next == 109 {
                terminated = true;
                break;
            }
            if (48..=57).contains(&next) || next == 59 {
                sequence.push_unit(next);
            } else {
                result.push_unit(27);
                result.push_unit(91);
                result.push_str(&sequence);
                result.push_unit(next);
                terminated = true;
                break;
            }
        }
        if !terminated {
            result.push_unit(27);
            result.push_unit(91);
            result.push_str(&sequence);
        }
    }
    result
}

fn plain(markdown: &mut Markdown, width: usize) -> Vec<JsString> {
    render(markdown, width)
        .into_iter()
        .map(|line| strip_sgr(&line).trim_end())
        .collect()
}

fn joined(markdown: &mut Markdown, width: usize) -> JsString {
    JsString::join(render(markdown, width), "\n")
}

fn color_style(style: markdown_theme::Style) -> Box<dyn Fn(&JsString) -> JsString> {
    Box::new(move |text: &JsString| apply_chalk(&[style], text))
}

#[test]
fn mermaid_diagram_renders_as_art() {
    let mut md = markdown("```mermaid\nflowchart LR\n  A[Start] --> B[Done]\n```");
    md.set_mermaid_rendering(true, false);
    assert!(plain(&mut md, 80).iter().any(|line| line.contains("───▶")));
}

struct MermaidTheme;

impl MarkdownTheme for MermaidTheme {
    fn heading(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn link(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn link_url(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn code(&self, text: &JsString) -> JsString {
        apply_chalk(&[YELLOW], text)
    }
    fn code_block(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn code_block_border(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn quote(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn quote_border(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn hr(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn list_bullet(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn bold(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn italic(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn strikethrough(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn underline(&self, text: &JsString) -> JsString {
        text.clone()
    }
    fn mermaid_border(&self, text: &JsString) -> JsString {
        apply_chalk(&[GRAY], text)
    }
    fn mermaid_text(&self, text: &JsString) -> JsString {
        apply_chalk(&[CYAN], text)
    }
    fn mermaid_edge(&self, text: &JsString) -> JsString {
        apply_chalk(&[MAGENTA], text)
    }
}

#[test]
fn mermaid_styles_semantic_spans_inside_code_style() {
    let mut md = Markdown::new(
        "```mermaid\nflowchart LR\n A[Start] --> B[Done]\n```",
        0,
        0,
        Box::new(MermaidTheme),
        None,
        MarkdownOptions::default(),
    );
    md.set_mermaid_rendering(true, false);
    let output = joined(&mut md, 80);
    assert!(output.contains("\x1b[33m"));
    assert!(output.contains("\x1b[90m"));
    assert!(output.contains("\x1b[36m"));
    assert!(output.contains("\x1b[35m"));
}

fn assert_historical_oracle_pin() {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir("/home/osso/.worktrees/pi-rust-source-532f7d12")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "532f7d12cd19235081bd4d6e425bde185652d86f"
    );
}

#[test]
fn mermaid_styling_matches_historical_oracle() {
    use std::process::Command;
    assert_historical_oracle_pin();
    let text = "```mermaid\nflowchart LR\n A[Start] --> B[Done]\n```";
    let script = r#"
import {createMermaidMarkdownTransformer} from './packages/coding-agent/src/modes/interactive/components/mermaid.ts';
import {Markdown} from './packages/tui/src/components/markdown.ts';
import {Chalk} from 'chalk';
const chalk = new Chalk({level: 3});
const identity = text => text;
const theme = new Proxy({}, {get: (_, key) => key === 'codeBlockIndent' ? '  ' : key === 'highlightCode' ? undefined : key === 'code' ? chalk.yellow : identity});
const colors = {borderMuted: chalk.gray, text: chalk.cyan, accent: chalk.magenta, muted: identity, warning: identity};
const transformer = createMermaidMarkdownTransformer({getMode: () => 'streaming', theme: {fg: (color, text) => colors[color](text), bold: identity}});
const text = transformer(process.argv[1], {availableWidth: 80, isStreaming: false, messageType: 'assistant'});
console.log(JSON.stringify(new Markdown(text, 0, 0, theme).render(80).map(line => line.trimEnd())));
"#;
    let output = Command::new("node")
        .args(["--import", "tsx", "--input-type=module", "-e", script, text])
        .current_dir("/home/osso/.worktrees/pi-rust-source-532f7d12")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    let mut md = Markdown::new(text, 0, 0, Box::new(MermaidTheme), None, MarkdownOptions::default());
    md.set_mermaid_rendering(true, false);
    let actual: Vec<String> = md
        .render(80)
        .iter()
        .map(|line| line.trim_end().to_utf8().unwrap())
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn mermaid_mode_changes_invalidate_cached_tokens() {
    let mut md = markdown("```mermaid\nflowchart LR\n A[Foo]:::highlight --> B[Bar]\n```");
    assert!(joined(&mut md, 80).contains("```mermaid"));
    md.set_mermaid_rendering(true, true);
    assert!(joined(&mut md, 80).contains("Foo"));
    assert!(!joined(&mut md, 80).contains("```mermaid"));
    md.set_mermaid_rendering(true, false);
    assert!(joined(&mut md, 80).contains("Mermaid diagram not rendered"));
    md.set_mermaid_rendering(false, false);
    assert!(!joined(&mut md, 80).contains("Mermaid diagram not rendered"));
}

#[test]
fn mermaid_preserves_non_scalar_code() {
    let mut text = JsString::from("```mermaid\nflowchart LR\n A[");
    text.push_unit(0xd800);
    text.push_str("] --> B\n```");
    let mut md = Markdown::new(text, 0, 0, Box::new(ChalkTheme), None, MarkdownOptions::default());
    md.set_mermaid_rendering(true, false);
    let output = joined(&mut md, 80);
    assert!(output.units().contains(&0xd800));
    assert!(output.contains("```mermaid"));
}

#[test]
fn mermaid_matches_historical_markdown_oracle() {
    use std::process::Command;
    assert_historical_oracle_pin();
    let cases = [
        ("```mermaid\nflowchart LR\n A[Start] --> B[Done]\n```", 80, true, false),
        ("```mermaid\nflowchart LR\n A[👩‍💻] --> B[e\u{301}]\n```", 80, true, false),
        (
            "```mermaid\nsequenceDiagram\n Alice->>Bob: Hello\n Bob-->>Alice: Hi\n```",
            80,
            true,
            false,
        ),
        (
            "```mermaid\nflowchart LR\n A[plain ` tick] --> B[Done]\n```",
            80,
            true,
            false,
        ),
        ("```mermaid\nflowchart LR\n A --> B\n```", 8, true, false),
        ("```mermaid\npie\n title Pets\n```", 80, true, false),
        (
            "```mermaid\nflowchart LR\n A[Foo]:::highlight --> B[Bar]\n```",
            80,
            true,
            false,
        ),
        (
            "```mermaid\nflowchart LR\n A[Foo]:::highlight --> B[Bar]\n```",
            80,
            true,
            true,
        ),
        ("> ```mermaid\n> flowchart LR\n> A --> B\n> ```", 80, true, false),
        ("```mermaid\nflowchart LR\n A --> B\n```", 80, false, false),
        (
            "Before\n\n```mermaid\nflowchart LR\n A --> B\n```\nAfter",
            80,
            true,
            false,
        ),
        (
            "```mermaid\nflowchart LR\n A[Foo]:::highlight --> B[Bar]\n```\nAfter",
            80,
            true,
            false,
        ),
    ];
    let script = r#"
import {createMermaidMarkdownTransformer} from './packages/coding-agent/src/modes/interactive/components/mermaid.ts';
import {Markdown} from './packages/tui/src/components/markdown.ts';
const [text, width, enabled, streaming] = JSON.parse(process.argv[1]);
const transformed = createMermaidMarkdownTransformer({getMode: () => enabled ? 'streaming' : 'off'})(text, {availableWidth: width, isStreaming: streaming, messageType: 'assistant'});
const identity = text => text;
const theme = new Proxy({}, {get: (_, key) => key === 'codeBlockIndent' ? '  ' : key === 'highlightCode' ? undefined : identity});
console.log(JSON.stringify(new Markdown(transformed, 0, 0, theme).render(width).map(line => line.trimEnd())));
"#;
    for (text, width, enabled, streaming) in cases {
        let input = serde_json::json!([text, width, enabled, streaming]).to_string();
        let output = Command::new("node")
            .args(["--import", "tsx", "--input-type=module", "-e", script, &input])
            .current_dir("/home/osso/.worktrees/pi-rust-source-532f7d12")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
        let mut md = markdown(text);
        md.set_mermaid_rendering(enabled, streaming);
        let actual: Vec<String> = plain(&mut md, width)
            .iter()
            .map(|line| line.to_utf8().unwrap())
            .collect();
        assert_eq!(actual, expected, "{text:?}, width={width}, streaming={streaming}");
    }
}

#[test]
fn hides_html_comments() {
    let _guard = lock();
    let mut markdown = markdown("alpha\n\n<!-- -->\n\nbeta");
    let lines = plain(&mut markdown, 80);
    assert!(lines.iter().any(|line| line == "alpha"));
    assert!(lines.iter().any(|line| line == "beta"));
    assert!(!lines.iter().any(|line| line.contains("<!--")));
}

#[test]
fn renders_non_comment_html_as_plain_text() {
    let _guard = lock();
    let mut markdown = markdown("<span>visible</span>");
    assert_eq!(plain(&mut markdown, 80), vec!["<span>visible</span>"]);
}

#[test]
fn renders_simple_nested_list() {
    let _guard = lock();
    let mut markdown = markdown("- Item 1\n  - Nested 1.1\n  - Nested 1.2\n- Item 2");
    let lines = plain(&mut markdown, 80);
    assert!(lines.iter().any(|line| line.contains("- Item 1")));
    assert!(lines.iter().any(|line| line.contains("    - Nested 1.1")));
    assert!(lines.iter().any(|line| line.contains("    - Nested 1.2")));
    assert!(lines.iter().any(|line| line.contains("- Item 2")));
}

#[test]
fn renders_deeply_nested_list() {
    let _guard = lock();
    let mut markdown = markdown("- Level 1\n  - Level 2\n    - Level 3\n      - Level 4");
    let lines = plain(&mut markdown, 80);
    assert!(lines.iter().any(|line| line.contains("- Level 1")));
    assert!(lines.iter().any(|line| line.contains("    - Level 2")));
    assert!(lines.iter().any(|line| line.contains("        - Level 3")));
    assert!(lines.iter().any(|line| line.contains("            - Level 4")));
}

#[test]
fn renders_ordered_nested_list() {
    let _guard = lock();
    let mut markdown = markdown("1. First\n   1. Nested first\n   2. Nested second\n2. Second");
    let lines = plain(&mut markdown, 80);
    assert!(lines.iter().any(|line| line.contains("1. First")));
    assert!(lines.iter().any(|line| line.contains("    1. Nested first")));
    assert!(lines.iter().any(|line| line.contains("    2. Nested second")));
    assert!(lines.iter().any(|line| line.contains("2. Second")));
}

#[test]
fn normalizes_ordered_list_markers_by_default() {
    let _guard = lock();
    let mut markdown = markdown("1. alpha\n1. beta\n1. gamma");
    assert_eq!(plain(&mut markdown, 80), vec!["1. alpha", "2. beta", "3. gamma"]);
}

#[test]
fn preserves_source_list_markers_when_configured() {
    let _guard = lock();
    let mut markdown = markdown_with(
        "  4. forth\n  3. third\n\n10) ten\n7) seven\n\n+ plus\n* star\n- minus\n+",
        0,
        0,
        None,
        MarkdownOptions {
            preserve_ordered_list_markers: true,
            ..Default::default()
        },
    );
    assert_eq!(
        plain(&mut markdown, 80),
        vec![
            "4. forth", "3. third", "", "10) ten", "7) seven", "", "+ plus", "* star", "- minus", "+",
        ]
    );
}

#[test]
fn renders_mixed_ordered_and_unordered_nested_lists() {
    let _guard = lock();
    let mut markdown =
        markdown("1. Ordered item\n   - Unordered nested\n   - Another nested\n2. Second ordered\n   - More nested");
    let lines = plain(&mut markdown, 80);
    assert!(lines.iter().any(|line| line.contains("1. Ordered item")));
    assert!(lines.iter().any(|line| line.contains("    - Unordered nested")));
    assert!(lines.iter().any(|line| line.contains("2. Second ordered")));
}

#[test]
fn renders_blank_lines_between_loose_list_items() {
    let _guard = lock();
    let mut markdown = markdown(
        "1. Lorem ipsum dolor sit amet.\n\n   Ut enim ad minim veniam.\n\n2. Duis aute irure dolor.\n\n   Excepteur sint occaecat cupidatat.\n\n3. Beep boop",
    );
    assert_eq!(
        plain(&mut markdown, 80),
        vec![
            "1. Lorem ipsum dolor sit amet.",
            "",
            "   Ut enim ad minim veniam.",
            "",
            "2. Duis aute irure dolor.",
            "",
            "   Excepteur sint occaecat cupidatat.",
            "",
            "3. Beep boop",
        ]
    );
}

#[test]
fn renders_task_list_markers() {
    let _guard = lock();
    let mut markdown = markdown("- [ ] beep\n- [x] boop");
    assert_eq!(plain(&mut markdown, 80), vec!["- [ ] beep", "- [x] boop"]);
}

#[test]
fn maintains_numbering_when_code_blocks_are_not_indented() {
    let _guard = lock();
    let mut markdown = markdown(
        "1. First item\n\n```typescript\n// code block\n```\n\n2. Second item\n\n```typescript\n// another code block\n```\n\n3. Third item",
    );
    let lines: Vec<JsString> = render(&mut markdown, 80)
        .into_iter()
        .map(|line| strip_sgr(&line).trim())
        .collect();
    let numbered: Vec<&JsString> = lines
        .iter()
        .filter(|line| line.units().first().is_some_and(|unit| (48..=57).contains(unit)))
        .collect();
    assert_eq!(numbered.len(), 3, "{numbered:?}");
    assert!(numbered[0].starts_with("1."), "{numbered:?}");
    assert!(numbered[1].starts_with("2."), "{numbered:?}");
    assert!(numbered[2].starts_with("3."), "{numbered:?}");
}

#[test]
fn indents_wrapped_unordered_list_lines() {
    let _guard = lock();
    let mut markdown = markdown("- alpha beta gamma delta epsilon");
    assert_eq!(plain(&mut markdown, 20), vec!["- alpha beta gamma", "  delta epsilon"]);
}

#[test]
fn indents_wrapped_ordered_list_lines() {
    let _guard = lock();
    let mut markdown = markdown("1. alpha beta gamma delta epsilon");
    assert_eq!(
        plain(&mut markdown, 20),
        vec!["1. alpha beta gamma", "   delta epsilon"]
    );
}

#[test]
fn indents_wrapped_ordered_list_lines_with_multi_digit_markers() {
    let _guard = lock();
    let mut markdown = markdown("10. alpha beta gamma delta epsilon");
    assert_eq!(
        plain(&mut markdown, 21),
        vec!["10. alpha beta gamma", "    delta epsilon"]
    );
}

#[test]
fn indents_wrapped_nested_list_lines() {
    let _guard = lock();
    let mut markdown = markdown("- parent\n  - alpha beta gamma delta epsilon");
    assert_eq!(
        plain(&mut markdown, 24),
        vec!["- parent", "    - alpha beta gamma", "      delta epsilon"]
    );
}

#[test]
fn indents_wrapped_nested_list_lines_under_ordered_parents() {
    let _guard = lock();
    let mut markdown = markdown("1. parent\n   - alpha beta gamma delta epsilon");
    assert_eq!(
        plain(&mut markdown, 24),
        vec!["1. parent", "    - alpha beta gamma", "      delta epsilon"]
    );
}

#[test]
fn renders_and_wraps_blockquotes_inside_list_items() {
    let _guard = lock();
    let mut markdown = markdown("- > alpha beta gamma delta epsilon zeta");
    assert_eq!(
        plain(&mut markdown, 24),
        vec!["- │ alpha beta gamma", "  │ delta epsilon zeta"]
    );
}

#[test]
fn renders_and_wraps_code_blocks_inside_list_items() {
    let _guard = lock();
    let mut markdown = markdown("- ```ts\n  alpha beta gamma delta epsilon zeta\n  ```");
    assert_eq!(
        plain(&mut markdown, 24),
        vec!["- ```ts", "    alpha beta gamma", "  delta epsilon zeta", "  ```"]
    );
}

#[test]
fn renders_simple_table() {
    let _guard = lock();
    let mut markdown = markdown("| Name | Age |\n| --- | --- |\n| Alice | 30 |\n| Bob | 25 |");
    let lines = plain(&mut markdown, 80);
    assert!(lines.iter().any(|line| line.contains("Name")));
    assert!(lines.iter().any(|line| line.contains("Age")));
    assert!(lines.iter().any(|line| line.contains("Alice")));
    assert!(lines.iter().any(|line| line.contains("Bob")));
    assert!(lines.iter().any(|line| line.contains("│")));
    assert!(lines.iter().any(|line| line.contains("─")));
}

#[test]
fn renders_row_dividers_between_data_rows() {
    let _guard = lock();
    let mut markdown = markdown("| Name | Age |\n| --- | --- |\n| Alice | 30 |\n| Bob | 25 |");
    let lines = plain(&mut markdown, 80);
    let dividers = lines.iter().filter(|line| line.contains("┼")).count();
    assert_eq!(dividers, 2);
}

#[test]
fn keeps_column_width_at_least_the_longest_word() {
    let _guard = lock();
    let longest_word = "superlongword";
    let mut markdown = markdown(&format!(
        "| Column One | Column Two |\n| --- | --- |\n| {longest_word} short | otherword |\n| small | tiny |"
    ));
    let lines = plain(&mut markdown, 32);
    let data_line = lines.iter().find(|line| line.contains(longest_word)).expect("data row");
    let segments: Vec<JsString> = data_line.split('│').collect();
    let first_segment = &segments[1];
    let first_column_width = first_segment.len() - 2;
    assert!(
        first_column_width >= longest_word.len(),
        "{first_column_width} < {}",
        longest_word.len()
    );
}

#[test]
fn renders_table_with_alignment() {
    let _guard = lock();
    let mut markdown =
        markdown("| Left | Center | Right |\n| :--- | :---: | ---: |\n| A | B | C |\n| Long text | Middle | End |");
    let lines = plain(&mut markdown, 80);
    assert!(lines.iter().any(|line| line.contains("Left")));
    assert!(lines.iter().any(|line| line.contains("Center")));
    assert!(lines.iter().any(|line| line.contains("Right")));
    assert!(lines.iter().any(|line| line.contains("Long text")));
}

#[test]
fn handles_tables_with_varying_column_widths() {
    let _guard = lock();
    let mut markdown = markdown(
        "| Short | Very long column header |\n| --- | --- |\n| A | This is a much longer cell content |\n| B | Short |",
    );
    let lines = plain(&mut markdown, 80);
    assert!(lines.iter().any(|line| line.contains("Very long column header")));
    assert!(
        lines
            .iter()
            .any(|line| line.contains("This is a much longer cell content"))
    );
}

#[test]
fn wraps_table_cells_when_table_exceeds_available_width() {
    let _guard = lock();
    let mut markdown = markdown(
        "| Command | Description | Example |\n| --- | --- | --- |\n| npm install | Install all dependencies | npm install |\n| npm run build | Build the project | npm run build |",
    );
    let lines = plain(&mut markdown, 50);
    for line in &lines {
        assert!(
            char::decode_utf16(line.units().iter().copied()).count() <= 50,
            "line exceeds width: {line:?}"
        );
    }
    let all_text = JsString::join(&lines, " ");
    assert!(all_text.contains("Command"));
    assert!(all_text.contains("Description"));
    assert!(all_text.contains("npm install"));
    assert!(all_text.contains("Install"));
}

#[test]
fn wraps_long_cell_content_to_multiple_lines() {
    let _guard = lock();
    let mut markdown = markdown("| Header |\n| --- |\n| This is a very long cell content that should wrap |");
    let lines = plain(&mut markdown, 25);
    let data_rows = lines
        .iter()
        .filter(|line| line.starts_with('│') && !line.contains('─'))
        .count();
    assert!(data_rows > 2, "{data_rows}");
    let all_text = JsString::join(&lines, " ");
    assert!(all_text.contains("very long"));
    assert!(all_text.contains("cell content"));
    assert!(all_text.contains("should wrap"));
}

#[test]
fn wraps_long_unbroken_tokens_inside_table_cells() {
    let _guard = lock();
    set_capabilities(TerminalCapabilities {
        images: None,
        true_color: false,
        hyperlinks: false,
    });
    let url = "https://example.com/this/is/a/very/long/url/that/should/wrap";
    let mut markdown = markdown(&format!("| Value |\n| --- |\n| prefix {url} |"));
    let width = 30;
    let lines = plain(&mut markdown, width);
    reset_capabilities_cache();
    for line in &lines {
        assert!(
            char::decode_utf16(line.units().iter().copied()).count() <= width,
            "line exceeds width: {line:?}"
        );
    }
    let table_lines: Vec<&JsString> = lines.iter().filter(|line| line.starts_with('│')).collect();
    assert!(!table_lines.is_empty());
    for line in table_lines {
        assert_eq!(
            line.units().iter().filter(|unit| **unit == 0x2502).count(),
            2,
            "{line:?}"
        );
    }
    let joined = JsString::join(&lines, "");
    let extracted = JsString::from_utf16(
        joined
            .units()
            .iter()
            .copied()
            .filter(|unit| {
                ![0x2502, 0x251c, 0x2524, 0x2500].contains(unit) && !pi_js_compat::string::is_js_whitespace_unit(*unit)
            })
            .collect(),
    );
    assert!(extracted.contains("prefix"));
    assert!(extracted.contains(url));
}

#[test]
fn wraps_styled_inline_code_inside_table_cells_without_breaking_borders() {
    let _guard = lock();
    let mut markdown = markdown("| Code |\n| --- |\n| `averyveryveryverylongidentifier` |");
    let width = 20;
    let lines = render(&mut markdown, width);
    assert!(JsString::join(&lines, "\n").contains("\u{1b}[33m"));
    let plain_lines: Vec<JsString> = lines.iter().map(|line| strip_sgr(line).trim_end()).collect();
    for line in &plain_lines {
        assert!(
            char::decode_utf16(line.units().iter().copied()).count() <= width,
            "line exceeds width: {line:?}"
        );
    }
    for line in plain_lines.iter().filter(|line| line.starts_with('│')) {
        assert_eq!(
            line.units().iter().filter(|unit| **unit == 0x2502).count(),
            2,
            "{line:?}"
        );
    }
}

#[test]
fn handles_extremely_narrow_width_gracefully() {
    let _guard = lock();
    let mut markdown = markdown("| A | B | C |\n| --- | --- | --- |\n| 1 | 2 | 3 |");
    let lines = plain(&mut markdown, 15);
    assert!(!lines.is_empty());
    for line in &lines {
        assert!(
            char::decode_utf16(line.units().iter().copied()).count() <= 15,
            "line exceeds width: {line:?}"
        );
    }
}

#[test]
fn renders_table_correctly_when_it_fits_naturally() {
    let _guard = lock();
    let mut markdown = markdown("| A | B |\n| --- | --- |\n| 1 | 2 |");
    let lines = plain(&mut markdown, 80);
    let header = lines
        .iter()
        .find(|line| line.contains('A') && line.contains('B'))
        .expect("header row");
    assert!(header.contains('│'));
    assert!(lines.iter().any(|line| line.contains('├') && line.contains('┼')));
    assert!(lines.iter().any(|line| line.contains('1') && line.contains('2')));
}

#[test]
fn respects_padding_x_when_calculating_table_width() {
    let _guard = lock();
    let mut markdown = markdown_with(
        "| Column One | Column Two |\n| --- | --- |\n| Data 1 | Data 2 |",
        2,
        0,
        None,
        MarkdownOptions::default(),
    );
    let lines = plain(&mut markdown, 40);
    for line in &lines {
        assert!(
            char::decode_utf16(line.units().iter().copied()).count() <= 40,
            "line exceeds width: {line:?}"
        );
    }
    let table_row = lines.iter().find(|line| line.contains('│')).expect("table row");
    assert!(table_row.starts_with("  "));
}

#[test]
fn does_not_add_trailing_blank_line_when_table_is_last_block() {
    let _guard = lock();
    let mut markdown = markdown("| Name |\n| --- |\n| Alice |");
    let lines = plain(&mut markdown, 80);
    assert_ne!(lines.last().cloned(), Some(JsString::default()));
}

#[test]
fn renders_lists_and_tables_together() {
    let _guard = lock();
    let mut markdown =
        markdown("# Test Document\n\n- Item 1\n  - Nested item\n- Item 2\n\n| Col1 | Col2 |\n| --- | --- |\n| A | B |");
    let lines = plain(&mut markdown, 80);
    assert!(lines.iter().any(|line| line.contains("Test Document")));
    assert!(lines.iter().any(|line| line.contains("- Item 1")));
    assert!(lines.iter().any(|line| line.contains("    - Nested item")));
    assert!(lines.iter().any(|line| line.contains("Col1")));
    assert!(lines.iter().any(|line| line.contains('│')));
}

#[test]
fn normalizes_escaped_punctuation_by_default() {
    let _guard = lock();
    let mut markdown = markdown("\"\\\"");
    assert_eq!(plain(&mut markdown, 80), vec!["\"\""]);
}

#[test]
fn preserves_source_backslash_escapes_when_configured() {
    let _guard = lock();
    let mut markdown = markdown_with(
        "\"\\\"",
        0,
        0,
        None,
        MarkdownOptions {
            preserve_backslash_escapes: true,
            ..Default::default()
        },
    );
    assert_eq!(plain(&mut markdown, 80), vec!["\"\\\""]);
}

#[test]
fn preserves_gray_italic_styling_after_inline_code() {
    let _guard = lock();
    let mut markdown = markdown_with(
        "This is thinking with `inline code` and more text after",
        1,
        0,
        Some(DefaultTextStyle {
            color: Some(color_style(GRAY)),
            italic: true,
            ..Default::default()
        }),
        MarkdownOptions::default(),
    );
    let output = joined(&mut markdown, 80);
    assert!(output.contains("inline code"));
    assert!(output.contains("\u{1b}[90m"));
    assert!(output.contains("\u{1b}[3m"));
    assert!(output.contains("\u{1b}[33m"));
}

#[test]
fn preserves_gray_italic_styling_after_bold_text() {
    let _guard = lock();
    let mut markdown = markdown_with(
        "This is thinking with **bold text** and more after",
        1,
        0,
        Some(DefaultTextStyle {
            color: Some(color_style(GRAY)),
            italic: true,
            ..Default::default()
        }),
        MarkdownOptions::default(),
    );
    let output = joined(&mut markdown, 80);
    assert!(output.contains("bold text"));
    assert!(output.contains("\u{1b}[90m"));
    assert!(output.contains("\u{1b}[3m"));
    assert!(output.contains("\u{1b}[1m"));
}

struct MarkdownWithInput {
    markdown: Markdown,
    line_count: Rc<RefCell<usize>>,
}

impl Component for MarkdownWithInput {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = self.markdown.render(width);
        *self.line_count.borrow_mut() = lines.len();
        lines.push("INPUT".into());
        lines
    }

    fn invalidate(&mut self) {
        self.markdown.invalidate();
    }
}

#[test]
fn does_not_leak_styles_into_following_lines_in_tui() {
    let _guard = lock();
    let markdown = markdown_with(
        "This is thinking with `inline code`",
        1,
        0,
        Some(DefaultTextStyle {
            color: Some(color_style(GRAY)),
            italic: true,
            ..Default::default()
        }),
        MarkdownOptions::default(),
    );
    let line_count = Rc::new(RefCell::new(0usize));
    let wrapper = MarkdownWithInput {
        markdown,
        line_count: line_count.clone(),
    };

    let (terminal, handle) = concrete(80, 6);
    let tui = TUI::new(handle);
    tui.add_child(component(wrapper));
    tui.start().expect("tui starts");
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();

    assert!(*line_count.borrow() > 0);
    let input_row = *line_count.borrow();
    assert!(!terminal.borrow().cell_italic(input_row, 0));
    tui.stop().unwrap();
}

#[test]
fn has_only_one_blank_line_between_code_block_and_paragraph() {
    let _guard = lock();
    let mut markdown = markdown("hello world\n\n```js\nconst hello = \"world\";\n```\n\nagain, hello world");
    let lines = plain(&mut markdown, 80);
    let closing = lines.iter().position(|line| line == "```").expect("closing");
    let empty_line_count = lines[closing + 1..]
        .iter()
        .position(|line| !line.is_empty())
        .expect("following paragraph");
    assert_eq!(empty_line_count, 1);
}

#[test]
fn normalizes_paragraph_and_code_block_spacing_to_one_blank_line() {
    let _guard = lock();
    let cases = [
        "hello this is text\n```\ncode block\n```\nmore text",
        "hello this is text\n\n```\ncode block\n```\n\nmore text",
    ];
    let expected = vec!["hello this is text", "", "```", "  code block", "```", "", "more text"];
    for text in cases {
        let mut markdown = markdown(text);
        assert_eq!(plain(&mut markdown, 80), expected, "{text:?}");
    }
}

#[test]
fn does_not_add_trailing_blank_line_when_code_block_is_last_block() {
    let _guard = lock();
    for text in [
        "```js\nconst hello = 'world';\n```",
        "hello world\n\n```js\nconst hello = 'world';\n```",
    ] {
        let mut markdown = markdown(text);
        let lines = plain(&mut markdown, 80);
        assert_ne!(lines.last().cloned(), Some(JsString::default()), "{text:?}");
    }
}

#[test]
fn has_only_one_blank_line_between_divider_and_paragraph() {
    let _guard = lock();
    let mut markdown = markdown("hello world\n\n---\n\nagain, hello world");
    let lines = plain(&mut markdown, 80);
    let divider = lines.iter().position(|line| line.contains('─')).expect("divider");
    let empty_line_count = lines[divider + 1..]
        .iter()
        .position(|line| !line.is_empty())
        .expect("following paragraph");
    assert_eq!(empty_line_count, 1);
}

#[test]
fn does_not_add_trailing_blank_line_when_divider_is_last_block() {
    let _guard = lock();
    let mut markdown = markdown("---");
    let lines = plain(&mut markdown, 80);
    assert_ne!(lines.last().cloned(), Some(JsString::default()));
}

#[test]
fn has_only_one_blank_line_between_heading_and_paragraph() {
    let _guard = lock();
    let mut markdown = markdown("# Hello\n\nThis is a paragraph");
    let lines = plain(&mut markdown, 80);
    let heading = lines.iter().position(|line| line.contains("Hello")).expect("heading");
    let empty_line_count = lines[heading + 1..]
        .iter()
        .position(|line| !line.is_empty())
        .expect("following paragraph");
    assert_eq!(empty_line_count, 1);
}

#[test]
fn does_not_add_trailing_blank_line_when_heading_is_last_block() {
    let _guard = lock();
    let mut markdown = markdown("# Hello");
    let lines = plain(&mut markdown, 80);
    assert_ne!(lines.last().cloned(), Some(JsString::default()));
}

#[test]
fn has_only_one_blank_line_between_blockquote_and_paragraph() {
    let _guard = lock();
    let mut markdown = markdown("hello world\n\n> This is a quote\n\nagain, hello world");
    let lines = plain(&mut markdown, 80);
    let quote = lines
        .iter()
        .position(|line| line.contains("This is a quote"))
        .expect("quote");
    let empty_line_count = lines[quote + 1..]
        .iter()
        .position(|line| !line.is_empty())
        .expect("following paragraph");
    assert_eq!(empty_line_count, 1);
}

#[test]
fn does_not_add_trailing_blank_line_when_blockquote_is_last_block() {
    let _guard = lock();
    let mut markdown = markdown("> This is a quote");
    let lines = plain(&mut markdown, 80);
    assert_ne!(lines.last().cloned(), Some(JsString::default()));
}

#[test]
fn applies_consistent_styling_to_lazy_continuation_blockquote() {
    let _guard = lock();
    let mut markdown = markdown_with(
        ">Foo\nbar",
        0,
        0,
        Some(DefaultTextStyle {
            color: Some(color_style(MAGENTA)),
            ..Default::default()
        }),
        MarkdownOptions::default(),
    );
    let lines = render(&mut markdown, 80);
    let plain_lines: Vec<JsString> = lines.iter().map(strip_sgr).collect();
    let quoted = plain_lines.iter().filter(|line| line.starts_with("│ ")).count();
    assert_eq!(quoted, 2, "{plain_lines:?}");
    let foo = lines.iter().find(|line| line.contains("Foo")).expect("Foo");
    let bar = lines.iter().find(|line| line.contains("bar")).expect("bar");
    assert!(foo.contains("\u{1b}[3m"), "{foo:?}");
    assert!(bar.contains("\u{1b}[3m"), "{bar:?}");
    assert!(!foo.contains("\u{1b}[35m"), "{foo:?}");
    assert!(!bar.contains("\u{1b}[35m"), "{bar:?}");
}

#[test]
fn applies_consistent_styling_to_explicit_multiline_blockquote() {
    let _guard = lock();
    let mut markdown = markdown_with(
        ">Foo\n>bar",
        0,
        0,
        Some(DefaultTextStyle {
            color: Some(color_style(CYAN)),
            ..Default::default()
        }),
        MarkdownOptions::default(),
    );
    let lines = render(&mut markdown, 80);
    let plain_lines: Vec<JsString> = lines.iter().map(strip_sgr).collect();
    let quoted = plain_lines.iter().filter(|line| line.starts_with("│ ")).count();
    assert_eq!(quoted, 2, "{plain_lines:?}");
    let foo = lines.iter().find(|line| line.contains("Foo")).expect("Foo");
    let bar = lines.iter().find(|line| line.contains("bar")).expect("bar");
    assert!(foo.contains("\u{1b}[3m"), "{foo:?}");
    assert!(bar.contains("\u{1b}[3m"), "{bar:?}");
    assert!(!foo.contains("\u{1b}[36m"), "{foo:?}");
    assert!(!bar.contains("\u{1b}[36m"), "{bar:?}");
}

#[test]
fn renders_list_content_inside_blockquotes() {
    let _guard = lock();
    let mut markdown = markdown("> 1. bla bla\n> - nested bullet");
    let lines = plain(&mut markdown, 80);
    let quoted: Vec<&JsString> = lines.iter().filter(|line| line.starts_with("│ ")).collect();
    assert!(quoted.iter().any(|line| line.contains("1. bla bla")), "{quoted:?}");
    assert!(quoted.iter().any(|line| line.contains("- nested bullet")), "{quoted:?}");
}

#[test]
fn wraps_long_blockquote_lines_and_adds_border_to_each() {
    let _guard = lock();
    let long_text = "This is a very long blockquote line that should wrap to multiple lines when rendered";
    let mut markdown = markdown(&format!("> {long_text}"));
    let lines = plain(&mut markdown, 30);
    let content: Vec<&JsString> = lines.iter().filter(|line| !line.is_empty()).collect();
    assert!(content.len() > 1, "{content:?}");
    for line in &content {
        assert!(line.starts_with("│ "), "{line:?}");
    }
    let all_text = JsString::join(content, " ");
    assert!(all_text.contains("very long"));
    assert!(all_text.contains("blockquote"));
    assert!(all_text.contains("multiple"));
}

#[test]
fn properly_indents_wrapped_blockquote_lines_with_styling() {
    let _guard = lock();
    let mut markdown = markdown_with(
        "> This is styled text that is long enough to wrap",
        0,
        0,
        Some(DefaultTextStyle {
            color: Some(color_style(YELLOW)),
            italic: true,
            ..Default::default()
        }),
        MarkdownOptions::default(),
    );
    let lines = render(&mut markdown, 25);
    let plain_lines: Vec<JsString> = lines.iter().map(|line| strip_sgr(line).trim_end()).collect();
    for line in plain_lines.iter().filter(|line| !line.is_empty()) {
        assert!(line.starts_with("│ "), "{line:?}");
    }
    let output = JsString::join(&lines, "\n");
    assert!(output.contains("\u{1b}[3m"));
    assert!(!output.contains("\u{1b}[33m"));
}

#[test]
fn renders_inline_formatting_inside_blockquotes_and_reapplies_quote_styling() {
    let _guard = lock();
    let mut markdown = markdown("> Quote with **bold** and `code`");
    let lines = render(&mut markdown, 80);
    let plain_lines: Vec<JsString> = lines.iter().map(strip_sgr).collect();
    assert!(plain_lines.iter().any(|line| line.starts_with("│ ")));
    let all_plain = JsString::join(&plain_lines, " ");
    assert!(all_plain.contains("Quote with"));
    assert!(all_plain.contains("bold"));
    assert!(all_plain.contains("code"));
    let output = JsString::join(&lines, "\n");
    assert!(output.contains("\u{1b}[1m"));
    assert!(output.contains("\u{1b}[33m"));
    assert!(output.contains("\u{1b}[3m"));
}

#[test]
fn preserves_heading_styling_after_inline_code() {
    let _guard = lock();
    let mut markdown = markdown("### Why `sourceInfo` should not be optional");
    let output = joined(&mut markdown, 80);
    assert!(output.contains("\u{1b}[33m"));
    let after_code = output.find("should not be optional").expect("text after code");
    let preceding = output.slice(after_code.saturating_sub(40), after_code);
    assert!(preceding.contains("\u{1b}[1m"), "{preceding:?}");
    assert!(preceding.contains("\u{1b}[36m"), "{preceding:?}");
}

#[test]
fn preserves_heading_styling_after_inline_code_for_h1() {
    let _guard = lock();
    let mut markdown = markdown("# Title with `code` inside");
    let output = joined(&mut markdown, 80);
    let after_code = output.find("inside").expect("text after code");
    let preceding = output.slice(after_code.saturating_sub(40), after_code);
    assert!(preceding.contains("\u{1b}[1m"), "{preceding:?}");
    assert!(preceding.contains("\u{1b}[36m"), "{preceding:?}");
    assert!(preceding.contains("\u{1b}[4m"), "{preceding:?}");
}

#[test]
fn does_not_leak_h1_underline_into_padding() {
    let _guard = lock();
    let markdown = markdown("# Important distinction from `open()`");
    let (terminal, handle) = concrete(80, 4);
    let tui = TUI::new(handle);
    tui.add_child(component(markdown));
    tui.start().expect("tui starts");
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();

    let first_row = terminal.borrow().viewport()[0].clone();
    let content_width = first_row.trim_end().len();
    assert!(content_width > 0);
    for column in content_width..80 {
        assert!(
            !terminal.borrow().cell_underline(0, column),
            "underline at column {column}"
        );
    }
    tui.stop().unwrap();
}

#[test]
fn preserves_heading_styling_after_bold_text() {
    let _guard = lock();
    let mut markdown = markdown("## Heading with **bold** and more");
    let output = joined(&mut markdown, 80);
    let after_bold = output.find("and more").expect("text after bold");
    let preceding = output.slice(after_bold.saturating_sub(40), after_bold);
    assert!(preceding.contains("\u{1b}[1m"), "{preceding:?}");
    assert!(preceding.contains("\u{1b}[36m"), "{preceding:?}");
}

#[test]
fn renders_strikethrough() {
    let _guard = lock();
    let mut markdown = markdown("Use ~~strikethrough~~ here");
    let lines = render(&mut markdown, 80);
    let output = JsString::join(&lines, "\n");
    let plain_text = JsString::join(lines.iter().map(strip_sgr), " ");
    assert!(output.contains("\u{1b}[9m"));
    assert!(plain_text.contains("strikethrough"));
    assert!(!plain_text.contains("~~strikethrough~~"));
}

#[test]
fn keeps_single_tilde_as_plain_text() {
    let _guard = lock();
    let mut markdown = markdown("Use ~strikethrough~ literally");
    let lines = render(&mut markdown, 80);
    let output = JsString::join(&lines, "\n");
    let plain_text = JsString::join(lines.iter().map(strip_sgr), " ");
    assert!(plain_text.contains("~strikethrough~"));
    assert!(!output.contains("\u{1b}[9m"));
}

fn without_hyperlinks() {
    set_capabilities(TerminalCapabilities {
        images: None,
        true_color: false,
        hyperlinks: false,
    });
}

fn with_hyperlinks() {
    set_capabilities(TerminalCapabilities {
        images: None,
        true_color: false,
        hyperlinks: true,
    });
}

#[test]
fn does_not_duplicate_url_for_autolinked_emails() {
    let _guard = lock();
    without_hyperlinks();
    let mut markdown = markdown("Contact user@example.com for help");
    let plain_text = JsString::join(plain(&mut markdown, 80), " ");
    reset_capabilities_cache();
    assert!(plain_text.contains("user@example.com"));
    assert!(!plain_text.contains("mailto:"));
}

#[test]
fn does_not_duplicate_url_for_bare_urls() {
    let _guard = lock();
    without_hyperlinks();
    let mut markdown = markdown("Visit https://example.com for more");
    let plain_text = JsString::join(plain(&mut markdown, 80), " ");
    reset_capabilities_cache();
    assert_eq!(
        plain_text
            .units()
            .windows("https://example.com".len())
            .filter(|window| window.iter().copied().eq("https://example.com".encode_utf16()))
            .count(),
        1
    );
}

#[test]
fn shows_url_in_parentheses_when_hyperlinks_are_not_supported() {
    let _guard = lock();
    without_hyperlinks();
    let mut markdown = markdown("[click here](https://example.com)");
    let plain_text = JsString::join(plain(&mut markdown, 80), " ");
    reset_capabilities_cache();
    assert!(plain_text.contains("click here"));
    assert!(plain_text.contains("(https://example.com)"));
}

#[test]
fn shows_mailto_url_in_parentheses_when_hyperlinks_are_not_supported() {
    let _guard = lock();
    without_hyperlinks();
    let mut markdown = markdown("[Email me](mailto:test@example.com)");
    let plain_text = JsString::join(plain(&mut markdown, 80), " ");
    reset_capabilities_cache();
    assert!(plain_text.contains("Email me"));
    assert!(plain_text.contains("(mailto:test@example.com)"));
}

#[test]
fn emits_osc8_hyperlink_when_supported() {
    let _guard = lock();
    with_hyperlinks();
    let mut markdown = markdown("[click here](https://example.com)");
    let output = JsString::join(render(&mut markdown, 80), "");
    reset_capabilities_cache();
    assert!(output.contains("\u{1b}]8;;https://example.com\u{1b}\\"));
    assert!(output.contains("\u{1b}]8;;\u{1b}\\"));
    let without_osc = strip_osc8(&output);
    assert!(without_osc.contains("click here"));
    assert!(!without_osc.contains("(https://example.com)"));
}

fn strip_osc8(text: &JsString) -> JsString {
    let mut result = JsString::default();
    let mut rest = text.clone();
    while let Some(start) = rest.find("\x1b]8;;") {
        result.push_str(rest.slice(0, start));
        let after = rest.slice(start + 5, rest.len());
        if let Some(end) = after.find("\x1b\\") {
            rest = after.slice(end + 2, after.len());
        } else {
            return result;
        }
    }
    result + rest
}

#[test]
fn uses_osc8_for_mailto_links_when_supported() {
    let _guard = lock();
    with_hyperlinks();
    let mut markdown = markdown("[Email me](mailto:test@example.com)");
    let output = JsString::join(render(&mut markdown, 80), "");
    reset_capabilities_cache();
    assert!(output.contains("\u{1b}]8;;mailto:test@example.com\u{1b}\\"));
    assert!(output.contains("\u{1b}]8;;\u{1b}\\"));
}

#[test]
fn uses_osc8_for_bare_urls_when_supported() {
    let _guard = lock();
    with_hyperlinks();
    let mut markdown = markdown("Visit https://example.com for more");
    let output = JsString::join(render(&mut markdown, 80), "");
    reset_capabilities_cache();
    assert!(output.contains("\u{1b}]8;;https://example.com\u{1b}\\"));
    assert!(!strip_osc8(&output).contains("(https://example.com)"));
}

#[test]
fn renders_content_with_html_like_tags_as_text() {
    let _guard = lock();
    let mut markdown = markdown("This is text with <thinking>hidden content</thinking> that should be visible");
    let plain_text = JsString::join(plain(&mut markdown, 80), " ");
    assert!(
        plain_text.contains("hidden content") || plain_text.contains("<thinking>"),
        "{plain_text:?}"
    );
}

#[test]
fn renders_html_tags_in_code_blocks() {
    let _guard = lock();
    let mut markdown = markdown("```html\n<div>Some HTML</div>\n```");
    let output = JsString::join(plain(&mut markdown, 80), "\n");
    assert!(output.contains("<div>"));
    assert!(output.contains("</div>"));
}

#[test]
fn stabilizes_partial_closing_fence_rendering() {
    let _guard = lock();
    let cases = [
        ("```ts\nconst x = 1;\n``", vec!["```ts", "  const x = 1;", "```"]),
        (
            "```md\nnot a closing fence:\n``\n```",
            vec!["```md", "  not a closing fence:", "  ``", "```"],
        ),
        ("```ts\n``", vec!["```ts", "", "```"]),
        ("````\n```", vec!["```", "", "```"]),
        ("~~~~~\n~~~~", vec!["```", "", "```"]),
        (
            "```md\nnot a closing fence:\n``\n```\n\nafter",
            vec!["```md", "  not a closing fence:", "  ``", "```", "", "after"],
        ),
    ];
    for (input, expected) in cases {
        let mut markdown = markdown(input);
        assert_eq!(plain(&mut markdown, 80), expected, "{input:?}");
    }

    let mut partial = markdown("```ts\nconst x = 1;\n``");
    let mut complete = markdown("```ts\nconst x = 1;\n```");
    assert_eq!(render(&mut partial, 80).len(), render(&mut complete, 80).len());
}

#[test]
fn theme_is_used_for_all_default_styles() {
    let _guard = lock();
    // Guard against accidentally wiring the wrong theme method.
    let theme = ChalkTheme;
    assert_eq!(theme.bold(&"x".into()), apply_chalk(&[markdown_theme::BOLD], "x"));
    assert_eq!(theme.hr(&"x".into()), apply_chalk(&[markdown_theme::DIM], "x"));
    assert_eq!(theme.code(&"x".into()), apply_chalk(&[YELLOW], "x"));
    assert_eq!(
        theme.quote_border(&"x".into()),
        apply_chalk(&[markdown_theme::DIM], "x")
    );
    assert_eq!(theme.list_bullet(&"x".into()), apply_chalk(&[CYAN], "x"));
    assert_eq!(
        theme.code_block(&"x".into()),
        apply_chalk(&[markdown_theme::GREEN], "x")
    );
    assert_eq!(
        theme.code_block_border(&"x".into()),
        apply_chalk(&[markdown_theme::DIM], "x")
    );
    assert_eq!(theme.link_url(&"x".into()), apply_chalk(&[markdown_theme::DIM], "x"));
    assert_eq!(
        theme.underline(&"x".into()),
        apply_chalk(&[markdown_theme::UNDERLINE], "x")
    );
    assert_eq!(
        theme.strikethrough(&"x".into()),
        apply_chalk(&[markdown_theme::STRIKETHROUGH], "x")
    );
    assert_eq!(theme.italic(&"x".into()), apply_chalk(&[markdown_theme::ITALIC], "x"));
    assert_eq!(
        theme.heading(&"x".into()),
        apply_chalk(&[markdown_theme::BOLD, CYAN], "x")
    );
}
