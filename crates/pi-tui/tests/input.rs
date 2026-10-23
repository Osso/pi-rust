use pi_js_compat::js_json::JsString;
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::tui::{Component, Focusable};
use pi_tui::utils::visible_width;

fn input() -> Input {
    Input::new(InputOptions::default())
}

#[test]
fn submits_value_including_backslash_on_enter() {
    let mut input = input();
    let submitted = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = std::rc::Rc::clone(&submitted);
    input.on_submit = Some(std::rc::Rc::new(move |value: &JsString| {
        sink.borrow_mut().push(value.clone());
    }));

    for character in ["h", "e", "l", "l", "o", "\\"] {
        input.handle_input(pi_js_compat::js_json::JsString::from(character));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\r"));

    assert_eq!(*submitted.borrow(), vec!["hello\\"]);
}

#[test]
fn inserts_backslash_as_regular_character() {
    let mut input = input();
    input.handle_input(pi_js_compat::js_json::JsString::from("\\"));
    input.handle_input(pi_js_compat::js_json::JsString::from("x"));
    assert_eq!(input.get_value(), "\\x");
}

#[test]
fn does_not_overflow_with_wide_cjk_and_fullwidth_text() {
    let width = 93;
    let cases = [
        "가나다라마바사아자차카타파하 한글 텍스트가 터미널 너비를 초과하면 크래시가 발생합니다 이것은 재현용 테스트입니다",
        "これはテスト文章です。日本語のテキストが正しく表示されるかどうかを確認するためのサンプルテキストです。あいうえお",
        "这是一段测试文本，用于验证中文字符在终端中的显示宽度是否被正确计算，如果不正确就会导致用户界面崩溃的问题",
        "ＡＢＣＤＥＦＧＨＩＪＫＬＭＮＯＰＱＲＳＴＵＶＷＸＹＺ０１２３４５６７８９ａｂｃｄｅｆｇｈｉｊｋｌｍ",
    ];
    for text in cases {
        for position in 0..3 {
            let mut input = input();
            input.set_value(text);
            input.set_focused(true);
            match position {
                1 => {
                    for _ in 0..10 {
                        input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[C"));
                    }
                }
                2 => input.handle_input(pi_js_compat::js_json::JsString::from("\x05")),
                _ => {}
            }

            let lines = input.render(width);
            assert!(visible_width(&lines[0]) <= width, "rendered line overflowed: {text}");
        }
    }
}

#[test]
fn keeps_the_cursor_visible_when_horizontally_scrolling_wide_text() {
    let mut input = input();
    input.set_value("가나다라마바사아자차카타파하");
    input.set_focused(true);
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    for _ in 0..5 {
        input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[C"));
    }

    let lines = input.render(20);
    assert!(visible_width(&lines[0]) <= 20);
}

#[test]
fn masks_secret_input_while_preserving_the_stored_value() {
    let mut input = Input::new(InputOptions { mask: true });
    input.set_value("s3cr3t");
    input.set_focused(true);

    let lines = input.render(20);
    assert_eq!(input.get_value(), "s3cr3t");
    let rendered = pi_tui::utils::strip_terminal_sequences(&lines[0]);
    assert!(!rendered.contains("s3cr3t"));
    assert!(rendered.contains("••••••"));
    assert_eq!(visible_width(&lines[0]), 20);
}

#[test]
fn ctrl_w_saves_deleted_text_to_kill_ring_and_ctrl_y_yanks_it() {
    let mut input = input();
    input.set_value("foo bar baz");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    assert_eq!(input.get_value(), "foo bar ");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "bazfoo bar ");
}

#[test]
fn ctrl_w_preserves_ascii_punctuation_boundaries() {
    let mut input = input();
    input.set_value("foo.bar");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    assert_eq!(input.get_value(), "foo.");

    input.set_value("foo:bar");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    assert_eq!(input.get_value(), "foo:");
}

#[test]
fn ctrl_w_handles_unicode_word_boundaries() {
    let mut input = input();
    input.set_value("你好世界。你好，世界");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    let expected = [
        "你好世界。你好，",
        "你好世界。你好",
        "你好世界。",
        "你好世界",
        "你好",
        "",
    ];
    for value in expected {
        input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
        assert_eq!(input.get_value(), value);
    }
}

#[test]
fn ctrl_u_saves_deleted_text_to_kill_ring() {
    let mut input = input();
    input.set_value("hello world");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    for _ in 0..6 {
        input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[C"));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\x15"));
    assert_eq!(input.get_value(), "world");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "hello world");
}

#[test]
fn ctrl_k_saves_deleted_text_to_kill_ring() {
    let mut input = input();
    input.set_value("hello world");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x0b"));
    assert_eq!(input.get_value(), "");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "hello world");
}

