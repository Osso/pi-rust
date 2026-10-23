//! Port of `packages/tui/test/bug-regression-isimageline-startswith-bug.test.ts`.
//!
//! `isImageLine` must detect image escape sequences anywhere in a line so the
//! Markdown wrapper skips width checks on huge base64 payloads.

use pi_js_compat::js_json::JsString;
use pi_tui::components::markdown::{Markdown, MarkdownOptions};
use pi_tui::terminal_image::is_image_line;
use pi_tui::tui::Component;

fn image_line_with_text() -> String {
    "Read image file [image/jpeg]\u{1b}]1337;File=size=800,600;inline=1:base64data...\u{7}".to_string()
}

#[test]
fn detects_iterm2_sequence_with_leading_text() {
    assert!(is_image_line(image_line_with_text()));
}

#[test]
fn detects_kitty_sequences_in_any_position() {
    let long_line = format!("Text before \u{1b}_Ga=T,f=100{} text after", "A".repeat(300_000));
    let scenarios = [
        "At start: \u{1b}_Ga=T,f=100,data...\u{1b}\\".to_string(),
        "Prefix \u{1b}_Ga=T,data...\u{1b}\\".to_string(),
        "Suffix text \u{1b}_Ga=T,data...\u{1b}\\ suffix".to_string(),
        "Middle \u{1b}_Ga=T,data...\u{1b}\\ more text".to_string(),
        long_line,
    ];
    for line in scenarios {
        assert!(is_image_line(&line), "{}", &line[..line.len().min(50)]);
    }
}

#[test]
fn detects_iterm2_sequences_in_any_position() {
    let long_line = format!(
        "Text before \u{1b}]1337;File=size=800,600;inline=1:{} text after",
        "B".repeat(300_000)
    );
    let scenarios = [
        "At start: \u{1b}]1337;File=size=100,100:base64...\u{7}".to_string(),
        "Prefix \u{1b}]1337;File=inline=1:data==\u{7}".to_string(),
        "Suffix text \u{1b}]1337;File=inline=1:data==\u{7} suffix".to_string(),
        "Middle \u{1b}]1337;File=inline=1:data==\u{7} more text".to_string(),
        long_line,
    ];
    for line in scenarios {
        assert!(is_image_line(&line), "{}", &line[..line.len().min(50)]);
    }
}

#[test]
fn detects_image_sequences_in_read_tool_output() {
    let tool_output = "Read image file [image/jpeg]\u{1b}]1337;File=size=800,600;inline=1:base64image...\u{7}";
    assert!(is_image_line(tool_output));
}

#[test]
fn detects_kitty_sequences_from_image_component() {
    let kitty_line = "\u{1b}_Ga=T,f=100,t=f,d=base64data...\u{1b}\\\u{1b}_Gm=i=1;\u{1b}\\";
    assert!(is_image_line(kitty_line));
}

#[test]
fn handles_ansi_codes_before_image_sequences() {
    let lines = [
        "\u{1b}[31mError\u{1b}[0m: \u{1b}]1337;File=inline=1:base64==\u{7}",
        "\u{1b}[33mWarning\u{1b}[0m: \u{1b}_Ga=T,data...\u{1b}\\",
        "\u{1b}[1mBold\u{1b}[0m \u{1b}]1337;File=:base64==\u{7}\u{1b}[0m",
    ];
    for line in lines {
        assert!(is_image_line(line), "{}", &line[..line.len().min(30)]);
    }
}

#[test]
fn does_not_crash_on_very_long_lines_with_image_sequences() {
    let crash_line = format!(
        "Output: \u{1b}]1337;File=size=800,600;inline=1:{} end of output",
        "A".repeat(100).repeat(3040)
    );
    assert!(crash_line.len() > 300_000);
    assert!(is_image_line(&crash_line));
}

#[test]
fn handles_lines_matching_crash_log_dimensions() {
    let target_width = 58_649;
    let prefix = "Text";
    let sequence = "\u{1b}_Ga=T,f=100";
    let suffix = "End";
    let padding = "A".repeat(target_width - prefix.len() - sequence.len() - suffix.len());
    let line = format!("{prefix}{sequence}{padding}{suffix}");
    assert_eq!(line.len(), target_width);
    assert!(is_image_line(&line));
}

#[test]
fn does_not_detect_images_in_regular_long_text() {
    let long_text = "A".repeat(100_000);
    assert!(!is_image_line(&long_text));
}

#[test]
fn does_not_detect_images_in_file_paths() {
    for path in [
        "/path/to/1337/image.jpg",
        "/usr/local/bin/File_converter",
        "~/Documents/1337File_backup.png",
        "./_G_test_file.txt",
    ] {
        assert!(!is_image_line(path), "{path}");
    }
}

#[test]
fn markdown_does_not_wrap_image_lines() {
    let line = format!("Output: \u{1b}]1337;File=size=800,600;inline=1:{} end", "A".repeat(400));
    let mut markdown = Markdown::new(
        line.clone(),
        0,
        0,
        Box::new(NoopTheme),
        None,
        MarkdownOptions::default(),
    );
    let rendered = markdown.render(20);
    assert_eq!(rendered.len(), 1);
    assert!(rendered[0].starts_with("Output: \u{1b}]1337;File="));
}

struct NoopTheme;

impl pi_tui::components::markdown::MarkdownTheme for NoopTheme {
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
        text.clone()
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
}
