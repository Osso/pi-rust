//! Minimal port of the pinned `chalk` 5.6.2 behavior used by Pi's CLI output.
//!
//! Faithful pieces: exact `ansi-styles` SGR codes, chained styles, chalk's
//! re-open of embedded close codes, per-line close/re-open around `\n`, empty
//! input, and `supports-color` level detection for stdout/stderr.
//!
//! Deliberate exclusions (invalid JavaScript runtime input types): multi-argument
//! calls, `rgb`/`hex`/`ansi256` color models, background colors, `visible`, and
//! chalk levels outside `0..=3`.

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::sync::OnceLock;

use pi_js_compat::js_json::JsString;
use regex::Regex;

/// Output stream whose TTY status selects the detected color level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// `supports-color` level; `Plain` disables styling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Plain,
    Basic,
    Ansi256,
    Ansi16m,
}

impl Level {
    pub const fn number(self) -> u8 {
        match self {
            Level::Plain => 0,
            Level::Basic => 1,
            Level::Ansi256 => 2,
            Level::Ansi16m => 3,
        }
    }

    /// Typed replacement for chalk's runtime level validation (`0..=3`).
    pub const fn from_number(number: u8) -> Option<Self> {
        match number {
            0 => Some(Level::Plain),
            1 => Some(Level::Basic),
            2 => Some(Level::Ansi256),
            3 => Some(Level::Ansi16m),
            _ => None,
        }
    }
}

/// Style names from `ansi-styles` that Pi uses, plus the rest of its
/// modifier and foreground groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Reset,
    Bold,
    Dim,
    Italic,
    Underline,
    Overline,
    Inverse,
    Hidden,
    Strikethrough,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BlackBright,
    Gray,
    RedBright,
    GreenBright,
    YellowBright,
    BlueBright,
    MagentaBright,
    CyanBright,
}

impl Style {
    pub const fn open(self) -> &'static str {
        match self {
            Style::Reset => "\x1b[0m",
            Style::Bold => "\x1b[1m",
            Style::Dim => "\x1b[2m",
            Style::Italic => "\x1b[3m",
            Style::Underline => "\x1b[4m",
            Style::Overline => "\x1b[53m",
            Style::Inverse => "\x1b[7m",
            Style::Hidden => "\x1b[8m",
            Style::Strikethrough => "\x1b[9m",
            Style::Black => "\x1b[30m",
            Style::Red => "\x1b[31m",
            Style::Green => "\x1b[32m",
            Style::Yellow => "\x1b[33m",
            Style::Blue => "\x1b[34m",
            Style::Magenta => "\x1b[35m",
            Style::Cyan => "\x1b[36m",
            Style::White => "\x1b[37m",
            Style::BlackBright | Style::Gray => "\x1b[90m",
            Style::RedBright => "\x1b[91m",
            Style::GreenBright => "\x1b[92m",
            Style::YellowBright => "\x1b[93m",
            Style::BlueBright => "\x1b[94m",
            Style::MagentaBright => "\x1b[95m",
            Style::CyanBright => "\x1b[96m",
        }
    }

    pub const fn close(self) -> &'static str {
        match self {
            Style::Reset => "\x1b[0m",
            Style::Bold | Style::Dim => "\x1b[22m",
            Style::Italic => "\x1b[23m",
            Style::Underline => "\x1b[24m",
            Style::Overline => "\x1b[55m",
            Style::Inverse => "\x1b[27m",
            Style::Hidden => "\x1b[28m",
            Style::Strikethrough => "\x1b[29m",
            Style::Black
            | Style::Red
            | Style::Green
            | Style::Yellow
            | Style::Blue
            | Style::Magenta
            | Style::Cyan
            | Style::White
            | Style::BlackBright
            | Style::Gray
            | Style::RedBright
            | Style::GreenBright
            | Style::YellowBright
            | Style::BlueBright
            | Style::MagentaBright
            | Style::CyanBright => "\x1b[39m",
        }
    }
}

/// A chalk instance bound to one color level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chalk {
    level: Level,
}

impl Chalk {
    pub const fn new(level: Level) -> Self {
        Self { level }
    }

    /// Chalk bound to the detected stdout color level.
    pub fn stdout() -> Self {
        Self::new(detect_level(Stream::Stdout))
    }

    /// Chalk bound to the detected stderr color level (`chalkStderr`).
    pub fn stderr() -> Self {
        Self::new(detect_level(Stream::Stderr))
    }

    pub const fn level(self) -> Level {
        self.level
    }

