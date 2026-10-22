use pi_js_compat::js_json::JsString;
use pi_tui::utils::{
    AnsiCode, GraphemeCellRange, Segments, apply_background_to_line, extract_ansi_code, extract_segments,
    get_grapheme_cell_range, get_osc8_link_at_column, is_punctuation_char, is_whitespace_char,
    normalize_terminal_output, slice_by_column, slice_with_width, strip_terminal_sequences, truncate_to_width,
    truncate_to_width_with, visible_width, wrap_text_with_ansi,
};

#[test]
fn empty_string_has_zero_width() {
    assert_eq!(visible_width(""), 0);
}

#[test]
fn printable_ascii_width_is_byte_length() {
    assert_eq!(visible_width("hello world"), 11);
    assert_eq!(visible_width("!\"#$%&'()*+,-./"), 15);
}

#[test]
fn tabs_count_as_three_cells() {
    assert_eq!(visible_width("\t"), 3);
    assert_eq!(visible_width("a\tb"), 5);
    assert_eq!(visible_width("\t\x1b[31m界\x1b[0m"), 5);
}

#[test]
fn ansi_sgr_sequences_do_not_contribute_width() {
    assert_eq!(visible_width("\x1b[31mred\x1b[0m"), 3);
    assert_eq!(visible_width("\x1b[1;38;5;240mtext\x1b[0m"), 4);
}

