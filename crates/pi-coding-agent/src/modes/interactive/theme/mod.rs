//! Ported from `packages/coding-agent/src/modes/interactive/theme/theme.ts`.
//!
//! Theme JSON parsing/validation, ANSI color resolution, the process-global
//! theme instance, terminal background detection, and the TUI theme adapters.
//!
//! Typed Rust API exclusions (August 12, 2026 decision): `ThemeColor`/`ThemeBg`
//! are enums, so the TypeScript `Unknown theme color` throw is unreachable for
//! callers; a missing entry in a hand-built [`Theme`] panics instead.

mod theme_controller;

pub use theme_controller::{InteractiveThemeController, ThemeUi};

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use pi_agent_core::ThinkingLevel;
use pi_js_compat::compare_in_locale;
use pi_js_compat::number::parse_int;
use pi_tui::components::editor::EditorTheme;
use pi_tui::components::markdown::MarkdownTheme;
use pi_tui::components::select_list::SelectListTheme;
use pi_tui::components::settings_list::SettingsListTheme;
use pi_tui::components::text::StyleFn;
use pi_tui::terminal_colors::RgbColor;
use pi_tui::terminal_image::get_capabilities;
use serde_json::{Map, Value};

use crate::config::{embedded_builtin_theme, get_custom_themes_dir, get_themes_dir, package_dir_override};
use crate::core::source_info::SourceInfo;
use crate::utils::chalk::{Chalk, Style};
use crate::utils::fs_watch::{WatchEventKind, close_watcher, watch_with_error_handler};
use crate::utils::syntax_highlight::{
    HighlightFormatter, HighlightOptions, HighlightTheme, highlight, supports_language,
};

// ============================================================================
// Types & Schema
// ============================================================================

/// A theme color token: a hex string, a variable reference, or a 256-color index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColorValue {
    Text(String),
    Index(i64),
}

/// Required color tokens, in schema declaration order.
pub const REQUIRED_COLORS: &[&str] = &[
    "accent",
    "border",
    "borderAccent",
    "borderMuted",
    "success",
    "error",
    "warning",
    "muted",
    "dim",
    "text",
    "thinkingText",
    "selectedBg",
    "userMessageBg",
    "userMessageText",
    "customMessageBg",
    "customMessageText",
    "customMessageLabel",
    "toolPendingBg",
    "toolSuccessBg",
    "toolErrorBg",
    "toolTitle",
    "toolOutput",
    "mdHeading",
    "mdLink",
    "mdLinkUrl",
    "mdCode",
    "mdCodeBlock",
    "mdCodeBlockBorder",
    "mdQuote",
    "mdQuoteBorder",
    "mdHr",
    "mdListBullet",
    "toolDiffAdded",
    "toolDiffRemoved",
    "toolDiffContext",
    "syntaxComment",
    "syntaxKeyword",
    "syntaxFunction",
    "syntaxVariable",
    "syntaxString",
    "syntaxNumber",
    "syntaxType",
    "syntaxOperator",
    "syntaxPunctuation",
    "thinkingOff",
    "thinkingMinimal",
    "thinkingLow",
    "thinkingMedium",
    "thinkingHigh",
    "thinkingXhigh",
    "bashMode",
];

/// Color tokens in schema declaration order (optional tokens included).
const COLOR_KEYS: &[&str] = &[
    "accent",
    "border",
    "borderAccent",
    "borderMuted",
    "success",
    "error",
    "warning",
    "muted",
    "dim",
    "text",
    "thinkingText",
    "selectedBg",
    "scrollbarThumb",
    "searchMatchBg",
    "searchMatchText",
    "userMessageBg",
    "userMessageText",
    "customMessageBg",
    "customMessageText",
    "customMessageLabel",
    "toolPendingBg",
    "toolSuccessBg",
    "toolErrorBg",
    "toolTitle",
    "toolOutput",
    "mdHeading",
    "mdLink",
    "mdLinkUrl",
    "mdCode",
    "mdCodeBlock",
    "mdCodeBlockBorder",
    "mdQuote",
    "mdQuoteBorder",
    "mdHr",
    "mdListBullet",
    "toolDiffAdded",
    "toolDiffRemoved",
    "toolDiffContext",
    "syntaxComment",
    "syntaxKeyword",
    "syntaxFunction",
    "syntaxVariable",
    "syntaxString",
    "syntaxNumber",
    "syntaxType",
    "syntaxOperator",
    "syntaxPunctuation",
    "thinkingOff",
    "thinkingMinimal",
    "thinkingLow",
    "thinkingMedium",
    "thinkingHigh",
    "thinkingXhigh",
    "thinkingMax",
    "bashMode",
];

const EXPORT_KEYS: &[&str] = &["pageBg", "cardBg", "infoBg"];

/// Foreground theme colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeColor {
    Accent,
    Border,
    BorderAccent,
    BorderMuted,
    Success,
    Error,
    Warning,
    Muted,
    Dim,
    Text,
    ThinkingText,
    SearchMatchText,
    UserMessageText,
    CustomMessageText,
    CustomMessageLabel,
    ToolTitle,
    ToolOutput,
    MdHeading,
    MdLink,
    MdLinkUrl,
    MdCode,
    MdCodeBlock,
    MdCodeBlockBorder,
    MdQuote,
    MdQuoteBorder,
    MdHr,
    MdListBullet,
    ToolDiffAdded,
    ToolDiffRemoved,
    ToolDiffContext,
    SyntaxComment,
    SyntaxKeyword,
    SyntaxFunction,
    SyntaxVariable,
    SyntaxString,
    SyntaxNumber,
    SyntaxType,
    SyntaxOperator,
    SyntaxPunctuation,
    ThinkingOff,
    ThinkingMinimal,
    ThinkingLow,
    ThinkingMedium,
    ThinkingHigh,
    ThinkingXhigh,
    ThinkingMax,
    BashMode,
}