    /// Apply a style chain (outermost first) to `text`.
    pub fn paint(&self, styles: &[Style], text: impl Into<JsString>) -> JsString {
        let text = text.into();
        if self.level == Level::Plain || text.is_empty() {
            return text;
        }

        let open = styles.iter().map(|style| style.open()).collect::<String>();
        let close = styles.iter().rev().map(|style| style.close()).collect::<String>();
        let mut body = reopen_embedded_closes(&text, styles);
        if body.contains('\n') {
            body = encase_lines(&body, &close, &open);
        }
        JsString::from(open) + body + close
    }

    pub fn red(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Red], text)
    }

    pub fn dim(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Dim], text)
    }

    pub fn yellow(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Yellow], text)
    }

    pub fn bold(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Bold], text)
    }

    pub fn green(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Green], text)
    }

    pub fn cyan(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Cyan], text)
    }

    pub fn italic(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Italic], text)
    }

    pub fn strikethrough(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Strikethrough], text)
    }

    pub fn blue(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Blue], text)
    }

    pub fn underline(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Underline], text)
    }

    pub fn gray(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Gray], text)
    }

    pub fn magenta(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Magenta], text)
    }

    pub fn inverse(&self, text: impl Into<JsString>) -> JsString {
        self.paint(&[Style::Inverse], text)
    }
}

/// Detect the color level of the process's stdout or stderr.
pub fn detect_level(stream: Stream) -> Level {
    let env = process_environment();
    let argv = std::env::args().collect::<Vec<_>>();
    let is_tty = match stream {
        Stream::Stdout => std::io::stdout().is_terminal(),
        Stream::Stderr => std::io::stderr().is_terminal(),
    };
    detect_level_for(&env, &argv, is_tty)
}

/// Level detection over injected environment, argv, and TTY state.
pub fn detect_level_for(env: &BTreeMap<String, String>, argv: &[String], is_tty: bool) -> Level {
    let force =
        env_force_color(env).or_else(|| argv_force_color(argv).map(|level| ForceColor::Level(i64::from(level))));

    if matches!(force, Some(ForceColor::Level(0))) {
        return Level::Plain;
    }

    if let Some(level) = argv_level_override(argv) {
        return level;
    }

    // Azure DevOps pipelines are checked before the TTY test.
    if env.contains_key("TF_BUILD") && env.contains_key("AGENT_NAME") {
        return Level::Basic;
    }

    if !is_tty && force.is_none() {
        return Level::Plain;
    }

    let min = match force {
        Some(ForceColor::Level(level)) => level,
        Some(ForceColor::Invalid) | None => 0,
    };

    if env.get("TERM").is_some_and(|term| term == "dumb") {
        return level_from_min(min);
    }

    if env.contains_key("CI") {
        return ci_level(env, min);
    }

    if let Some(version) = env.get("TEAMCITY_VERSION") {
        return teamcity_level(version);
    }

    if let Some(level) = terminal_level(env) {
        return level;
    }

    if env.contains_key("COLORTERM") {
        return Level::Basic;
    }

    level_from_min(min)
}

/// `--color=16m`, `--color=full`, `--color=truecolor`, `--color=256`.
fn argv_level_override(argv: &[String]) -> Option<Level> {
    if ["color=16m", "color=full", "color=truecolor"]
        .iter()
        .any(|flag| has_flag(argv, flag))
    {
        return Some(Level::Ansi16m);
    }

    has_flag(argv, "color=256").then_some(Level::Ansi256)
}

fn ci_level(env: &BTreeMap<String, String>, min: i64) -> Level {
    if ["GITHUB_ACTIONS", "GITEA_ACTIONS", "CIRCLECI"]
        .iter()
        .any(|key| env.contains_key(*key))
    {
        return Level::Ansi16m;
    }

    let basic_ci = ["TRAVIS", "APPVEYOR", "GITLAB_CI", "BUILDKITE", "DRONE"]
        .iter()
        .any(|key| env.contains_key(*key));
    let codeship = env.get("CI_NAME").is_some_and(|name| name == "codeship");
    if basic_ci || codeship {
        return Level::Basic;
    }

    level_from_min(min)
}

fn teamcity_level(version: &str) -> Level {
    if teamcity_version_regex().is_match(version) {
        Level::Basic
    } else {
        Level::Plain
    }
}

/// Terminal capabilities that force a level regardless of the forced minimum.
fn terminal_level(env: &BTreeMap<String, String>) -> Option<Level> {
    if env.get("COLORTERM").is_some_and(|value| value == "truecolor") {
        return Some(Level::Ansi16m);
    }

    let term = env.get("TERM").map(String::as_str).unwrap_or("");
    if matches!(term, "xterm-kitty" | "xterm-ghostty" | "wezterm") {
        return Some(Level::Ansi16m);
    }

    if let Some(level) = terminal_program_level(env) {
        return Some(level);
    }

    term_name_level(term)
}

