use std::collections::BTreeMap;

use pi_coding_agent::utils::chalk::{Chalk, Level, Style, detect_level_for};

const ALL_STYLES: [(Style, &str, &str); 25] = [
    (Style::Reset, "\x1b[0m", "\x1b[0m"),
    (Style::Bold, "\x1b[1m", "\x1b[22m"),
    (Style::Dim, "\x1b[2m", "\x1b[22m"),
    (Style::Italic, "\x1b[3m", "\x1b[23m"),
    (Style::Underline, "\x1b[4m", "\x1b[24m"),
    (Style::Overline, "\x1b[53m", "\x1b[55m"),
    (Style::Inverse, "\x1b[7m", "\x1b[27m"),
    (Style::Hidden, "\x1b[8m", "\x1b[28m"),
    (Style::Strikethrough, "\x1b[9m", "\x1b[29m"),
    (Style::Black, "\x1b[30m", "\x1b[39m"),
    (Style::Red, "\x1b[31m", "\x1b[39m"),
    (Style::Green, "\x1b[32m", "\x1b[39m"),
    (Style::Yellow, "\x1b[33m", "\x1b[39m"),
    (Style::Blue, "\x1b[34m", "\x1b[39m"),
    (Style::Magenta, "\x1b[35m", "\x1b[39m"),
    (Style::Cyan, "\x1b[36m", "\x1b[39m"),
    (Style::White, "\x1b[37m", "\x1b[39m"),
    (Style::BlackBright, "\x1b[90m", "\x1b[39m"),
    (Style::Gray, "\x1b[90m", "\x1b[39m"),
    (Style::RedBright, "\x1b[91m", "\x1b[39m"),
    (Style::GreenBright, "\x1b[92m", "\x1b[39m"),
    (Style::YellowBright, "\x1b[93m", "\x1b[39m"),
    (Style::BlueBright, "\x1b[94m", "\x1b[39m"),
    (Style::MagentaBright, "\x1b[95m", "\x1b[39m"),
    (Style::CyanBright, "\x1b[96m", "\x1b[39m"),
];

const COLOR_LEVELS: [Level; 3] = [Level::Basic, Level::Ansi256, Level::Ansi16m];

#[test]
fn basic_style_codes_match_ansi_styles() {
    for level in COLOR_LEVELS {
        let chalk = Chalk::new(level);
        for (style, open, close) in ALL_STYLES {
            assert_eq!(
                chalk.paint(&[style], "x"),
                format!("{open}x{close}"),
                "{style:?} at {level:?}"
            );
        }
    }
}

#[test]
fn plain_level_emits_no_codes() {
    let chalk = Chalk::new(Level::Plain);
    for (style, _, _) in ALL_STYLES {
        assert_eq!(chalk.paint(&[style], "x"), "x", "{style:?}");
    }
    assert_eq!(chalk.paint(&[Style::Bold, Style::Cyan], "x"), "x");
}

#[test]
fn empty_text_stays_empty() {
    for level in COLOR_LEVELS {
        let chalk = Chalk::new(level);
        assert_eq!(chalk.paint(&[Style::Red], ""), "");
        assert_eq!(chalk.paint(&[Style::Bold, Style::Cyan], ""), "");
    }
}

#[test]
fn chained_styles_nest_open_and_close_codes() {
    let chalk = Chalk::new(Level::Basic);
    assert_eq!(
        chalk.paint(&[Style::Bold, Style::Cyan], "x"),
        "\x1b[1m\x1b[36mx\x1b[39m\x1b[22m"
    );
    assert_eq!(
        chalk.paint(&[Style::Red, Style::Underline, Style::Italic], "x"),
        "\x1b[31m\x1b[4m\x1b[3mx\x1b[23m\x1b[24m\x1b[39m"
    );
}

#[test]
fn embedded_close_code_reopens_the_style() {
    let chalk = Chalk::new(Level::Basic);
    assert_eq!(
        chalk.paint(&[Style::Red], "a\x1b[39mb"),
        "\x1b[31ma\x1b[39m\x1b[31mb\x1b[39m"
    );
    assert_eq!(
        chalk.paint(&[Style::Bold, Style::Cyan], "\x1b[39m\x1b[22m"),
        "\x1b[1m\x1b[36m\x1b[39m\x1b[36m\x1b[22m\x1b[1m\x1b[39m\x1b[22m"
    );
    assert_eq!(chalk.paint(&[Style::Red], "a\x1b[1mb"), "\x1b[31ma\x1b[1mb\x1b[39m");
}