impl ThemeColor {
    fn from_key(key: &str) -> Option<Self> {
        let color = match key {
            "accent" => Self::Accent,
            "border" => Self::Border,
            "borderAccent" => Self::BorderAccent,
            "borderMuted" => Self::BorderMuted,
            "success" => Self::Success,
            "error" => Self::Error,
            "warning" => Self::Warning,
            "muted" => Self::Muted,
            "dim" => Self::Dim,
            "text" => Self::Text,
            "thinkingText" => Self::ThinkingText,
            "searchMatchText" => Self::SearchMatchText,
            "userMessageText" => Self::UserMessageText,
            "customMessageText" => Self::CustomMessageText,
            "customMessageLabel" => Self::CustomMessageLabel,
            "toolTitle" => Self::ToolTitle,
            "toolOutput" => Self::ToolOutput,
            "mdHeading" => Self::MdHeading,
            "mdLink" => Self::MdLink,
            "mdLinkUrl" => Self::MdLinkUrl,
            "mdCode" => Self::MdCode,
            "mdCodeBlock" => Self::MdCodeBlock,
            "mdCodeBlockBorder" => Self::MdCodeBlockBorder,
            "mdQuote" => Self::MdQuote,
            "mdQuoteBorder" => Self::MdQuoteBorder,
            "mdHr" => Self::MdHr,
            "mdListBullet" => Self::MdListBullet,
            "toolDiffAdded" => Self::ToolDiffAdded,
            "toolDiffRemoved" => Self::ToolDiffRemoved,
            "toolDiffContext" => Self::ToolDiffContext,
            "syntaxComment" => Self::SyntaxComment,
            "syntaxKeyword" => Self::SyntaxKeyword,
            "syntaxFunction" => Self::SyntaxFunction,
            "syntaxVariable" => Self::SyntaxVariable,
            "syntaxString" => Self::SyntaxString,
            "syntaxNumber" => Self::SyntaxNumber,
            "syntaxType" => Self::SyntaxType,
            "syntaxOperator" => Self::SyntaxOperator,
            "syntaxPunctuation" => Self::SyntaxPunctuation,
            "thinkingOff" => Self::ThinkingOff,
            "thinkingMinimal" => Self::ThinkingMinimal,
            "thinkingLow" => Self::ThinkingLow,
            "thinkingMedium" => Self::ThinkingMedium,
            "thinkingHigh" => Self::ThinkingHigh,
            "thinkingXhigh" => Self::ThinkingXhigh,
            "thinkingMax" => Self::ThinkingMax,
            "bashMode" => Self::BashMode,
            _ => return None,
        };
        Some(color)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accent => "accent",
            Self::Border => "border",
            Self::BorderAccent => "borderAccent",
            Self::BorderMuted => "borderMuted",
            Self::Success => "success",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Muted => "muted",
            Self::Dim => "dim",
            Self::Text => "text",
            Self::ThinkingText => "thinkingText",
            Self::SearchMatchText => "searchMatchText",
            Self::UserMessageText => "userMessageText",
            Self::CustomMessageText => "customMessageText",
            Self::CustomMessageLabel => "customMessageLabel",
            Self::ToolTitle => "toolTitle",
            Self::ToolOutput => "toolOutput",
            Self::MdHeading => "mdHeading",
            Self::MdLink => "mdLink",
            Self::MdLinkUrl => "mdLinkUrl",
            Self::MdCode => "mdCode",
            Self::MdCodeBlock => "mdCodeBlock",
            Self::MdCodeBlockBorder => "mdCodeBlockBorder",
            Self::MdQuote => "mdQuote",
            Self::MdQuoteBorder => "mdQuoteBorder",
            Self::MdHr => "mdHr",
            Self::MdListBullet => "mdListBullet",
            Self::ToolDiffAdded => "toolDiffAdded",
            Self::ToolDiffRemoved => "toolDiffRemoved",
            Self::ToolDiffContext => "toolDiffContext",
            Self::SyntaxComment => "syntaxComment",
            Self::SyntaxKeyword => "syntaxKeyword",
            Self::SyntaxFunction => "syntaxFunction",
            Self::SyntaxVariable => "syntaxVariable",
            Self::SyntaxString => "syntaxString",
            Self::SyntaxNumber => "syntaxNumber",
            Self::SyntaxType => "syntaxType",
            Self::SyntaxOperator => "syntaxOperator",
            Self::SyntaxPunctuation => "syntaxPunctuation",
            Self::ThinkingOff => "thinkingOff",
            Self::ThinkingMinimal => "thinkingMinimal",
            Self::ThinkingLow => "thinkingLow",
            Self::ThinkingMedium => "thinkingMedium",
            Self::ThinkingHigh => "thinkingHigh",
            Self::ThinkingXhigh => "thinkingXhigh",
            Self::ThinkingMax => "thinkingMax",
            Self::BashMode => "bashMode",
        }
    }
}

/// Background theme colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeBg {
    SelectedBg,
    ScrollbarThumb,
    SearchMatchBg,
    UserMessageBg,
    CustomMessageBg,
    ToolPendingBg,
    ToolSuccessBg,
    ToolErrorBg,
}

impl ThemeBg {
    fn from_key(key: &str) -> Option<Self> {
        let background = match key {
            "selectedBg" => Self::SelectedBg,
            "scrollbarThumb" => Self::ScrollbarThumb,
            "searchMatchBg" => Self::SearchMatchBg,
            "userMessageBg" => Self::UserMessageBg,
            "customMessageBg" => Self::CustomMessageBg,
            "toolPendingBg" => Self::ToolPendingBg,
            "toolSuccessBg" => Self::ToolSuccessBg,
            "toolErrorBg" => Self::ToolErrorBg,
            _ => return None,
        };
        Some(background)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SelectedBg => "selectedBg",
            Self::ScrollbarThumb => "scrollbarThumb",
            Self::SearchMatchBg => "searchMatchBg",
            Self::UserMessageBg => "userMessageBg",
            Self::CustomMessageBg => "customMessageBg",
            Self::ToolPendingBg => "toolPendingBg",
            Self::ToolSuccessBg => "toolSuccessBg",
            Self::ToolErrorBg => "toolErrorBg",
        }
    }
}

/// Terminal color depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    Truecolor,
    Color256,
}

/// Theme loading and color-resolution failures, with TypeScript message text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeError {
    InvalidHexColor(String),
    InvalidColorValue(String),
    CircularVariableReference(String),
    VariableReferenceNotFound(String),
    Parse { label: String, message: String },
    Invalid { label: String, message: String },
    NameWithSlash(String),
    NotFound(String),
    NoSourcePath(String),
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHexColor(hex) => write!(formatter, "Invalid hex color: {hex}"),
            Self::InvalidColorValue(color) => write!(formatter, "Invalid color value: {color}"),
            Self::CircularVariableReference(name) => {
                write!(formatter, "Circular variable reference detected: {name}")
            }
            Self::VariableReferenceNotFound(name) => write!(formatter, "Variable reference not found: {name}"),
            Self::Parse { label, message } => write!(formatter, "Failed to parse theme {label}: {message}"),
            Self::Invalid { message, .. } => formatter.write_str(message),
            Self::NameWithSlash(name) => write!(
                formatter,
                "Invalid theme name \"{name}\": theme names cannot contain \"/\" because it is reserved for automatic light/dark theme settings."
            ),
            Self::NotFound(name) => write!(formatter, "Theme not found: {name}"),
            Self::NoSourcePath(name) => {
                write!(formatter, "Theme \"{name}\" does not have a source path for export")
            }
        }
    }
}

impl std::error::Error for ThemeError {}

/// Validated (or built-in raw) theme JSON.
#[derive(Clone, Debug)]
pub struct ThemeJson {
    pub name: String,
    pub vars: Map<String, Value>,
    pub colors: Map<String, Value>,
    pub export: Option<Map<String, Value>>,
}

// ============================================================================
// Color Utilities
// ============================================================================

fn hex_to_rgb(hex: &str) -> Result<RgbColor, ThemeError> {
    let cleaned = hex.replacen('#', "", 1);
    if cleaned.len() != 6 {
        return Err(ThemeError::InvalidHexColor(hex.to_owned()));
    }
    let channel = |start: usize| parse_int(&cleaned[start..start + 2], 16);
    let (r, g, b) = (channel(0), channel(2), channel(4));
    if r.is_nan() || g.is_nan() || b.is_nan() {
        return Err(ThemeError::InvalidHexColor(hex.to_owned()));
    }
    Ok(RgbColor {
        r: r as u8,
        g: g as u8,
        b: b as u8,
    })
}

const CUBE_VALUES: [i64; 6] = [0, 95, 135, 175, 215, 255];

fn gray_values() -> [i64; 24] {
    std::array::from_fn(|index| 8 + index as i64 * 10)
}

fn find_closest_cube_index(value: i64) -> usize {
    let mut min_dist = i64::MAX;
    let mut min_idx = 0;
    for (index, cube) in CUBE_VALUES.iter().enumerate() {
        let dist = (value - cube).abs();
        if dist < min_dist {
            min_dist = dist;
            min_idx = index;
        }
    }
    min_idx
}

fn find_closest_gray_index(gray: i64) -> usize {
    let values = gray_values();
    let mut min_dist = i64::MAX;
    let mut min_idx = 0;
    for (index, value) in values.iter().enumerate() {
        let dist = (gray - value).abs();
        if dist < min_dist {
            min_dist = dist;
            min_idx = index;
        }
    }
    min_idx
}

fn color_distance(left: (i64, i64, i64), right: (i64, i64, i64)) -> f64 {
    let dr = (left.0 - right.0) as f64;
    let dg = (left.1 - right.1) as f64;
    let db = (left.2 - right.2) as f64;
    dr * dr * 0.299 + dg * dg * 0.587 + db * db * 0.114
}

