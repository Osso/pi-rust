use std::rc::Rc;

use pi_tui::terminal_image::{ImageDimensions, TerminalCapabilities, set_capabilities};
use pi_tui::{Component, Image, ImageOptions, ImageTheme, Text};

#[test]
fn text_padding_mutation_reflows_and_invalidates_cached_output() {
    let mut text = Text::new("hello world", 0, 0);
    assert_eq!(text.render(12), ["hello world "]);
    text.set_padding(1, 1);
    assert_eq!(
        text.render(12),
        ["            ", " hello      ", " world      ", "            "]
    );
    text.set_padding(1, 1);
    assert_eq!(text.render(12).len(), 4);
}

#[test]
fn image_fallback_is_not_truncated_or_hyperlinked_by_the_component() {
    set_capabilities(TerminalCapabilities {
        images: None,
        true_color: true,
        hyperlinks: true,
    });
    let mut image = Image::new(
        "AAAA",
        "image/png",
        Rc::new(ImageTheme {
            fallback_color: Rc::new(|text| format!("<{text}>")),
        }),
        ImageOptions {
            filename: Some("/tmp/image.png".into()),
            ..ImageOptions::default()
        },
        Some(ImageDimensions {
            width_px: 10,
            height_px: 20,
        }),
    );
    assert_eq!(image.render(4), ["<[Image: /tmp/image.png [image/png] 10x20]>"]);
}

struct HighlightTheme(std::cell::Cell<usize>);
impl pi_tui::MarkdownTheme for HighlightTheme {
    fn heading(&self, text: &str) -> String {
        text.to_string()
    }
    fn link(&self, text: &str) -> String {
        text.to_string()
    }
    fn link_url(&self, text: &str) -> String {
        text.to_string()
    }
    fn code(&self, text: &str) -> String {
        text.to_string()
    }
    fn code_block(&self, text: &str) -> String {
        text.to_string()
    }
    fn code_block_border(&self, text: &str) -> String {
        text.to_string()
    }
    fn quote(&self, text: &str) -> String {
        text.to_string()
    }
    fn quote_border(&self, text: &str) -> String {
        text.to_string()
    }
    fn hr(&self, text: &str) -> String {
        text.to_string()
    }
    fn list_bullet(&self, text: &str) -> String {
        text.to_string()
    }
    fn bold(&self, text: &str) -> String {
        text.to_string()
    }
    fn italic(&self, text: &str) -> String {
        text.to_string()
    }
    fn strikethrough(&self, text: &str) -> String {
        text.to_string()
    }
    fn underline(&self, text: &str) -> String {
        text.to_string()
    }

    fn highlight_code(&self, code: &str, _: Option<&str>) -> Option<Vec<String>> {
        self.0.set(self.0.get() + 1);
        Some(vec![format!("highlight {}: {code}", self.0.get())])
    }
}

#[test]
fn streaming_markdown_reuses_unchanged_tokens_but_theme_change_invalidates_them() {
    let theme: Rc<dyn pi_tui::MarkdownTheme> = Rc::new(HighlightTheme(std::cell::Cell::new(0)));
    let mut md = pi_tui::Markdown::with_shared_theme(
        "```rs\nx\n```\n\nend",
        0,
        0,
        theme.clone(),
        None,
        pi_tui::MarkdownOptions::default(),
    );
    assert!(md.render(40).iter().any(|line| line.trim() == "highlight 1: x"));
    md.set_text("```rs\nx\n```\n\nend grows");
    assert!(md.render(40).iter().any(|line| line.trim() == "highlight 1: x"));
    md.set_padding(0, 0);
    md.set_theme(theme.clone());
    assert!(md.render(40).iter().any(|line| line.trim() == "highlight 1: x"));
    md.set_padding(1, 0);
    assert!(md.render(40).iter().any(|line| line.trim() == "highlight 2: x"));
    md.set_theme(Rc::new(HighlightTheme(std::cell::Cell::new(9))));
    assert!(md.render(40).iter().any(|line| line.trim() == "highlight 10: x"));
    md.invalidate();
    assert!(md.render(40).iter().any(|line| line.trim() == "highlight 11: x"));
}

#[test]
fn zero_argument_text_and_box_use_frozen_one_cell_padding() {
    let mut text = Text::default();
    text.set_text("hi");
    assert_eq!(text.render(4), ["    ", " hi ", "    "]);
}