#[test]
fn ctrl_y_does_nothing_when_kill_ring_is_empty() {
    let mut input = input();
    input.set_value("test");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "test");
}

#[test]
fn alt_y_cycles_through_kill_ring_after_ctrl_y() {
    let mut input = input();
    for value in ["first", "second", "third"] {
        input.set_value(value);
        input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
        input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    }
    assert_eq!(input.get_value(), "");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "third");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1by"));
    assert_eq!(input.get_value(), "second");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1by"));
    assert_eq!(input.get_value(), "first");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1by"));
    assert_eq!(input.get_value(), "third");
}

#[test]
fn alt_y_does_nothing_if_not_preceded_by_yank() {
    let mut input = input();
    input.set_value("test");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    input.set_value("other");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("x"));
    assert_eq!(input.get_value(), "otherx");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x1by"));
    assert_eq!(input.get_value(), "otherx");
}

#[test]
fn alt_y_does_nothing_if_kill_ring_has_one_entry() {
    let mut input = input();
    input.set_value("only");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "only");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x1by"));
    assert_eq!(input.get_value(), "only");
}

#[test]
fn consecutive_ctrl_w_accumulates_into_one_kill_ring_entry() {
    let mut input = input();
    input.set_value("one two three");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    assert_eq!(input.get_value(), "");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "one two three");
}

#[test]
fn non_delete_actions_break_kill_accumulation() {
    let mut input = input();
    input.set_value("foo bar baz");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    assert_eq!(input.get_value(), "foo bar ");

    input.handle_input(pi_js_compat::js_json::JsString::from("x"));
    assert_eq!(input.get_value(), "foo bar x");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    assert_eq!(input.get_value(), "foo bar ");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "foo bar x");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1by"));
    assert_eq!(input.get_value(), "foo bar baz");
}

#[test]
fn non_yank_actions_break_alt_y_chain() {
    let mut input = input();
    input.set_value("first");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    input.set_value("second");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    input.set_value("");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "second");
    input.handle_input(pi_js_compat::js_json::JsString::from("x"));
    assert_eq!(input.get_value(), "secondx");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1by"));
    assert_eq!(input.get_value(), "secondx");
}

#[test]
fn kill_ring_rotation_persists_after_cycling() {
    let mut input = input();
    for value in ["first", "second", "third"] {
        input.set_value(value);
        input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
        input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    }
    input.set_value("");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1by"));
    assert_eq!(input.get_value(), "second");

    input.handle_input(pi_js_compat::js_json::JsString::from("x"));
    input.set_value("");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "second");
}

#[test]
fn backward_deletions_prepend_and_forward_deletions_append() {
    let mut input = input();
    input.set_value("prefix|suffix");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    for _ in 0..6 {
        input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[C"));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\x0b"));
    assert_eq!(input.get_value(), "prefix");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "prefix|suffix");
}

#[test]
fn alt_d_deletes_word_forward_and_saves_to_kill_ring() {
    let mut input = input();
    input.set_value("hello world test");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1bd"));
    assert_eq!(input.get_value(), " world test");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1bd"));
    assert_eq!(input.get_value(), " test");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "hello world test");
}

#[test]
fn alt_d_preserves_ascii_punctuation_boundaries() {
    let mut input = input();
    input.set_value("foo.bar baz");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1bd"));
    assert_eq!(input.get_value(), ".bar baz");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1bd"));
    assert_eq!(input.get_value(), "bar baz");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1bd"));
    assert_eq!(input.get_value(), " baz");
}

#[test]
fn alt_d_handles_unicode_word_boundaries() {
    let mut input = input();
    input.set_value("你好世界。你好，世界");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    let expected = ["世界。你好，世界", "。你好，世界", "你好，世界", "，世界", "世界", ""];
    for value in expected {
        input.handle_input(pi_js_compat::js_json::JsString::from("\x1bd"));
        assert_eq!(input.get_value(), value);
    }
}