fn rgb_to_256(r: i64, g: i64, b: i64) -> i64 {
    let r_idx = find_closest_cube_index(r);
    let g_idx = find_closest_cube_index(g);
    let b_idx = find_closest_cube_index(b);
    let cube = (CUBE_VALUES[r_idx], CUBE_VALUES[g_idx], CUBE_VALUES[b_idx]);
    let cube_index = 16 + 36 * r_idx as i64 + 6 * g_idx as i64 + b_idx as i64;
    let cube_dist = color_distance((r, g, b), cube);

    let gray = (0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64).round() as i64;
    let gray_idx = find_closest_gray_index(gray);
    let gray_value = gray_values()[gray_idx];
    let gray_index = 232 + gray_idx as i64;
    let gray_dist = color_distance((r, g, b), (gray_value, gray_value, gray_value));

    let spread = r.max(g).max(b) - r.min(g).min(b);
    if spread < 10 && gray_dist < cube_dist {
        return gray_index;
    }
    cube_index
}

fn hex_to_256(hex: &str) -> Result<i64, ThemeError> {
    let rgb = hex_to_rgb(hex)?;
    Ok(rgb_to_256(i64::from(rgb.r), i64::from(rgb.g), i64::from(rgb.b)))
}

fn fg_ansi(color: &ColorValue, mode: ColorMode) -> Result<String, ThemeError> {
    match color {
        ColorValue::Text(text) if text.is_empty() => Ok("\x1b[39m".to_owned()),
        ColorValue::Index(index) => Ok(format!("\x1b[38;5;{index}m")),
        ColorValue::Text(text) if text.starts_with('#') => match mode {
            ColorMode::Truecolor => {
                let rgb = hex_to_rgb(text)?;
                Ok(format!("\x1b[38;2;{};{};{}m", rgb.r, rgb.g, rgb.b))
            }
            ColorMode::Color256 => Ok(format!("\x1b[38;5;{}m", hex_to_256(text)?)),
        },
        ColorValue::Text(text) => Err(ThemeError::InvalidColorValue(text.clone())),
    }
}

fn bg_ansi(color: &ColorValue, mode: ColorMode) -> Result<String, ThemeError> {
    match color {
        ColorValue::Text(text) if text.is_empty() => Ok("\x1b[49m".to_owned()),
        ColorValue::Index(index) => Ok(format!("\x1b[48;5;{index}m")),
        ColorValue::Text(text) if text.starts_with('#') => match mode {
            ColorMode::Truecolor => {
                let rgb = hex_to_rgb(text)?;
                Ok(format!("\x1b[48;2;{};{};{}m", rgb.r, rgb.g, rgb.b))
            }
            ColorMode::Color256 => Ok(format!("\x1b[48;5;{}m", hex_to_256(text)?)),
        },
        ColorValue::Text(text) => Err(ThemeError::InvalidColorValue(text.clone())),
    }
}

fn color_value_from_json(value: &Value) -> ColorValue {
    match value {
        Value::Number(number) => ColorValue::Index(number.as_i64().unwrap_or_default()),
        Value::String(text) => ColorValue::Text(text.clone()),
        _ => ColorValue::Text(String::new()),
    }
}

fn resolve_var_refs(
    value: &Value,
    vars: &Map<String, Value>,
    visited: &mut HashSet<String>,
) -> Result<ColorValue, ThemeError> {
    let resolved = color_value_from_json(value);
    let reference = match &resolved {
        ColorValue::Text(text) if !text.is_empty() && !text.starts_with('#') => text.clone(),
        _ => return Ok(resolved),
    };
    if visited.contains(&reference) {
        return Err(ThemeError::CircularVariableReference(reference));
    }
    let Some(target) = vars.get(&reference) else {
        return Err(ThemeError::VariableReferenceNotFound(reference));
    };
    visited.insert(reference);
    resolve_var_refs(target, vars, visited)
}

fn resolve_theme_colors(
    colors: &Map<String, Value>,
    vars: &Map<String, Value>,
) -> Result<Vec<(String, ColorValue)>, ThemeError> {
    colors
        .iter()
        .map(|(key, value)| {
            let resolved = resolve_var_refs(value, vars, &mut HashSet::new())?;
            Ok((key.clone(), resolved))
        })
        .collect()
}

fn with_theme_color_fallbacks(colors: &Map<String, Value>) -> Map<String, Value> {
    let mut merged = colors.clone();
    let fallbacks = [
        ("thinkingMax", "thinkingXhigh"),
        ("scrollbarThumb", "selectedBg"),
        ("searchMatchBg", "selectedBg"),
        ("searchMatchText", "text"),
    ];
    for (key, fallback_key) in fallbacks {
        if !merged.contains_key(key)
            && let Some(fallback) = colors.get(fallback_key)
        {
            merged.insert(key.to_owned(), fallback.clone());
        }
    }
    merged
}

// ============================================================================
// Theme Class
// ============================================================================

/// A resolved theme: ANSI-wrapped color lookups plus text styles.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: Option<String>,
    pub source_path: Option<PathBuf>,
    pub source_info: Option<SourceInfo>,
    fg_colors: HashMap<ThemeColor, String>,
    bg_colors: HashMap<ThemeBg, String>,
    mode: ColorMode,
    chalk: Chalk,
}

impl Theme {
    pub fn new(
        fg_colors: &[(String, ColorValue)],
        bg_colors: &[(String, ColorValue)],
        mode: ColorMode,
        name: Option<String>,
        source_path: Option<PathBuf>,
    ) -> Result<Self, ThemeError> {
        let mut resolved_fg = HashMap::new();
        for (key, value) in fg_colors {
            let ansi = fg_ansi(value, mode)?;
            if let Some(color) = ThemeColor::from_key(key) {
                resolved_fg.insert(color, ansi);
            }
        }
        let mut resolved_bg = HashMap::new();
        for (key, value) in bg_colors {
            let ansi = bg_ansi(value, mode)?;
            if let Some(background) = ThemeBg::from_key(key) {
                resolved_bg.insert(background, ansi);
            }
        }
        Ok(Self {
            name,
            source_path,
            source_info: None,
            fg_colors: resolved_fg,
            bg_colors: resolved_bg,
            mode,
            chalk: Chalk::stdout(),
        })
    }

    fn fg_ansi(&self, color: ThemeColor) -> &str {
        self.fg_colors
            .get(&color)
            .unwrap_or_else(|| panic!("Unknown theme color: {}", color.as_str()))
    }

    fn bg_ansi(&self, color: ThemeBg) -> &str {
        self.bg_colors
            .get(&color)
            .unwrap_or_else(|| panic!("Unknown theme background color: {}", color.as_str()))
    }

    /// Wrap `text` in the foreground color and reset only the foreground.
    pub fn fg(&self, color: ThemeColor, text: &str) -> String {
        format!("{}{text}\x1b[39m", self.fg_ansi(color))
    }

    /// Wrap `text` in the background color and reset only the background.
    pub fn bg(&self, color: ThemeBg, text: &str) -> String {
        format!("{}{text}\x1b[49m", self.bg_ansi(color))
    }

    pub fn bold(&self, text: &str) -> String {
        self.chalk.bold(text)
    }

    pub fn italic(&self, text: &str) -> String {
        self.chalk.italic(text)
    }

    pub fn underline(&self, text: &str) -> String {
        self.chalk.underline(text)
    }

    pub fn inverse(&self, text: &str) -> String {
        self.chalk.inverse(text)
    }

    pub fn strikethrough(&self, text: &str) -> String {
        self.chalk.strikethrough(text)
    }

    pub fn get_fg_ansi(&self, color: ThemeColor) -> &str {
        self.fg_ansi(color)
    }