#[test]
fn zero_argument_box_uses_frozen_one_cell_padding() {
    let mut bx = pi_tui::Box::default();
    bx.add_child(pi_tui::tui::component(Text::new("hi", 0, 0)));
    assert_eq!(bx.render(4), ["    ", " hi ", "    "]);
}

#[test]
fn heading_context_does_not_call_default_foreground_style() {
    let calls = Rc::new(std::cell::Cell::new(0));
    let counter = calls.clone();
    let mut md = pi_tui::Markdown::with_shared_theme(
        "# heading",
        0,
        0,
        Rc::new(HighlightTheme(std::cell::Cell::new(0))),
        Some(pi_tui::DefaultTextStyle {
            color: Some(Box::new(move |text| {
                counter.set(counter.get() + 1);
                text.to_string()
            })),
            ..Default::default()
        }),
        pi_tui::MarkdownOptions::default(),
    );
    assert_eq!(md.render(10), ["heading   "]);
    assert_eq!(calls.get(), 0);
}

#[path = "support/oracle.rs"]
mod oracle;

#[test]
fn component_lifetimes_match_the_frozen_modules() {
    let mut oracle = oracle::TsOracle::spawn("markdown-oracle.ts", "packages/tui/src");
    let response = oracle.request(&serde_json::json!({"id": 1, "operation": "markdown_lifecycle"}));
    let mut input = pi_tui::Input::new(Default::default());
    input.set_value("hello");
    let mut selection = pi_tui::SelectList::new(
        vec![pi_tui::SelectItem {
            value: "alpha".into(),
            label: "alpha".into(),
            description: Some(" \r\n\u{feff} ".into()),
        }],
        5,
        pi_tui::SelectListTheme::plain(),
        Default::default(),
    );
    assert_eq!(response["ok"], true, "{response}");
    let theme: Rc<dyn pi_tui::MarkdownTheme> = Rc::new(HighlightTheme(std::cell::Cell::new(0)));
    let mut md = pi_tui::Markdown::with_shared_theme(
        "```rs\nx\n```\n\nend",
        0,
        0,
        theme.clone(),
        None,
        pi_tui::MarkdownOptions::default(),
    );
    let mut snapshots = vec![md.render(40)];
    md.set_text("```rs\nx\n```\n\nend grows");
    snapshots.push(md.render(40));
    md.set_padding(0, 0);
    md.set_theme(theme.clone());
    snapshots.push(md.render(40));
    md.set_padding(1, 0);
    snapshots.push(md.render(40));
    md.set_theme(Rc::new(HighlightTheme(std::cell::Cell::new(9))));
    snapshots.push(md.render(40));
    md.invalidate();
    snapshots.push(md.render(40));
    let calls = Rc::new(std::cell::Cell::new(0));
    let counter = calls.clone();
    let mut heading = pi_tui::Markdown::with_shared_theme(
        "# heading",
        0,
        0,
        theme,
        Some(pi_tui::DefaultTextStyle {
            color: Some(Box::new(move |text| {
                counter.set(counter.get() + 1);
                text.to_string()
            })),
            ..Default::default()
        }),
        pi_tui::MarkdownOptions::default(),
    );
    let mut text = Text::default();
    text.set_text("hi");
    let mut bx = pi_tui::Box::default();
    bx.add_child(pi_tui::tui::component(Text::new("hi", 0, 0)));
    assert_eq!(
        serde_json::json!({"snapshots": snapshots, "heading": heading.render(10), "colorCalls": calls.get(), "defaultText": text.render(4), "defaultBox": bx.render(4), "narrowInput": input.render(2), "emptyDescription": selection.render(80)}),
        response["output"]
    );
}

#[test]
fn input_at_prompt_width_does_not_add_a_fake_cursor() {
    let mut input = pi_tui::Input::new(Default::default());
    input.set_value("hello");
    assert_eq!(input.render(2), ["> "]);
}

#[test]
fn empty_normalized_description_does_not_create_a_description_column() {
    use pi_tui::components::select_list::{SelectItem, SelectList, SelectListLayoutOptions, SelectListTheme};
    let mut list = SelectList::new(
        vec![SelectItem {
            value: "alpha".into(),
            label: "alpha".into(),
            description: Some(" \r\n\u{feff} ".into()),
        }],
        5,
        SelectListTheme::plain(),
        SelectListLayoutOptions::default(),
    );
    assert_eq!(list.render(80), ["→ alpha"]);
}