#[test]
fn newlines_close_before_and_reopen_after() {
    let chalk = Chalk::new(Level::Basic);
    assert_eq!(
        chalk.paint(&[Style::Red], "a\nb"),
        "\x1b[31ma\x1b[39m\n\x1b[31mb\x1b[39m"
    );
    assert_eq!(
        chalk.paint(&[Style::Red], "a\r\nb"),
        "\x1b[31ma\x1b[39m\r\n\x1b[31mb\x1b[39m"
    );
    assert_eq!(
        chalk.paint(&[Style::Bold, Style::Cyan], "a\nb\nc"),
        "\x1b[1m\x1b[36ma\x1b[39m\x1b[22m\n\x1b[1m\x1b[36mb\x1b[39m\x1b[22m\n\x1b[1m\x1b[36mc\x1b[39m\x1b[22m"
    );
    assert_eq!(chalk.paint(&[Style::Red], "a\n"), "\x1b[31ma\x1b[39m\n\x1b[31m\x1b[39m");
}

#[test]
fn no_styles_leaves_text_untouched() {
    let chalk = Chalk::new(Level::Basic);
    assert_eq!(chalk.paint(&[], "a\nb"), "a\nb");
}

#[test]
fn named_style_helpers_match_paint() {
    let chalk = Chalk::new(Level::Basic);
    assert_eq!(chalk.yellow("x"), "\x1b[33mx\x1b[39m");
    assert_eq!(chalk.red("x"), "\x1b[31mx\x1b[39m");
    assert_eq!(chalk.dim("x"), "\x1b[2mx\x1b[22m");
    assert_eq!(chalk.bold("x"), "\x1b[1mx\x1b[22m");
    assert_eq!(chalk.green("x"), "\x1b[32mx\x1b[39m");
    assert_eq!(chalk.cyan("x"), "\x1b[36mx\x1b[39m");
    assert_eq!(chalk.italic("x"), "\x1b[3mx\x1b[23m");
    assert_eq!(chalk.strikethrough("x"), "\x1b[9mx\x1b[29m");
    assert_eq!(chalk.blue("x"), "\x1b[34mx\x1b[39m");
    assert_eq!(chalk.underline("x"), "\x1b[4mx\x1b[24m");
    assert_eq!(chalk.gray("x"), "\x1b[90mx\x1b[39m");
    assert_eq!(chalk.magenta("x"), "\x1b[35mx\x1b[39m");
    assert_eq!(chalk.inverse("x"), "\x1b[7mx\x1b[27m");
}

#[test]
fn level_detection_honors_forced_color_and_term() {
    assert_eq!(detect(&[], true), Level::Plain);
    assert_eq!(detect(&[], false), Level::Plain);
    assert_eq!(detect(&[("TERM", "dumb")], true), Level::Plain);
    assert_eq!(detect(&[("TERM", "xterm-256color")], true), Level::Ansi256);
    assert_eq!(detect(&[("FORCE_COLOR", "1")], false), Level::Basic);
    assert_eq!(detect(&[("FORCE_COLOR", "3")], false), Level::Ansi16m);
    assert_eq!(detect(&[("FORCE_COLOR", "0")], true), Level::Plain);
    assert_eq!(detect(&[("NO_COLOR", "1")], true), Level::Plain);
    assert_eq!(detect(&[("COLORTERM", "truecolor")], true), Level::Ansi16m);
}

#[test]
fn level_detection_honors_color_argv_flags() {
    assert_eq!(detect_argv(&["--color"], false), Level::Basic);
    assert_eq!(detect_argv(&["--no-color"], true), Level::Plain);
    assert_eq!(detect_argv(&["--color=256"], true), Level::Ansi256);
    assert_eq!(detect_argv(&["--color=16m"], true), Level::Ansi16m);
    assert_eq!(detect_argv(&["--color=never"], true), Level::Plain);
    assert_eq!(detect_argv(&["--", "--color"], false), Level::Plain);
}

fn detect(pairs: &[(&str, &str)], is_tty: bool) -> Level {
    let env = pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect::<BTreeMap<_, _>>();
    detect_level_for(&env, &["pi".to_owned()], is_tty)
}

fn detect_argv(flags: &[&str], is_tty: bool) -> Level {
    let mut argv = vec!["pi".to_owned()];
    argv.extend(flags.iter().map(|flag| (*flag).to_owned()));
    detect_level_for(&BTreeMap::new(), &argv, is_tty)
}