    pub fn get_bg_ansi(&self, color: ThemeBg) -> &str {
        self.bg_ansi(color)
    }

    pub const fn get_color_mode(&self) -> ColorMode {
        self.mode
    }

    /// Border color callback for a thinking level.
    pub fn get_thinking_border_color(self: &Arc<Self>, level: ThinkingLevel) -> StyleFn {
        let color = match level {
            ThinkingLevel::Off => ThemeColor::ThinkingOff,
            ThinkingLevel::Minimal => ThemeColor::ThinkingMinimal,
            ThinkingLevel::Low => ThemeColor::ThinkingLow,
            ThinkingLevel::Medium => ThemeColor::ThinkingMedium,
            ThinkingLevel::High => ThemeColor::ThinkingHigh,
            ThinkingLevel::XHigh => ThemeColor::ThinkingXhigh,
            ThinkingLevel::Max | ThinkingLevel::Ultra => ThemeColor::ThinkingMax,
        };
        let theme = Arc::clone(self);
        Rc::new(move |text: &str| theme.fg(color, text))
    }

    pub fn get_bash_mode_border_color(self: &Arc<Self>) -> StyleFn {
        let theme = Arc::clone(self);
        Rc::new(move |text: &str| theme.fg(ThemeColor::BashMode, text))
    }
}

// ============================================================================
// Theme Loading
// ============================================================================

#[derive(Default)]
struct ThemeState {
    builtin: Option<Map<String, Value>>,
    registered: HashMap<String, Arc<Theme>>,
    current_theme_name: Option<String>,
    global_theme: Option<Arc<Theme>>,
    on_theme_change: Option<Rc<dyn Fn()>>,
    watcher: Option<notify::RecommendedWatcher>,
}

thread_local! {
    static STATE: RefCell<ThemeState> = RefCell::new(ThemeState::default());
}

fn with_state<R>(f: impl FnOnce(&mut ThemeState) -> R) -> R {
    STATE.with(|state| f(&mut state.borrow_mut()))
}

/// Reload parsed by the file-watcher thread, applied on the next theme access.
static PENDING_RELOAD: Mutex<Option<(String, Arc<Theme>)>> = Mutex::new(None);

struct WatchControl {
    generation: u64,
    watched_name: Option<String>,
}

static WATCH_CONTROL: Mutex<WatchControl> = Mutex::new(WatchControl {
    generation: 0,
    watched_name: None,
});

fn read_theme_file(path: &Path) -> Result<String, ThemeError> {
    fs::read_to_string(path).map_err(|error| ThemeError::Parse {
        label: path.display().to_string(),
        message: error.to_string(),
    })
}

fn get_builtin_themes() -> Map<String, Value> {
    if let Some(builtin) = with_state(|state| state.builtin.clone()) {
        return builtin;
    }
    let themes_dir = get_themes_dir();
    let read_from_disk = package_dir_override().is_some();
    let mut builtin = Map::new();
    for name in ["dark", "light"] {
        let path = themes_dir.join(format!("{name}.json"));
        let content = match embedded_builtin_theme(name) {
            Some(embedded) if !read_from_disk => embedded.to_owned(),
            _ => fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("Failed to read built-in theme {}: {error}", path.display())),
        };
        let value: Value = serde_json::from_str(&content)
            .unwrap_or_else(|error| panic!("Failed to parse built-in theme {}: {error}", path.display()));
        builtin.insert(name.to_owned(), value);
    }
    with_state(|state| state.builtin = Some(builtin.clone()));
    builtin
}

/// Names of all built-in, custom, and registered themes, sorted by name.
pub fn get_available_themes() -> Vec<String> {
    get_available_themes_with_paths()
        .into_iter()
        .map(|info| info.name)
        .collect()
}

/// Theme name/path pairs; built-in themes first, then custom and registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeInfo {
    pub name: String,
    pub path: Option<PathBuf>,
}

pub fn get_available_themes_with_paths() -> Vec<ThemeInfo> {
    let themes_dir = get_themes_dir();
    let mut result: Vec<ThemeInfo> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut add_theme = |theme_info: ThemeInfo| {
        if seen.contains(&theme_info.name) {
            return;
        }
        seen.insert(theme_info.name.clone());
        result.push(theme_info);
    };

    for name in get_builtin_themes().keys() {
        add_theme(ThemeInfo {
            name: name.clone(),
            path: Some(themes_dir.join(format!("{name}.json"))),
        });
    }

    for theme_info in get_custom_theme_infos() {
        add_theme(theme_info);
    }

    let registered: Vec<(String, Option<PathBuf>)> = with_state(|state| {
        state
            .registered
            .iter()
            .map(|(name, theme)| (name.clone(), theme.source_path.clone()))
            .collect()
    });
    for (name, path) in registered {
        add_theme(ThemeInfo { name, path });
    }

    let locale = pi_js_compat::host_locale();
    result.sort_by(|left, right| compare_in_locale(&left.name, &right.name, &locale));
    result
}

fn get_custom_theme_infos() -> Vec<ThemeInfo> {
    let custom_themes_dir = get_custom_themes_dir();
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir(&custom_themes_dir) else {
        return result;
    };
    for entry in entries.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if !file.ends_with(".json") {
            continue;
        }
        let theme_path = custom_themes_dir.join(&file);
        if let Ok(custom_theme) = load_theme_from_path(&theme_path, None)
            && let Some(name) = custom_theme.name.clone()
        {
            result.push(ThemeInfo {
                name,
                path: Some(theme_path),
            });
        }
    }
    result
}

fn assert_theme_name_is_valid(name: &str) -> Result<(), ThemeError> {
    if name.contains('/') {
        return Err(ThemeError::NameWithSlash(name.to_owned()));
    }
    Ok(())
}

fn json_type_errors(path: &str, value: &Value) -> Vec<String> {
    match value {
        Value::String(_) => Vec::new(),
        Value::Number(number) if number.as_i64().is_some() => {
            let index = number.as_i64().unwrap_or_default();
            if index < 0 {
                vec![
                    format!("  - {path}: must be string"),
                    format!("  - {path}: must be >= 0"),
                    format!("  - {path}: must match a schema in anyOf"),
                ]
            } else if index > 255 {
                vec![
                    format!("  - {path}: must be string"),
                    format!("  - {path}: must be <= 255"),
                    format!("  - {path}: must match a schema in anyOf"),
                ]
            } else {
                Vec::new()
            }
        }
        _ => vec![
            format!("  - {path}: must be string"),
            format!("  - {path}: must be integer"),
            format!("  - {path}: must match a schema in anyOf"),
        ],
    }
}

/// TypeBox yields at most eight errors before its iterator stops.
const MAX_VALIDATION_ERRORS: usize = 8;

enum ThemeValidationError {
    MissingColors(Vec<String>),
    Other(String),
}

