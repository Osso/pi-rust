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