#[test]
fn osc_and_apc_sequences_do_not_contribute_width() {
    assert_eq!(visible_width("\x1b]133;A\x07hello\x1b]133;B\x07"), 5);
    assert_eq!(visible_width("\x1b]133;A\x1b\\hello\x1b]133;B\x1b\\"), 5);
    assert_eq!(visible_width("\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\"), 4);
    assert_eq!(visible_width("\x1b_payload\x07text\x1b_"), 5);
}

#[test]
fn cjk_and_japanese_widths() {
    assert_eq!(visible_width("网络"), 4);
    assert_eq!(visible_width("ネットワーク"), 12);
    assert_eq!(visible_width("が"), 2);
    assert_eq!(visible_width("か\u{3099}"), 2);
}

#[test]
fn ordinary_combining_marks_are_zero_width() {
    assert_eq!(visible_width("e\u{0301}"), 1);
    assert_eq!(visible_width("čřžůú"), 5);
    assert_eq!(visible_width("שָׁ"), 1);
    assert_eq!(visible_width("بّ"), 1);
    assert_eq!(visible_width("རྐ"), 1);
    assert_eq!(visible_width("ᜠ᜴"), 1);
    assert_eq!(visible_width("가〮"), 2);
    assert_eq!(visible_width("가〯"), 2);
}

#[test]
fn indic_conjuncts_use_the_frozen_base_width() {
    assert_eq!(visible_width("र्क"), 1);
    assert_eq!(visible_width("नेटवर्क"), 4);
    assert_eq!(visible_width("सर्वाधिकार सुरक्षित। ऑर्डर पर क्लिक करें"), 24);
    assert_eq!(visible_width("র্ক"), 1);
    assert_eq!(visible_width("ર્ક"), 1);
    assert_eq!(visible_width("ର୍କ"), 1);
    assert_eq!(visible_width("ర్క"), 1);
    assert_eq!(visible_width("ര്‍ക"), 1);
}

#[test]
fn myanmar_marks_use_the_frozen_base_width() {
    assert_eq!(visible_width("ကာ"), 1);
    assert_eq!(visible_width("ကေ"), 1);
    assert_eq!(visible_width("က်"), 1);
    assert_eq!(visible_width("ကျ"), 1);
    assert_eq!(visible_width("ကြ"), 1);
    assert_eq!(visible_width("ကဳ"), 1);
    assert_eq!(visible_width("ကဴ"), 1);
    assert_eq!(visible_width("ကဵ"), 1);
    assert_eq!(visible_width("ကး"), 1);
    assert_eq!(visible_width("ကို"), 1);
    assert_eq!(visible_width("က္"), 1);
}

#[test]
fn thai_and_lao_am_clusters_keep_their_cell_width() {
    assert_eq!(visible_width("ำ"), 1);
    assert_eq!(visible_width("ຳ"), 1);
    assert_eq!(visible_width("กำ"), 2);
    assert_eq!(visible_width("ກຳ"), 2);
}

#[test]
fn regional_indicators_are_full_width() {
    for code_point in 0x1f1e6..=0x1f1ff {
        let indicator = char::from_u32(code_point).unwrap();
        assert_eq!(visible_width(&indicator.to_string()), 2);
    }
    assert_eq!(visible_width("🇨"), 2);
    assert_eq!(visible_width("🇨🇳"), 2);
    assert_eq!(visible_width("      - 🇨"), 10);
}

#[test]
fn emoji_samples_are_two_cells() {
    for sample in [
        "👍",
        "👍🏻",
        "✅",
        "⚡",
        "⚡️",
        "👨",
        "👨‍💻",
        "🏳️‍🌈",
        "❤️",
        "©️",
        "1️⃣",
        "☝🏻",
        "❤️‍🔥",
    ] {
        assert_eq!(visible_width(sample), 2, "expected {sample:?} to be width 2");
    }
}

#[test]
fn strip_terminal_sequences_keeps_visible_text() {
    assert_eq!(strip_terminal_sequences("plain"), "plain");
    assert_eq!(strip_terminal_sequences("\x1b[31mred\x1b[0m"), "red");
    assert_eq!(strip_terminal_sequences("\x1b]8;;url\x07link\x1b]8;;\x07"), "link");
    assert_eq!(strip_terminal_sequences("\x1b_payload\x07text\x1b_"), "text\x1b_");
    assert_eq!(strip_terminal_sequences("a\x1bnot-ansi"), "a\x1bnot-ansi");
}

#[test]
fn extract_ansi_code_recognizes_csi_sequences() {
    assert_eq!(
        extract_ansi_code("\x1b[31mred", 0),
        Some(AnsiCode {
            code: "\x1b[31m".into(),
            length: 5
        })
    );
    assert_eq!(extract_ansi_code("\x1b[2K", 0).unwrap().code, "\x1b[2K");
    assert_eq!(extract_ansi_code("\x1b[1G", 0).unwrap().code, "\x1b[1G");
    assert_eq!(extract_ansi_code("\x1b[2H", 0).unwrap().code, "\x1b[2H");
    assert_eq!(extract_ansi_code("\x1b[0J", 0).unwrap().code, "\x1b[0J");
    assert_eq!(extract_ansi_code("ab\x1b[31m", 2).unwrap().code, "\x1b[31m");
}

#[test]
fn extract_ansi_code_rejects_incomplete_sequences() {
    assert_eq!(extract_ansi_code("\x1b[31", 0), None);
    assert_eq!(extract_ansi_code("\x1b", 0), None);
    assert_eq!(extract_ansi_code("\x1bX", 0), None);
    assert_eq!(extract_ansi_code("x", 0), None);
    assert_eq!(extract_ansi_code("", 0), None);
}

#[test]
fn extract_ansi_code_recognizes_osc_and_apc_with_both_terminators() {
    assert_eq!(extract_ansi_code("\x1b]8;;url\x07", 0).unwrap().code, "\x1b]8;;url\x07");
    assert_eq!(
        extract_ansi_code("\x1b]8;;url\x1b\\", 0).unwrap().code,
        "\x1b]8;;url\x1b\\"
    );
    assert_eq!(extract_ansi_code("\x1b_data\x07", 0).unwrap().code, "\x1b_data\x07");
    assert_eq!(extract_ansi_code("\x1b_data\x1b\\", 0).unwrap().code, "\x1b_data\x1b\\");
    assert_eq!(extract_ansi_code("\x1b]unterminated", 0), None);
    assert_eq!(extract_ansi_code("\x1b_unterminated", 0), None);
}

#[test]
fn normalize_terminal_output_replaces_thai_and_lao_am_vowels() {
    assert_eq!(normalize_terminal_output("ำ"), "ํา");
    assert_eq!(normalize_terminal_output("ຳ"), "ໍາ");
    assert_eq!(visible_width(&normalize_terminal_output("ำabc")), visible_width("ำabc"));
    assert_eq!(visible_width(&normalize_terminal_output("ຳabc")), visible_width("ຳabc"));
}

#[test]
fn normalize_terminal_output_preserves_tabs() {
    assert_eq!(normalize_terminal_output("a\tb"), "a\tb");
    for control in [
        "\x1b]8;;https://example.test/a\tb\x07",
        "\x1b]0;window\ttitle\x1b\\",
        "\x1b_payload\tdata\x1b\\",
    ] {
        assert_eq!(
            normalize_terminal_output(&format!("{control}label\ttext")),
            format!("{control}label\ttext")
        );
    }
}

#[test]
fn grapheme_cell_range_reports_the_covering_grapheme() {
    assert_eq!(
        get_grapheme_cell_range("abc", 1),
        Some(GraphemeCellRange { start: 1, end: 2 })
    );
    assert_eq!(
        get_grapheme_cell_range("a界b", 1),
        Some(GraphemeCellRange { start: 1, end: 3 })
    );
    assert_eq!(
        get_grapheme_cell_range("a界b", 2),
        Some(GraphemeCellRange { start: 1, end: 3 })
    );
    assert_eq!(
        get_grapheme_cell_range("a界b", 3),
        Some(GraphemeCellRange { start: 3, end: 4 })
    );
    assert_eq!(get_grapheme_cell_range("abc", 3), None);
    assert_eq!(get_grapheme_cell_range("", 0), None);
    assert_eq!(
        get_grapheme_cell_range("\x1b[31m界\x1b[0m", 0),
        Some(GraphemeCellRange { start: 0, end: 2 })
    );
}

#[test]
fn osc8_link_at_column_tracks_open_and_close() {
    let line = "\x1b]8;;https://a.test\x07link\x1b]8;;\x07 after";
    assert_eq!(get_osc8_link_at_column(line, 0), Some(JsString::from("https://a.test")));
    assert_eq!(get_osc8_link_at_column(line, 3), Some(JsString::from("https://a.test")));
    assert_eq!(get_osc8_link_at_column(line, 4), None);
    assert_eq!(get_osc8_link_at_column(line, 6), None);
    assert_eq!(get_osc8_link_at_column("plain", 0), None);
}

#[test]
fn slice_with_width_extracts_columns_and_widths() {
    let text = "out 192M\t.pi/skill-tests/results-ha";
    let slice = slice_with_width(text, 0, 10, true);
    assert_eq!(slice.text, "out 192M");
    assert_eq!(slice.width, 8);
    assert_eq!(visible_width(&slice.text), slice.width);

    assert_eq!(slice_by_column("abcdef", 1, 3, false), "bcd");
    assert_eq!(slice_with_width("abcdef", 1, 3, false).width, 3);
    assert_eq!(slice_with_width("abcdef", 0, 0, false).text, "");
    assert_eq!(slice_by_column("a界b", 1, 2, false), "界");
    assert_eq!(slice_by_column("a界b", 1, 2, true), "界");
    assert_eq!(slice_by_column("a界b", 1, 1, true), "");
    assert_eq!(slice_by_column("a界b", 0, 3, true), "a界");
}

#[test]
fn slice_with_width_preserves_styling_for_selected_columns() {
    assert_eq!(slice_by_column("\x1b[31mabc\x1b[0m", 0, 2, false), "\x1b[31mab");
    assert_eq!(slice_by_column("\x1b[31mabc\x1b[0m", 1, 2, false), "\x1b[31mbc");
    assert_eq!(slice_by_column("a\x1b[31mbc\x1b[0m", 1, 2, false), "\x1b[31mbc");
}

#[test]
fn extract_segments_returns_before_and_after() {
    let text = "out 192M\t.pi/skill-tests/results-ha";
    let segments = extract_segments(text, 10, 13, 10, true);
    assert_eq!(segments.before, "out 192M");
    assert_eq!(segments.before_width, 8);
    assert_eq!(visible_width(&segments.before), segments.before_width);

    let tab_fits = extract_segments(text, 11, 13, 10, true);
    assert_eq!(tab_fits.before, "out 192M\t");
    assert_eq!(tab_fits.before_width, 11);
    assert_eq!(visible_width(&tab_fits.before), tab_fits.before_width);
}

#[test]
fn extract_segments_inherits_styling_from_before_the_overlay() {
    let segments = extract_segments("\x1b[31mabcdef\x1b[0m", 2, 4, 2, false);
    assert_eq!(segments.before, "\x1b[31mab");
    assert_eq!(segments.before_width, 2);
    assert_eq!(segments.after, "\x1b[31mef");
    assert_eq!(segments.after_width, 2);
}

#[test]
fn apply_background_pads_to_full_width() {
    assert_eq!(
        apply_background_to_line("abc", 5, |text| JsString::from("[") + text + "]"),
        "[abc  ]"
    );
    assert_eq!(
        apply_background_to_line("abcde", 3, |text| JsString::from("[") + text + "]"),
        "[abcde]"
    );
    assert_eq!(
        apply_background_to_line("\x1b[31mab\x1b[0m", 4, |text| JsString::from("[") + text + "]"),
        "[\x1b[31mab\x1b[0m  ]"
    );
}

#[test]
fn whitespace_and_punctuation_helpers_match_javascript_regexes() {
    assert!(is_whitespace_char(" "));
    assert!(is_whitespace_char("\t"));
    assert!(is_whitespace_char("\u{00a0}"));
    assert!(is_whitespace_char("a b"));
    assert!(!is_whitespace_char(""));
    assert!(!is_whitespace_char("a"));
    assert!(!is_whitespace_char("\u{0085}"));

    for punctuation in "(){}[]<>.,;:'\"!?+-=*/\\|&%^$#@~`".chars() {
        assert!(is_punctuation_char(&punctuation.to_string()), "{punctuation:?}");
    }
    assert!(!is_punctuation_char("a"));
    assert!(!is_punctuation_char(" "));
    assert!(is_punctuation_char("a,b"));
}

#[test]
fn wrapping_handles_line_endings() {
    assert_eq!(
        wrap_text_with_ansi("first\nsecond\r\nthird\rfourth", 80),
        vec!["first", "second\r", "third\rfourth"]
    );
}

#[test]
fn wrapping_preserves_ansi_state_across_line_endings() {
    let red = "\x1b[31m";
    let reset = "\x1b[0m";
    assert_eq!(
        wrap_text_with_ansi(&format!("{red}first\r\nsecond\rthird{reset}"), 80),
        vec![format!("{red}first\r"), format!("{red}second\rthird{reset}"),]
    );
}

#[test]
fn wrapping_plain_text_respects_width() {
    let wrapped = wrap_text_with_ansi("hello world this is a test", 10);
    assert!(wrapped.len() > 1);
    for line in &wrapped {
        assert!(visible_width(line) <= 10, "line {line:?} too wide");
    }
}

#[test]
fn wrapping_breaks_cjk_runs_at_grapheme_boundaries() {
    let text = "This is an example 中文汉字测试段落内容中文汉字测试段落内容.";
    let wrapped = wrap_text_with_ansi(text, 40);
    assert_eq!(
        wrapped,
        vec!["This is an example 中文汉字测试段落内容", "中文汉字测试段落内容."]
    );
    for line in &wrapped {
        assert!(visible_width(line) <= 40);
    }
}

#[test]
fn wrapping_preserves_color_codes_when_wrapping_cjk_runs() {
    let red = "\x1b[31m";
    let reset = "\x1b[0m";
    let text = format!("{red}This is an example 中文汉字测试段落内容中文汉字测试段落内容.{reset}");
    let wrapped = wrap_text_with_ansi(&text, 40);
    assert_eq!(wrapped.len(), 2);
    assert_eq!(wrapped[0], format!("{red}This is an example 中文汉字测试段落内容"));
    assert_eq!(wrapped[1], format!("{red}中文汉字测试段落内容.{reset}"));
}

#[test]
fn wrapping_truncates_trailing_whitespace_that_exceeds_width() {
    let wrapped = wrap_text_with_ansi("  ", 1);
    assert!(visible_width(&wrapped[0]) <= 1);
}

#[test]
fn wrapping_keeps_underline_off_the_leading_plain_text() {
    let underline_on = "\x1b[4m";
    let underline_off = "\x1b[24m";
    let url = "https://example.com/very/long/path/that/will/wrap";
    let text = format!("read this thread {underline_on}{url}{underline_off}");
    let wrapped = wrap_text_with_ansi(&text, 40);
    assert_eq!(wrapped[0], "read this thread");
    assert!(wrapped[1].starts_with(underline_on));
    assert!(wrapped[1].contains("https://"));
}

#[test]
fn wrapping_does_not_put_whitespace_before_underline_reset() {
    let underline_on = "\x1b[4m";
    let underline_off = "\x1b[24m";
    let text = format!("{underline_on}underlined text here {underline_off}more");
    let wrapped = wrap_text_with_ansi(&text, 18);
    assert!(!wrapped[0].contains(&format!(" {underline_off}")));
}

#[test]
fn wrapping_closes_underline_without_full_reset_on_middle_lines() {
    let underline_on = "\x1b[4m";
    let underline_off = "\x1b[24m";
    let url = "https://example.com/very/long/path/that/will/definitely/wrap";
    let text = format!("prefix {underline_on}{url}{underline_off} suffix");
    let wrapped = wrap_text_with_ansi(&text, 30);
    for line in &wrapped[1..wrapped.len() - 1] {
        if line.contains(underline_on) {
            assert!(line.ends_with(underline_off));
            assert!(!line.ends_with("\x1b[0m"));
        }
    }
}

#[test]
fn wrapping_preserves_background_across_lines() {
    let bg_blue = "\x1b[44m";
    let reset = "\x1b[0m";
    let text = format!("{bg_blue}hello world this is blue background text{reset}");
    let wrapped = wrap_text_with_ansi(&text, 15);
    for line in &wrapped {
        assert!(line.contains(bg_blue), "line {line:?} lost background");
    }
    for line in &wrapped[..wrapped.len() - 1] {
        assert!(!line.ends_with("\x1b[0m"));
    }
}

#[test]
fn wrapping_resets_underline_but_preserves_background() {
    let underline_on = "\x1b[4m";
    let underline_off = "\x1b[24m";
    let reset = "\x1b[0m";
    let text = format!("\x1b[41mprefix {underline_on}UNDERLINED_CONTENT_THAT_WRAPS{underline_off} suffix{reset}");
    let wrapped = wrap_text_with_ansi(&text, 20);
    for line in &wrapped {
        let has_bg = line.contains("[41m") || line.contains(";41m") || line.contains("[41;");
        assert!(has_bg, "line {line:?} lost background");
    }
    for line in &wrapped[..wrapped.len() - 1] {
        if (line.contains("[4m") || line.contains("[4;") || line.contains(";4m")) && !line.contains(underline_off) {
            assert!(line.ends_with(underline_off));
            assert!(!line.ends_with("\x1b[0m"));
        }
    }
}

#[test]
fn wrapping_reopens_osc8_hyperlinks_and_closes_them_before_breaks() {
    let url = "https://example.com";
    let input = format!("\x1b]8;;{url}\x1b\\0123456789\x1b]8;;\x1b\\");
    let lines = wrap_text_with_ansi(&input, 6);
    for line in &lines {
        let stripped = line
            .replace("\x1b]8;;\x1b\\", "")
            .replace(&format!("\x1b]8;;{url}\x1b\\"), "");
        if !stripped.trim().is_empty() {
            assert!(line.contains(&format!("\x1b]8;;{url}\x1b\\")));
        }
    }
    for line in &lines[..lines.len() - 1] {
        if line.contains(&format!("\x1b]8;;{url}\x1b\\")) {
            assert!(line.ends_with("\x1b]8;;\x1b\\"));
        }
    }
}

#[test]
fn wrapping_preserves_bel_terminators_for_oauth_hyperlinks() {
    let url = format!("https://example.com/oauth/{}", "a".repeat(32));
    let input = format!("\x1b]8;;{url}\x07{url}\x1b]8;;\x07");
    let lines = wrap_text_with_ansi(&input, 20);
    assert!(lines.len() > 1);
    for line in &lines {
        assert!(line.contains(&format!("\x1b]8;;{url}\x07")));
        assert!(!line.contains(&format!("\x1b]8;;{url}\x1b\\")));
    }
    for line in &lines[..lines.len() - 1] {
        assert!(line.ends_with("\x1b]8;;\x07"));
    }
}

#[test]
fn wrapping_does_not_emit_osc8_outside_the_link() {
    let url = "https://example.com";
    let input = format!("before \x1b]8;;{url}\x1b\\link\x1b]8;;\x1b\\ after");
    let lines = wrap_text_with_ansi(&input, 80);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0]
            .units()
            .windows(format!("\x1b]8;;{url}\x1b\\").len())
            .filter(|window| window.iter().copied().eq(format!("\x1b]8;;{url}\x1b\\").encode_utf16()))
            .count(),
        1
    );
    assert_eq!(
        lines[0]
            .units()
            .windows(7)
            .filter(|window| window.iter().copied().eq("\x1b]8;;\x1b\\".encode_utf16()))
            .count(),
        1
    );
}