fn validate_theme_json(label: &str, json: &Value) -> Result<(), ThemeError> {
    let Value::Object(root) = json else {
        return Err(invalid_theme(label, "/", "must be object"));
    };

    let mut errors: Vec<ThemeValidationError> = Vec::new();

    let mut required: Vec<&str> = Vec::new();
    if !root.contains_key("name") {
        required.push("name");
    }
    if !root.contains_key("colors") {
        required.push("colors");
    }
    if !required.is_empty() {
        errors.push(ThemeValidationError::Other(format!(
            "  - /: must have required properties {}",
            required.join(", ")
        )));
    }

    if let Some(schema) = root.get("$schema")
        && !schema.is_string()
    {
        errors.push(ThemeValidationError::Other("  - /$schema: must be string".to_owned()));
    }

    if let Some(name) = root.get("name")
        && !name.is_string()
    {
        errors.push(ThemeValidationError::Other("  - /name: must be string".to_owned()));
    }

    if let Some(vars) = root.get("vars") {
        match vars {
            Value::Object(vars) => {
                for (key, value) in vars {
                    errors.extend(
                        json_type_errors(&format!("/vars/{key}"), value)
                            .into_iter()
                            .map(ThemeValidationError::Other),
                    );
                }
            }
            _ => errors.push(ThemeValidationError::Other("  - /vars: must be object".to_owned())),
        }
    }

    if let Some(colors) = root.get("colors") {
        match colors {
            Value::Object(colors) => {
                let missing: Vec<String> = REQUIRED_COLORS
                    .iter()
                    .filter(|key| !colors.contains_key(**key))
                    .map(|key| (*key).to_owned())
                    .collect();
                if !missing.is_empty() {
                    errors.push(ThemeValidationError::MissingColors(missing));
                }
                for key in COLOR_KEYS {
                    if let Some(value) = colors.get(*key) {
                        errors.extend(
                            json_type_errors(&format!("/colors/{key}"), value)
                                .into_iter()
                                .map(ThemeValidationError::Other),
                        );
                    }
                }
            }
            _ => errors.push(ThemeValidationError::Other("  - /colors: must be object".to_owned())),
        }
    }

    if let Some(export) = root.get("export") {
        match export {
            Value::Object(export) => {
                for key in EXPORT_KEYS {
                    if let Some(value) = export.get(*key) {
                        errors.extend(
                            json_type_errors(&format!("/export/{key}"), value)
                                .into_iter()
                                .map(ThemeValidationError::Other),
                        );
                    }
                }
            }
            _ => errors.push(ThemeValidationError::Other("  - /export: must be object".to_owned())),
        }
    }

    errors.truncate(MAX_VALIDATION_ERRORS);
    let mut missing_colors: Vec<String> = Vec::new();
    let mut other_errors: Vec<String> = Vec::new();
    for error in errors {
        match error {
            ThemeValidationError::MissingColors(missing) => missing_colors.extend(missing),
            ThemeValidationError::Other(message) => other_errors.push(message),
        }
    }

    if missing_colors.is_empty() && other_errors.is_empty() {
        return Ok(());
    }

    let mut message = format!("Invalid theme \"{label}\":\n");
    if !missing_colors.is_empty() {
        missing_colors.sort();
        message.push_str("\nMissing required color tokens:\n");
        message.push_str(
            &missing_colors
                .iter()
                .map(|color| format!("  - {color}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        message.push_str("\n\nPlease add these colors to your theme's \"colors\" object.");
        message.push_str("\nSee the built-in themes (dark.json, light.json) for reference values.");
    }
    if !other_errors.is_empty() {
        message.push_str("\n\nOther errors:\n");
        message.push_str(&other_errors.join("\n"));
    }
    Err(ThemeError::Invalid {
        label: label.to_owned(),
        message,
    })
}

fn invalid_theme(label: &str, path: &str, message: &str) -> ThemeError {
    ThemeError::Invalid {
        label: label.to_owned(),
        message: format!("Invalid theme \"{label}\":\n\n\nOther errors:\n  - {path}: {message}"),
    }
}

fn theme_json_from_value(value: &Value) -> ThemeJson {
    let object = value.as_object().cloned().unwrap_or_default();
    ThemeJson {
        name: object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        vars: object
            .get("vars")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
        colors: object
            .get("colors")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
        export: object.get("export").and_then(Value::as_object).cloned(),
    }
}

fn parse_theme_json(label: &str, json: &Value) -> Result<ThemeJson, ThemeError> {
    validate_theme_json(label, json)?;
    let theme_json = theme_json_from_value(json);
    assert_theme_name_is_valid(&theme_json.name)?;
    Ok(theme_json)
}

fn parse_theme_json_content(label: &str, content: &str) -> Result<ThemeJson, ThemeError> {
    let json: Value = serde_json::from_str(content).map_err(|error| ThemeError::Parse {
        label: label.to_owned(),
        message: error.to_string(),
    })?;
    parse_theme_json(label, &json)
}

fn load_theme_json(name: &str) -> Result<ThemeJson, ThemeError> {
    let builtin_themes = get_builtin_themes();
    if let Some(builtin) = builtin_themes.get(name) {
        return Ok(theme_json_from_value(builtin));
    }
    let registered = with_state(|state| state.registered.get(name).cloned());
    if let Some(registered) = registered {
        let Some(source_path) = registered.source_path.clone() else {
            return Err(ThemeError::NoSourcePath(name.to_owned()));
        };
        let content = read_theme_file(&source_path)?;
        return parse_theme_json_content(&source_path.display().to_string(), &content);
    }
    let theme_path = get_custom_themes_dir().join(format!("{name}.json"));
    if !theme_path.exists() {
        return Err(ThemeError::NotFound(name.to_owned()));
    }
    let content = read_theme_file(&theme_path)?;
    parse_theme_json_content(name, &content)
}

fn create_theme(
    theme_json: &ThemeJson,
    mode: Option<ColorMode>,
    source_path: Option<PathBuf>,
) -> Result<Theme, ThemeError> {
    let color_mode = mode.unwrap_or(if get_capabilities().true_color {
        ColorMode::Truecolor
    } else {
        ColorMode::Color256
    });
    let colors = with_theme_color_fallbacks(&theme_json.colors);
    let resolved = resolve_theme_colors(&colors, &theme_json.vars)?;
    let mut fg_colors = Vec::new();
    let mut bg_colors = Vec::new();
    for (key, value) in resolved {
        if ThemeBg::from_key(&key).is_some() {
            bg_colors.push((key, value));
        } else {
            fg_colors.push((key, value));
        }
    }
    Theme::new(
        &fg_colors,
        &bg_colors,
        color_mode,
        Some(theme_json.name.clone()),
        source_path,
    )
}

/// Load and validate a theme from an explicit file path.
pub fn load_theme_from_path(theme_path: &Path, mode: Option<ColorMode>) -> Result<Theme, ThemeError> {
    let content = read_theme_file(theme_path)?;
    let label = theme_path.display().to_string();
    let theme_json = parse_theme_json_content(&label, &content)?;
    create_theme(&theme_json, mode, Some(theme_path.to_path_buf()))
}

fn load_theme(name: &str, mode: Option<ColorMode>) -> Result<Arc<Theme>, ThemeError> {
    if let Some(registered) = with_state(|state| state.registered.get(name).cloned()) {
        return Ok(registered);
    }
    let theme_json = load_theme_json(name)?;
    Ok(Arc::new(create_theme(&theme_json, mode, None)?))
}

/// Load a theme by name, returning `None` when loading fails.
pub fn get_theme_by_name(name: &str) -> Option<Arc<Theme>> {
    load_theme(name, None).ok()
}

// ============================================================================
// Automatic Light/Dark Theme Settings
// ============================================================================

/// Terminal background classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalTheme {
    Dark,
    Light,
}

impl TerminalTheme {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// Parsed `<light>/<dark>` automatic theme setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoThemeSetting {
    pub light_theme: String,
    pub dark_theme: String,
}

pub fn parse_auto_theme_setting(theme_setting: Option<&str>) -> Option<AutoThemeSetting> {
    let theme_setting = theme_setting?;
    if theme_setting.is_empty() {
        return None;
    }
    let slash_index = theme_setting.find('/')?;
    if theme_setting[slash_index + 1..].contains('/') {
        return None;
    }
    let light_theme = theme_setting[..slash_index].trim().to_owned();
    let dark_theme = theme_setting[slash_index + 1..].trim().to_owned();
    if light_theme.is_empty() || dark_theme.is_empty() {
        return None;
    }
    Some(AutoThemeSetting {
        light_theme,
        dark_theme,
    })
}

pub fn resolve_theme_setting(theme_setting: Option<&str>, terminal_theme: TerminalTheme) -> Option<String> {
    if let Some(auto_theme) = parse_auto_theme_setting(theme_setting) {
        return Some(match terminal_theme {
            TerminalTheme::Light => auto_theme.light_theme,
            TerminalTheme::Dark => auto_theme.dark_theme,
        });
    }
    let theme_setting = theme_setting?;
    if theme_setting.contains('/') {
        return None;
    }
    Some(theme_setting.to_owned())
}

// ============================================================================
// Terminal Background Detection
// ============================================================================

/// Where the detected terminal theme came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeDetectionSource {
    TerminalBackground,
    ColorFgBg,
    Fallback,
}

impl ThemeDetectionSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TerminalBackground => "terminal background",
            Self::ColorFgBg => "COLORFGBG",
            Self::Fallback => "fallback",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeDetectionConfidence {
    High,
    Low,
}

impl ThemeDetectionConfidence {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Low => "low",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalThemeDetection {
    pub theme: TerminalTheme,
    pub source: ThemeDetectionSource,
    pub detail: String,
    pub confidence: ThemeDetectionConfidence,
}

/// Query failure for `queryTerminalBackgroundColor`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeQueryError(pub String);

impl std::fmt::Display for ThemeQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ThemeQueryError {}

/// Terminal background detector (`queryTerminalBackgroundColor`).
pub trait TerminalBackgroundThemeDetector {
    fn query_terminal_background_color(&self, timeout: Duration) -> Result<Option<RgbColor>, ThemeQueryError>;
}

/// Detector with the optional color-scheme query.
pub trait TerminalAutoThemeDetector: TerminalBackgroundThemeDetector {
    fn query_terminal_color_scheme(
        &self,
        _timeout: Duration,
    ) -> Option<Result<Option<TerminalTheme>, ThemeQueryError>> {
        None
    }
}

fn get_color_fg_bg_background_index(colorfgbg: &str) -> Option<i64> {
    let parts: Vec<&str> = colorfgbg.split(';').collect();
    for part in parts.iter().rev() {
        let background = parse_int(part.trim(), 10);
        if background.fract() == 0.0 && (0.0..=255.0).contains(&background) {
            return Some(background as i64);
        }
    }
    None
}

fn get_rgb_color_luminance(rgb: RgbColor) -> f64 {
    let to_linear = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * to_linear(rgb.r) + 0.7152 * to_linear(rgb.g) + 0.0722 * to_linear(rgb.b)
}

fn get_ansi_color_luminance(index: i64) -> f64 {
    get_rgb_color_luminance(hex_to_rgb(&ansi256_to_hex(index)).expect("ansi256 colors are valid hex"))
}

pub fn get_theme_for_rgb_color(rgb: RgbColor) -> TerminalTheme {
    if get_rgb_color_luminance(rgb) >= 0.5 {
        TerminalTheme::Light
    } else {
        TerminalTheme::Dark
    }
}

pub fn detect_terminal_background_from_env(env: &BTreeMap<String, String>) -> TerminalThemeDetection {
    let colorfgbg = env.get("COLORFGBG").map(String::as_str).unwrap_or_default();
    if let Some(background) = get_color_fg_bg_background_index(colorfgbg) {
        return TerminalThemeDetection {
            theme: if get_ansi_color_luminance(background) >= 0.5 {
                TerminalTheme::Light
            } else {
                TerminalTheme::Dark
            },
            source: ThemeDetectionSource::ColorFgBg,
            detail: format!("background color index {background}"),
            confidence: ThemeDetectionConfidence::High,
        };
    }
    TerminalThemeDetection {
        theme: TerminalTheme::Dark,
        source: ThemeDetectionSource::Fallback,
        detail: "no terminal background hint found".to_owned(),
        confidence: ThemeDetectionConfidence::Low,
    }
}

pub fn detect_terminal_background_theme(
    ui: &dyn TerminalBackgroundThemeDetector,
    timeout: Duration,
    env: &BTreeMap<String, String>,
) -> TerminalThemeDetection {
    if let Ok(Some(rgb)) = ui.query_terminal_background_color(timeout) {
        return TerminalThemeDetection {
            theme: get_theme_for_rgb_color(rgb),
            source: ThemeDetectionSource::TerminalBackground,
            detail: format!("OSC 11 background rgb({}, {}, {})", rgb.r, rgb.g, rgb.b),
            confidence: ThemeDetectionConfidence::High,
        };
    }
    detect_terminal_background_from_env(env)
}

pub fn detect_terminal_theme_for_auto(
    ui: &dyn TerminalAutoThemeDetector,
    timeout: Duration,
    env: &BTreeMap<String, String>,
) -> TerminalTheme {
    let color_scheme = ui.query_terminal_color_scheme(timeout);
    let background_theme = detect_terminal_background_theme(ui, timeout, env);
    match color_scheme {
        Some(Ok(Some(theme))) => theme,
        _ => background_theme.theme,
    }
}

/// Default theme from `COLORFGBG`, defaulting to dark.
pub fn get_default_theme() -> TerminalTheme {
    detect_terminal_background_from_env(&process_env()).theme
}

fn process_env() -> BTreeMap<String, String> {
    pi_js_compat::process_env::vars().into_iter().collect()
}

// ============================================================================
// Global Theme Instance
// ============================================================================

/// Theme lookup used by components; panics with the TypeScript message when the
/// global theme has not been initialized.
pub fn current_theme() -> Arc<Theme> {
    apply_pending_reload();
    with_state(|state| state.global_theme.clone())
        .unwrap_or_else(|| panic!("Theme not initialized. Call initTheme() first."))
}

/// Non-panicking [`current_theme`] for callers that surface the error.
pub fn current_theme_if_initialized() -> Option<Arc<Theme>> {
    apply_pending_reload();
    with_state(|state| state.global_theme.clone())
}

/// Apply a watcher reload parsed on another thread, then fire the change callback.
fn apply_pending_reload() {
    let pending = PENDING_RELOAD.lock().unwrap_or_else(PoisonError::into_inner).take();
    let Some((name, theme)) = pending else {
        return;
    };
    let callback = with_state(|state| {
        state.registered.insert(name, Arc::clone(&theme));
        state.global_theme = Some(theme);
        state.on_theme_change.clone()
    });
    if let Some(callback) = callback {
        callback();
    }
}

/// Run `f` with the global theme.
pub fn with_theme<R>(f: impl FnOnce(&Theme) -> R) -> R {
    let theme = current_theme();
    f(&theme)
}

/// Replace the registered theme set.
pub fn set_registered_themes(themes: Vec<Arc<Theme>>) -> Result<(), ThemeError> {
    let mut registered = HashMap::new();
    for theme in themes {
        let Some(name) = theme.name.clone() else {
            continue;
        };
        assert_theme_name_is_valid(&name)?;
        registered.insert(name, theme);
    }
    with_state(|state| state.registered = registered);
    Ok(())
}

fn set_global_theme(theme: Arc<Theme>) {
    with_state(|state| state.global_theme = Some(theme));
}

/// Initialize the global theme, falling back to `dark` on load failure.
pub fn init_theme(theme_name: Option<&str>, enable_watcher: bool) {
    let name = theme_name
        .map(str::to_owned)
        .unwrap_or_else(|| get_default_theme().as_str().to_owned());
    with_state(|state| state.current_theme_name = Some(name.clone()));
    match load_theme(&name, None) {
        Ok(theme) => {
            set_global_theme(theme);
            if enable_watcher {
                start_theme_watcher();
            }
        }
        Err(_) => {
            with_state(|state| state.current_theme_name = Some("dark".to_owned()));
            let dark = load_theme("dark", None).expect("built-in dark theme must load");
            set_global_theme(dark);
        }
    }
}

/// Result of [`set_theme`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetThemeResult {
    pub success: bool,
    pub error: Option<String>,
}

/// Switch the global theme, falling back to `dark` and reporting the failure.
pub fn set_theme(name: &str, enable_watcher: bool) -> SetThemeResult {
    with_state(|state| state.current_theme_name = Some(name.to_owned()));
    match load_theme(name, None) {
        Ok(theme) => {
            set_global_theme(theme);
            if enable_watcher {
                start_theme_watcher();
            }
            let callback = with_state(|state| state.on_theme_change.clone());
            if let Some(callback) = callback {
                callback();
            }
            SetThemeResult {
                success: true,
                error: None,
            }
        }
        Err(error) => {
            with_state(|state| state.current_theme_name = Some("dark".to_owned()));
            let dark = load_theme("dark", None).expect("built-in dark theme must load");
            set_global_theme(dark);
            SetThemeResult {
                success: false,
                error: Some(error.to_string()),
            }
        }
    }
}

/// Use an in-memory theme instance; stops the file watcher.
pub fn set_theme_instance(theme: Arc<Theme>) {
    set_global_theme(theme);
    with_state(|state| state.current_theme_name = Some("<in-memory>".to_owned()));
    stop_theme_watcher();
    let callback = with_state(|state| state.on_theme_change.clone());
    if let Some(callback) = callback {
        callback();
    }
}

pub fn on_theme_change(callback: Rc<dyn Fn()>) {
    with_state(|state| state.on_theme_change = Some(callback));
}

fn start_theme_watcher() {
    stop_theme_watcher();

    let Some(watched_theme_name) = with_state(|state| state.current_theme_name.clone()) else {
        return;
    };
    if watched_theme_name == "dark" || watched_theme_name == "light" {
        return;
    }

    let custom_themes_dir = get_custom_themes_dir();
    let watched_file_name = format!("{watched_theme_name}.json");
    let theme_file = custom_themes_dir.join(&watched_file_name);
    if !theme_file.exists() {
        return;
    }

    let generation = {
        let mut control = WATCH_CONTROL.lock().unwrap_or_else(PoisonError::into_inner);
        control.generation += 1;
        control.watched_name = Some(watched_theme_name.clone());
        control.generation
    };

    let listener_file = watched_file_name.clone();
    let listener = move |_event: WatchEventKind, filename: Option<PathBuf>| {
        let should_reload = match filename {
            None => true,
            Some(filename) => filename.to_string_lossy() == listener_file,
        };
        if !should_reload {
            return;
        }
        let control = WATCH_CONTROL.lock().unwrap_or_else(PoisonError::into_inner);
        if control.generation != generation || control.watched_name.as_deref() != Some(watched_theme_name.as_str()) {
            return;
        }
        drop(control);
        schedule_theme_reload(generation, &theme_file);
    };
    let watcher = watch_with_error_handler(&custom_themes_dir, listener, || {
        stop_theme_watcher();
    });
    with_state(|state| state.watcher = watcher);
}

fn schedule_theme_reload(generation: u64, theme_file: &Path) {
    let theme_file = theme_file.to_path_buf();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        {
            let control = WATCH_CONTROL.lock().unwrap_or_else(PoisonError::into_inner);
            if control.generation != generation {
                return;
            }
        }
        if !theme_file.exists() {
            return;
        }
        let Ok(reloaded_theme) = load_theme_from_path(&theme_file, None) else {
            return;
        };
        let Some(name) = reloaded_theme.name.clone() else {
            return;
        };
        *PENDING_RELOAD.lock().unwrap_or_else(PoisonError::into_inner) = Some((name, Arc::new(reloaded_theme)));
    });
}

