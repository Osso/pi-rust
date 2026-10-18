use pi_coding_agent::core::export_html::ansi_to_html::{ansi_lines_to_html, ansi_to_html};

#[test]
fn lines_are_wrapped_without_inserting_whitespace() {
    assert_eq!(
        ansi_lines_to_html(&["one".to_owned(), "two".to_owned()]),
        "<div class=\"ansi-line\">one</div><div class=\"ansi-line\">two</div>"
    );
}

#[test]
fn empty_lines_render_a_nbsp() {
    assert_eq!(
        ansi_lines_to_html(&["".to_owned()]),
        "<div class=\"ansi-line\">&nbsp;</div>"
    );
}

#[test]
fn standard_colors_and_resets_become_inline_styles() {
    assert_eq!(
        ansi_to_html("\u{1b}[31mone\u{1b}[0m"),
        "<span style=\"color:#800000\">one</span>"
    );
    assert_eq!(
        ansi_to_html("\u{1b}[1;3;4;2mbold\u{1b}[22mundim\u{1b}[23mnoitalic\u{1b}[24m"),
        "<span style=\"font-weight:bold;opacity:0.6;font-style:italic;text-decoration:underline\">bold</span>\
         <span style=\"font-style:italic;text-decoration:underline\">undim</span>\
         <span style=\"text-decoration:underline\">noitalic</span>"
    );
}

#[test]
fn bright_and_background_colors_use_the_palette() {
    assert_eq!(
        ansi_to_html("\u{1b}[97;41mX\u{1b}[39;49mY"),
        "<span style=\"color:#ffffff;background-color:#800000\">X</span>Y"
    );
}

#[test]
fn extended_colors_support_256_palette_and_rgb() {
    assert_eq!(
        ansi_to_html("\u{1b}[38;5;196ma\u{1b}[48;2;1;2;3mb"),
        "<span style=\"color:#ff0000\">a</span><span style=\"color:#ff0000;background-color:rgb(1,2,3)\">b</span>"
    );
    assert_eq!(
        ansi_to_html("\u{1b}[38;5;16mc"),
        "<span style=\"color:#000000\">c</span>"
    );
    assert_eq!(
        ansi_to_html("\u{1b}[38;5;232md"),
        "<span style=\"color:#080808\">d</span>"
    );
    assert_eq!(
        ansi_to_html("\u{1b}[38;5;255me"),
        "<span style=\"color:#eeeeee\">e</span>"
    );
}

#[test]
fn text_is_html_escaped() {
    assert_eq!(
        ansi_to_html("<a href=\"x\">&'</a>"),
        "&lt;a href=&quot;x&quot;&gt;&amp;&#039;&lt;/a&gt;"
    );
}

#[test]
fn empty_params_reset_and_unknown_codes_are_ignored() {
    assert_eq!(
        ansi_to_html("\u{1b}[31ma\u{1b}[mb"),
        "<span style=\"color:#800000\">a</span>b"
    );
    assert_eq!(ansi_to_html("\u{1b}[99;5mplain"), "plain");
    assert_eq!(ansi_to_html("\u{1b}[38;5ma"), "a");
    assert_eq!(
        ansi_to_html("\u{1b}[38;2;1;2ma"),
        "<span style=\"font-weight:bold;opacity:0.6\">a</span>"
    );
}

#[test]
fn styles_accumulate_and_close_at_the_next_sequence() {
    assert_eq!(
        ansi_to_html("\u{1b}[1ma\u{1b}[31mb"),
        "<span style=\"font-weight:bold\">a</span><span style=\"color:#800000;font-weight:bold\">b</span>"
    );
}