fn terminal_program_level(env: &BTreeMap<String, String>) -> Option<Level> {
    match env.get("TERM_PROGRAM")?.as_str() {
        "iTerm.app" => Some(iterm_level(env)),
        "Apple_Terminal" => Some(Level::Ansi256),
        _ => None,
    }
}

fn iterm_level(env: &BTreeMap<String, String>) -> Level {
    let major = env
        .get("TERM_PROGRAM_VERSION")
        .and_then(|version| version.split('.').next())
        .map(|version| pi_js_compat::number::parse_int(&version.encode_utf16().collect::<Vec<_>>(), 10))
        .unwrap_or(0.0);

    if major >= 3.0 { Level::Ansi16m } else { Level::Ansi256 }
}

fn term_name_level(term: &str) -> Option<Level> {
    if term_256_regex().is_match(term) {
        return Some(Level::Ansi256);
    }

    term_basic_regex().is_match(term).then_some(Level::Basic)
}

fn process_environment() -> BTreeMap<String, String> {
    pi_js_compat::process_env::vars().into_iter().collect()
}

fn level_from_min(min: i64) -> Level {
    match min {
        i64::MIN..=0 => Level::Plain,
        1 => Level::Basic,
        2 => Level::Ansi256,
        _ => Level::Ansi16m,
    }
}

/// Highest chalk color level (`ansi16m`).
const MAX_COLOR_LEVEL: i64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ForceColor {
    /// Parsed `FORCE_COLOR`/argv value, already clamped to at most 3.
    Level(i64),
    /// `FORCE_COLOR` present but not parseable as an integer (`NaN`).
    Invalid,
}

fn argv_force_color(argv: &[String]) -> Option<u8> {
    const NEVER: [&str; 4] = ["no-color", "no-colors", "color=false", "color=never"];
    const ALWAYS: [&str; 4] = ["color", "colors", "color=true", "color=always"];

    if NEVER.iter().any(|flag| has_flag(argv, flag)) {
        Some(0)
    } else if ALWAYS.iter().any(|flag| has_flag(argv, flag)) {
        Some(1)
    } else {
        None
    }
}

fn env_force_color(env: &BTreeMap<String, String>) -> Option<ForceColor> {
    let value = env.get("FORCE_COLOR")?;
    match value.as_str() {
        "true" => Some(ForceColor::Level(1)),
        "false" => Some(ForceColor::Level(0)),
        "" => Some(ForceColor::Level(1)),
        _ => {
            let parsed = pi_js_compat::number::parse_int(&value.encode_utf16().collect::<Vec<_>>(), 10);
            Some(if parsed.is_nan() {
                ForceColor::Invalid
            } else {
                ForceColor::Level((parsed as i64).min(MAX_COLOR_LEVEL))
            })
        }
    }
}

fn has_flag(argv: &[String], flag: &str) -> bool {
    let prefix = if flag.starts_with('-') {
        ""
    } else if flag.len() == 1 {
        "-"
    } else {
        "--"
    };
    let position = argv.iter().position(|argument| argument == &format!("{prefix}{flag}"));
    let Some(position) = position else {
        return false;
    };
    match argv.iter().position(|argument| argument == "--") {
        Some(terminator) => position < terminator,
        None => true,
    }
}

fn reopen_embedded_closes(text: &JsString, styles: &[Style]) -> JsString {
    if !text.contains('\u{1b}') {
        return text.to_owned();
    }
    styles.iter().rev().fold(text.to_owned(), |current, style| {
        current.replace(style.close(), format!("{}{}", style.close(), style.open()))
    })
}

fn encase_lines(text: &JsString, prefix: &str, postfix: &str) -> JsString {
    let mut result = JsString::default();
    let mut line_start = 0;
    while let Some(index) = text.find_from('\n', line_start) {
        let has_carriage_return = index > 0 && text.units()[index - 1] == u16::from(b'\r');
        let line_end = if has_carriage_return { index - 1 } else { index };
        result.push_str(text.slice(line_start, line_end));
        result.push_str(prefix);
        result.push_str(if has_carriage_return { "\r\n" } else { "\n" });
        result.push_str(postfix);
        line_start = index + 1;
    }
    result.push_str(text.slice(line_start, text.len()));
    result
}

fn teamcity_version_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"^(9\.(0*[1-9]\d*)\.|\d{2,}\.)").expect("the pinned TEAMCITY_VERSION pattern must compile")
    })
}

fn term_256_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)-256(color)?$").expect("the pinned 256-color TERM pattern must compile"))
}

fn term_basic_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)^screen|^xterm|^vt100|^vt220|^rxvt|color|ansi|cygwin|linux")
            .expect("the pinned basic-color TERM pattern must compile")
    })
}