pub fn stop_theme_watcher() {
    {
        let mut control = WATCH_CONTROL.lock().unwrap_or_else(PoisonError::into_inner);
        control.generation += 1;
        control.watched_name = None;
    }
    with_state(|state| close_watcher(state.watcher.take()));
}

// ============================================================================
// HTML Export Helpers
// ============================================================================

fn ansi256_to_hex(index: i64) -> String {
    const BASIC_COLORS: [&str; 16] = [
        "#000000", "#800000", "#008000", "#808000", "#000080", "#800080", "#008080", "#c0c0c0", "#808080", "#ff0000",
        "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff",
    ];
    if index < 16 {
        return BASIC_COLORS[index as usize].to_owned();
    }
    if index < 232 {
        let cube_index = index - 16;
        let to_hex = |value: i64| {
            if value == 0 {
                "00".to_owned()
            } else {
                format!("{:02x}", 55 + value * 40)
            }
        };
        let r = cube_index / 36;
        let g = (cube_index % 36) / 6;
        let b = cube_index % 6;
        return format!("#{}{}{}", to_hex(r), to_hex(g), to_hex(b));
    }
    let gray = 8 + (index - 232) * 10;
    let gray_hex = format!("{gray:02x}");
    format!("#{gray_hex}{gray_hex}{gray_hex}")
}

