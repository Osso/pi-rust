use pi_tui::terminal_colors::{
    RgbColor, TerminalColorScheme, is_osc11_background_color_response, parse_osc11_background_color,
    parse_terminal_color_scheme_report,
};

#[test]
fn parses_16_bit_osc11_rgb_responses() {
    assert_eq!(
        parse_osc11_background_color("\x1b]11;rgb:0000/8000/ffff\x07"),
        Some(RgbColor {
            r: 0.0,
            g: 128.0,
            b: 255.0
        })
    );
}

#[test]
fn parses_osc11_hex_responses() {
    assert_eq!(
        parse_osc11_background_color("\x1b]11;#ffffff\x1b\\"),
        Some(RgbColor {
            r: 255.0,
            g: 255.0,
            b: 255.0
        })
    );
    assert_eq!(
        parse_osc11_background_color("\x1b]11;#000000\x07"),
        Some(RgbColor { r: 0.0, g: 0.0, b: 0.0 })
    );
}

#[test]
fn rejects_non_strict_osc11_responses() {
    assert_eq!(parse_osc11_background_color("x\x1b]11;#ffffff\x07"), None);
    assert_eq!(parse_osc11_background_color("\x1b]10;#ffffff\x07"), None);
    assert_eq!(parse_osc11_background_color("\x1b]11;#ffffff\x07x"), None);
}

#[test]
fn recognizes_the_osc11_response_envelope() {
    assert!(is_osc11_background_color_response("\x1b]11;#ffffff\x07"));
    assert!(is_osc11_background_color_response("\x1b]11;not-a-color\x1b\\"));
    assert!(!is_osc11_background_color_response("x\x1b]11;#ffffff\x07"));
    assert!(!is_osc11_background_color_response("\x1b]10;#ffffff\x07"));
    assert!(!is_osc11_background_color_response("\x1b]11;#ffffff\x07x"));
}

#[test]
fn parses_12_digit_hex_channels() {
    assert_eq!(
        parse_osc11_background_color("\x1b]11;#FFFFFFFFFFFF\x07"),
        Some(RgbColor {
            r: 255.0,
            g: 255.0,
            b: 255.0
        })
    );
    assert_eq!(
        parse_osc11_background_color("\x1b]11;#000000000000\x07"),
        Some(RgbColor { r: 0.0, g: 0.0, b: 0.0 })
    );
}

#[test]
fn rejects_unsupported_hex_lengths() {
    assert!(is_osc11_background_color_response("\x1b]11;#fff\x07"));
    assert_eq!(parse_osc11_background_color("\x1b]11;#fff\x07"), None);
    assert_eq!(parse_osc11_background_color("\x1b]11;#gggggg\x07"), None);
}

#[test]
fn parses_short_and_long_rgb_channels() {
    assert_eq!(
        parse_osc11_background_color("\x1b]11;rgb:ff/80/00\x07"),
        Some(RgbColor {
            r: 255.0,
            g: 128.0,
            b: 0.0
        })
    );
    assert_eq!(
        parse_osc11_background_color("\x1b]11;rgb:0/8/f\x07"),
        Some(RgbColor {
            r: 0.0,
            g: 136.0,
            b: 255.0
        })
    );
    assert_eq!(
        parse_osc11_background_color("\x1b]11;rgb:000/800/fff\x07"),
        Some(RgbColor {
            r: 0.0,
            g: 128.0,
            b: 255.0
        })
    );
}

#[test]
fn accepts_rgba_prefix_and_ignores_extra_channels() {
    assert_eq!(
        parse_osc11_background_color("\x1b]11;rgba:ff/80/00\x07"),
        Some(RgbColor {
            r: 255.0,
            g: 128.0,
            b: 0.0
        })
    );
    assert_eq!(
        parse_osc11_background_color("\x1b]11;rgb:ff/80/00/40\x07"),
        Some(RgbColor {
            r: 255.0,
            g: 128.0,
            b: 0.0
        })
    );
}

#[test]
fn rejects_incomplete_or_invalid_rgb_channels() {
    assert_eq!(parse_osc11_background_color("\x1b]11;rgb:ff/80\x07"), None);
    assert_eq!(parse_osc11_background_color("\x1b]11;rgb:/80/00\x07"), None);
    assert_eq!(parse_osc11_background_color("\x1b]11;rgb:gg/80/00\x07"), None);
}

#[test]
fn trims_surrounding_whitespace_in_osc11_values() {
    assert_eq!(
        parse_osc11_background_color("\x1b]11;  #ffffff  \x07"),
        Some(RgbColor {
            r: 255.0,
            g: 255.0,
            b: 255.0
        })
    );
}

#[test]
fn parses_color_scheme_reports() {
    assert_eq!(
        parse_terminal_color_scheme_report("\x1b[?997;1n"),
        Some(TerminalColorScheme::Dark)
    );
    assert_eq!(
        parse_terminal_color_scheme_report("\x1b[?997;2n"),
        Some(TerminalColorScheme::Light)
    );
    assert_eq!(
        parse_terminal_color_scheme_report("\x1b[?997;2n\x1b[?997;1n\x1b[?997;1n"),
        None
    );
    assert_eq!(
        parse_terminal_color_scheme_report("\x1b[?997;1n\x1b[?997;2n\x1b[?997;2n"),
        None
    );
    assert_eq!(parse_terminal_color_scheme_report("\x1b[?997;1n\x1b[?997;2n"), None);
}

#[test]
fn rejects_unsupported_color_scheme_reports() {
    assert_eq!(parse_terminal_color_scheme_report("\x1b[?997;3n"), None);
    assert_eq!(parse_terminal_color_scheme_report("\x1b[?996n"), None);
    assert_eq!(parse_terminal_color_scheme_report("x\x1b[?997;1n"), None);
    assert_eq!(parse_terminal_color_scheme_report(""), None);
}
