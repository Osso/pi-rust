use pi_js_compat::js_json::JsString;
use std::cell::Cell;
use std::rc::Rc;

use pi_tui::components::text::Text;
use pi_tui::tui::Component;
use pi_tui::utils::visible_width;

#[test]
fn renders_nothing_for_empty_or_whitespace_text() {
    let mut empty = Text::default();
    assert!(empty.render(10).is_empty());

    let mut whitespace = Text::new(" \t\n", 1, 1);
    assert!(whitespace.render(10).is_empty());
}

#[test]
fn pads_content_lines_and_vertical_padding_to_width() {
    let mut text = Text::new("Hello", 1, 1);
    let lines = text.render(10);

    assert_eq!(lines.len(), 3);
    for line in &lines {
        assert_eq!(visible_width(line), 10);
    }
    assert_eq!(lines[1], " Hello    ");
}

#[test]
fn wraps_words_and_expands_tabs() {
    let mut text = Text::new("alpha\tbeta gamma", 0, 0);
    let lines = text.render(12);

    assert_eq!(lines[0], "alpha   beta");
    assert_eq!(lines[1], "gamma       ");
}

#[test]
fn applies_background_to_content_and_padding_lines() {
    let background = Rc::new(|line: &JsString| JsString::from("[") + line + "]");
    let mut text = Text::with_background("hi", 1, 1, background);
    let lines = text.render(6);

    assert_eq!(
        lines,
        vec!["[      ]".to_string(), "[ hi   ]".to_string(), "[      ]".to_string()]
    );
}

#[test]
fn caches_render_until_text_or_width_changes() {
    let mut text = Text::new("one", 0, 0);
    assert_eq!(text.render(10), vec!["one       ".to_string()]);
    assert_eq!(text.render(10), vec!["one       ".to_string()]);

    text.set_text("two");
    assert_eq!(text.render(10), vec!["two       ".to_string()]);
    assert_eq!(text.render(4), vec!["two ".to_string()]);
}

#[test]
fn background_change_invalidates_cache() {
    let calls = Rc::new(Cell::new(0));
    let counter = Rc::clone(&calls);
    let mut text = Text::new("x", 0, 0);
    assert_eq!(text.render(3), vec!["x  ".to_string()]);

    let counter_ref = Rc::clone(&counter);
    text.set_custom_bg_fn(Some(Rc::new(move |line: &JsString| {
        counter_ref.set(counter_ref.get() + 1);
        JsString::from("(") + line + ")"
    })));
    assert_eq!(text.render(3), vec!["(x  )".to_string()]);
    assert_eq!(calls.get(), 1);
}