/// Resolved theme colors as CSS-compatible hex strings, in theme key order.
pub fn get_resolved_theme_colors(theme_name: Option<&str>) -> Result<Vec<(String, String)>, ThemeError> {
    let name = match theme_name {
        Some(name) => name.to_owned(),
        None => with_state(|state| state.current_theme_name.clone())
            .unwrap_or_else(|| get_default_theme().as_str().to_owned()),
    };
    let is_light = name == "light";
    let theme_json = load_theme_json(&name)?;
    let colors = with_theme_color_fallbacks(&theme_json.colors);
    let resolved = resolve_theme_colors(&colors, &theme_json.vars)?;
    let default_text = if is_light { "#000000" } else { "#e5e5e7" };
    Ok(resolved
        .into_iter()
        .map(|(key, value)| {
            let css = match value {
                ColorValue::Index(index) => ansi256_to_hex(index),
                ColorValue::Text(text) if text.is_empty() => default_text.to_owned(),
                ColorValue::Text(text) => text,
            };
            (key, css)
        })
        .collect())
}

pub fn is_light_theme(theme_name: Option<&str>) -> bool {
    theme_name == Some("light")
}

/// Explicit HTML export colors from the theme JSON.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeExportColors {
    pub page_bg: Option<String>,
    pub card_bg: Option<String>,
    pub info_bg: Option<String>,
}

pub fn get_theme_export_colors(theme_name: Option<&str>) -> ThemeExportColors {
    let name = match theme_name {
        Some(name) => name.to_owned(),
        None => with_state(|state| state.current_theme_name.clone())
            .unwrap_or_else(|| get_default_theme().as_str().to_owned()),
    };
    let Ok(theme_json) = load_theme_json(&name) else {
        return ThemeExportColors::default();
    };
    let Some(export_section) = theme_json.export else {
        return ThemeExportColors::default();
    };
    let resolve = |key: &str| -> Option<String> {
        let value = export_section.get(key)?;
        match resolve_var_refs(value, &theme_json.vars, &mut HashSet::new()) {
            Ok(ColorValue::Index(index)) => Some(ansi256_to_hex(index)),
            Ok(ColorValue::Text(text)) if text.is_empty() => None,
            Ok(ColorValue::Text(text)) => Some(text),
            Err(_) => None,
        }
    };
    ThemeExportColors {
        page_bg: resolve("pageBg"),
        card_bg: resolve("cardBg"),
        info_bg: resolve("infoBg"),
    }
}

// ============================================================================
// TUI Helpers
// ============================================================================

fn syntax_formatter(color: ThemeColor) -> HighlightFormatter {
    let theme = current_theme();
    let ansi = theme.get_fg_ansi(color).to_owned();
    std::sync::Arc::new(move |text: &str| format!("{ansi}{text}\x1b[39m"))
}

fn style_formatter(style: Style) -> HighlightFormatter {
    let chalk = Chalk::stdout();
    std::sync::Arc::new(move |text: &str| chalk.paint(&[style], text))
}