#[test]
fn truncation_keeps_very_large_unicode_within_width() {
    let text = "🙂界".repeat(100_000);
    let truncated = truncate_to_width_with(&text, 40, "…", false);
    assert!(visible_width(&truncated) <= 40);
    assert!(truncated.ends_with("…\x1b[0m"));
}

#[test]
fn truncation_preserves_styling_and_resets_around_ellipsis() {
    let text = format!("\x1b[31m{}\x1b[0m", "hello ".repeat(1000));
    let truncated = truncate_to_width_with(&text, 20, "…", false);
    assert!(visible_width(&truncated) <= 20);
    assert!(truncated.contains("\x1b[31m"));
    assert!(truncated.ends_with("\x1b[0m…\x1b[0m"));
}

#[test]
fn truncation_retains_frozen_unclosed_osc8_link() {
    let open = "\x1b]8;;https://example.com\x07";
    let close = "\x1b]8;;\x07";
    let text = format!("{open}some-longer-label-here{close}");
    assert_eq!(
        truncate_to_width(&text, 15),
        format!("{open}some-longer-\x1b[0m...\x1b[0m")
    );
}

#[test]
fn truncation_handles_malformed_ansi_prefixes() {
    let text = format!("abc\x1bnot-ansi {}", "🙂".repeat(1000));
    let truncated = truncate_to_width_with(&text, 20, "…", false);
    assert!(visible_width(&truncated) <= 20);
}