#[test]
fn handles_yank_in_middle_of_text() {
    let mut input = input();
    input.set_value("word");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    input.set_value("hello world");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    for _ in 0..6 {
        input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[C"));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "hello wordworld");
}

#[test]
fn handles_yank_pop_in_middle_of_text() {
    let mut input = input();
    for value in ["FIRST", "SECOND"] {
        input.set_value(value);
        input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
        input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    }
    input.set_value("hello world");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    for _ in 0..6 {
        input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[C"));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "hello SECONDworld");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1by"));
    assert_eq!(input.get_value(), "hello FIRSTworld");
}

#[test]
fn undo_does_nothing_when_stack_is_empty() {
    let mut input = input();
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "");
}

#[test]
fn coalesces_consecutive_word_characters_into_one_undo_unit() {
    let mut input = input();
    for character in ["h", "e", "l", "l", "o", " ", "w", "o", "r", "l", "d"] {
        input.handle_input(pi_js_compat::js_json::JsString::from(character));
    }
    assert_eq!(input.get_value(), "hello world");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "hello");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "");
}

#[test]
fn undoes_spaces_one_at_a_time() {
    let mut input = input();
    for character in ["h", "e", "l", "l", "o", " ", " "] {
        input.handle_input(pi_js_compat::js_json::JsString::from(character));
    }
    assert_eq!(input.get_value(), "hello  ");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "hello ");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "hello");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "");
}

#[test]
fn undoes_backspace() {
    let mut input = input();
    for character in ["h", "e", "l", "l", "o", "\x7f"] {
        input.handle_input(pi_js_compat::js_json::JsString::from(character));
    }
    assert_eq!(input.get_value(), "hell");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "hello");
}

#[test]
fn undoes_forward_delete() {
    let mut input = input();
    for character in ["h", "e", "l", "l", "o"] {
        input.handle_input(pi_js_compat::js_json::JsString::from(character));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[C"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[3~"));
    assert_eq!(input.get_value(), "hllo");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "hello");
}

#[test]
fn undoes_delete_operations() {
    let mut input = input();
    for character in ["h", "e", "l", "l", "o", " ", "w", "o", "r", "l", "d"] {
        input.handle_input(pi_js_compat::js_json::JsString::from(character));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    assert_eq!(input.get_value(), "hello ");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "hello world");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    for _ in 0..6 {
        input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[C"));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\x0b"));
    assert_eq!(input.get_value(), "hello ");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "hello world");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    for _ in 0..6 {
        input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[C"));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\x15"));
    assert_eq!(input.get_value(), "world");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "hello world");
}

#[test]
fn undoes_yank() {
    let mut input = input();
    for character in ["h", "e", "l", "l", "o", " "] {
        input.handle_input(pi_js_compat::js_json::JsString::from(character));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\x17"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x19"));
    assert_eq!(input.get_value(), "hello ");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "");
}

#[test]
fn undoes_paste_atomically() {
    let mut input = input();
    input.set_value("hello world");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    for _ in 0..5 {
        input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[C"));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[200~beep boop\x1b[201~"));
    assert_eq!(input.get_value(), "hellobeep boop world");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "hello world");
}

#[test]
fn undoes_alt_d() {
    let mut input = input();
    input.set_value("hello world");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1bd"));
    assert_eq!(input.get_value(), " world");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "hello world");
}

#[test]
fn cursor_movement_starts_new_undo_unit() {
    let mut input = input();
    for character in ["a", "b", "c"] {
        input.handle_input(pi_js_compat::js_json::JsString::from(character));
    }
    input.handle_input(pi_js_compat::js_json::JsString::from("\x01"));
    input.handle_input(pi_js_compat::js_json::JsString::from("\x05"));
    for character in ["d", "e"] {
        input.handle_input(pi_js_compat::js_json::JsString::from(character));
    }
    assert_eq!(input.get_value(), "abcde");

    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "abc");
    input.handle_input(pi_js_compat::js_json::JsString::from("\x1b[45;5u"));
    assert_eq!(input.get_value(), "");
}