fn build_cli_highlight_theme(_theme: &Theme) -> HighlightTheme {
    let mut highlight_theme = HashMap::new();
    let mut insert = |name: &str, formatter: HighlightFormatter| {
        highlight_theme.insert(name.to_owned(), formatter);
    };
    insert("keyword", syntax_formatter(ThemeColor::SyntaxKeyword));
    insert("built_in", syntax_formatter(ThemeColor::SyntaxType));
    insert("literal", syntax_formatter(ThemeColor::SyntaxNumber));
    insert("number", syntax_formatter(ThemeColor::SyntaxNumber));
    insert("regexp", syntax_formatter(ThemeColor::SyntaxString));
    insert("string", syntax_formatter(ThemeColor::SyntaxString));
    insert("comment", syntax_formatter(ThemeColor::SyntaxComment));
    insert("doctag", syntax_formatter(ThemeColor::SyntaxComment));
    insert("meta", syntax_formatter(ThemeColor::Muted));
    insert("function", syntax_formatter(ThemeColor::SyntaxFunction));
    insert("title", syntax_formatter(ThemeColor::SyntaxFunction));
    insert("class", syntax_formatter(ThemeColor::SyntaxType));
    insert("type", syntax_formatter(ThemeColor::SyntaxType));
    insert("tag", syntax_formatter(ThemeColor::SyntaxPunctuation));
    insert("name", syntax_formatter(ThemeColor::SyntaxKeyword));
    insert("attr", syntax_formatter(ThemeColor::SyntaxVariable));
    insert("variable", syntax_formatter(ThemeColor::SyntaxVariable));
    insert("params", syntax_formatter(ThemeColor::SyntaxVariable));
    insert("operator", syntax_formatter(ThemeColor::SyntaxOperator));
    insert("punctuation", syntax_formatter(ThemeColor::SyntaxPunctuation));
    insert("emphasis", style_formatter(Style::Italic));
    insert("strong", style_formatter(Style::Bold));
    insert("link", style_formatter(Style::Underline));
    insert("addition", syntax_formatter(ThemeColor::ToolDiffAdded));
    insert("deletion", syntax_formatter(ThemeColor::ToolDiffRemoved));
    highlight_theme
}

fn styled_lines(code: &str) -> Vec<String> {
    let theme = current_theme();
    code.split('\n')
        .map(|line| theme.fg(ThemeColor::MdCodeBlock, line))
        .collect()
}

/// Highlight code with syntax coloring based on the language name.
pub fn highlight_code(code: &str, lang: Option<&str>) -> Vec<String> {
    let valid_lang = lang.filter(|language| supports_language(language));
    let Some(valid_lang) = valid_lang else {
        return styled_lines(code);
    };
    let theme = current_theme();
    let options = HighlightOptions {
        language: Some(valid_lang.to_owned()),
        ignore_illegals: Some(true),
        language_subset: None,
        theme: build_cli_highlight_theme(&theme),
    };
    highlight(code, &options).split('\n').map(str::to_owned).collect()
}

/// Get a language identifier from a file path extension.
pub fn get_language_from_path(file_path: &str) -> Option<&'static str> {
    let extension = file_path.rsplit('.').next()?.to_lowercase();
    if extension.is_empty() {
        return None;
    }
    let language = match extension.as_str() {
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" => "python",
        "rb" => "ruby",
        "rs" => "rust",
        "go" => "go",
        "java" => "java",
        "kt" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "php" => "php",
        "sh" | "bash" | "zsh" => "bash",
        "fish" => "fish",
        "ps1" => "powershell",
        "sql" => "sql",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "sass" => "sass",
        "less" => "less",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "md" | "markdown" => "markdown",
        "dockerfile" => "dockerfile",
        "makefile" => "makefile",
        "cmake" => "cmake",
        "lua" => "lua",
        "perl" => "perl",
        "r" => "r",
        "scala" => "scala",
        "clj" => "clojure",
        "ex" | "exs" => "elixir",
        "erl" => "erlang",
        "hs" => "haskell",
        "ml" => "ocaml",
        "vim" => "vim",
        "graphql" => "graphql",
        "proto" => "protobuf",
        "tf" | "hcl" => "hcl",
        _ => return None,
    };
    Some(language)
}

struct PiMarkdownTheme;

impl MarkdownTheme for PiMarkdownTheme {
    fn heading(&self, text: &str) -> String {
        with_theme(|theme| theme.fg(ThemeColor::MdHeading, text))
    }

    fn link(&self, text: &str) -> String {
        with_theme(|theme| theme.fg(ThemeColor::MdLink, text))
    }

    fn link_url(&self, text: &str) -> String {
        with_theme(|theme| theme.fg(ThemeColor::MdLinkUrl, text))
    }

    fn code(&self, text: &str) -> String {
        with_theme(|theme| theme.fg(ThemeColor::MdCode, text))
    }

    fn code_block(&self, text: &str) -> String {
        with_theme(|theme| theme.fg(ThemeColor::MdCodeBlock, text))
    }

    fn code_block_border(&self, text: &str) -> String {
        with_theme(|theme| theme.fg(ThemeColor::MdCodeBlockBorder, text))
    }

    fn quote(&self, text: &str) -> String {
        with_theme(|theme| theme.fg(ThemeColor::MdQuote, text))
    }

    fn quote_border(&self, text: &str) -> String {
        with_theme(|theme| theme.fg(ThemeColor::MdQuoteBorder, text))
    }

    fn hr(&self, text: &str) -> String {
        with_theme(|theme| theme.fg(ThemeColor::MdHr, text))
    }

    fn list_bullet(&self, text: &str) -> String {
        with_theme(|theme| theme.fg(ThemeColor::MdListBullet, text))
    }

    fn bold(&self, text: &str) -> String {
        with_theme(|theme| theme.bold(text))
    }

    fn italic(&self, text: &str) -> String {
        with_theme(|theme| theme.italic(text))
    }

    fn strikethrough(&self, text: &str) -> String {
        with_theme(|theme| theme.strikethrough(text))
    }

    fn underline(&self, text: &str) -> String {
        with_theme(|theme| theme.underline(text))
    }

    fn highlight_code(&self, code: &str, lang: Option<&str>) -> Option<Vec<String>> {
        let valid_lang = lang.filter(|language| supports_language(language));
        let Some(valid_lang) = valid_lang else {
            return Some(styled_lines(code));
        };
        let theme = current_theme();
        let options = HighlightOptions {
            language: Some(valid_lang.to_owned()),
            ignore_illegals: Some(true),
            language_subset: None,
            theme: build_cli_highlight_theme(&theme),
        };
        Some(highlight(code, &options).split('\n').map(str::to_owned).collect())
    }
}

pub fn get_markdown_theme() -> Box<dyn MarkdownTheme> {
    Box::new(PiMarkdownTheme)
}

pub fn get_select_list_theme() -> SelectListTheme {
    SelectListTheme {
        selected_prefix: Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Accent, text))),
        selected_text: Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Accent, text))),
        description: Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
        scroll_info: Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
        no_match: Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
    }
}

pub fn get_editor_theme() -> EditorTheme {
    EditorTheme {
        border_color: Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::BorderMuted, text))),
        prompt_prefix: Some(Rc::new(|text: &str| {
            with_theme(|theme| theme.fg(ThemeColor::Accent, text))
        })),
        select_list: get_select_list_theme(),
    }
}

pub fn get_settings_list_theme() -> SettingsListTheme {
    SettingsListTheme {
        label: Rc::new(|text: &str, selected: bool| {
            if selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, text))
            } else {
                text.to_owned()
            }
        }),
        value: Rc::new(|text: &str, selected: bool| {
            if selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, text))
            } else {
                with_theme(|theme| theme.fg(ThemeColor::Muted, text))
            }
        }),
        description: Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Dim, text))),
        cursor: with_theme(|theme| theme.fg(ThemeColor::Accent, "→ ")),
        hint: Rc::new(|text: &str| with_theme(|theme| theme.fg(ThemeColor::Dim, text))),
    }
}