#[test]
fn truncation_clips_wide_ellipsis_safely() {
    assert_eq!(truncate_to_width_with("abcdef", 1, "🙂", false), "");
    assert_eq!(truncate_to_width_with("abcdef", 2, "🙂", false), "\x1b[0m🙂\x1b[0m");
    assert!(visible_width(&truncate_to_width_with("abcdef", 2, "🙂", false)) <= 2);
}

#[test]
fn truncation_returns_original_when_it_fits_even_if_ellipsis_is_too_wide() {
    assert_eq!(truncate_to_width_with("a", 2, "🙂", false), "a");
    assert_eq!(truncate_to_width_with("界", 2, "🙂", false), "界");
}

#[test]
fn truncation_pads_to_requested_width() {
    let truncated = truncate_to_width_with("🙂界🙂界🙂界", 8, "…", true);
    assert_eq!(visible_width(&truncated), 8);
}

#[test]
fn truncation_adds_trailing_reset_without_ellipsis() {
    let text = format!("\x1b[31m{}", "hello".repeat(100));
    let truncated = truncate_to_width_with(&text, 10, "", false);
    assert!(visible_width(&truncated) <= 10);
    assert!(truncated.ends_with("\x1b[0m"));
}

#[test]
fn truncation_keeps_a_contiguous_prefix() {
    let truncated = truncate_to_width_with("🙂\t界 \x1b_abc\x07", 7, "…", true);
    assert_eq!(truncated, "🙂\t\x1b[0m…\x1b[0m ");
}

#[test]
fn truncation_ascii_fast_path_pads_and_keeps_text() {
    assert_eq!(truncate_to_width("hello", 10), "hello");
    assert_eq!(truncate_to_width_with("hello", 10, "...", true), "hello     ");
    assert_eq!(
        truncate_to_width_with("hello world", 8, "...", false),
        "hello\x1b[0m...\x1b[0m"
    );
    assert_eq!(
        truncate_to_width_with("hello world", 8, "...", true),
        "hello\x1b[0m...\x1b[0m"
    );
}

#[test]
fn truncation_zero_width_returns_empty() {
    assert_eq!(truncate_to_width("abc", 0), "");
    assert_eq!(truncate_to_width_with("abc", 0, "...", true), "");
}

#[test]
fn truncation_empty_text_pads_when_requested() {
    assert_eq!(truncate_to_width("", 5), "");
    assert_eq!(truncate_to_width_with("", 5, "...", true), "     ");
}

#[test]
fn segments_struct_exposes_before_and_after() {
    let segments = Segments {
        before: "a".into(),
        before_width: 1,
        after: "b".to_string(),
        after_width: 1,
    };
    assert_eq!(segments.before_width + segments.after_width, 2);
}
