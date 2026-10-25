//! Ported from `packages/coding-agent/src/modes/interactive/theme/theme.ts`.
//!
//! Theme JSON parsing/validation, ANSI color resolution, the process-global
//! theme instance, terminal background detection, and the TUI theme adapters.
//!
//! Typed Rust API exclusions (August 12, 2026 decision): `ThemeColor`/`ThemeBg`
//! are enums, so the TypeScript `Unknown theme color` throw is unreachable for
//! callers; a missing entry in a hand-built [`Theme`] panics instead.

mod error;
mod theme_controller;
pub use error::ThemeError;

pub use theme_controller::{InteractiveThemeController, ThemeUi};

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use pi_agent_core::ThinkingLevel;
use pi_js_compat::collator::locale_compare_utf16;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value, encode_node_utf8, parse_lossless};
use pi_js_compat::number::parse_int;
use pi_tui::components::editor::EditorTheme;
use pi_tui::components::markdown::MarkdownTheme;
use pi_tui::components::select_list::SelectListTheme;
use pi_tui::components::settings_list::SettingsListTheme;
use pi_tui::components::text::StyleFn;
use pi_tui::terminal_colors::RgbColor;
use pi_tui::terminal_image::get_capabilities;

use crate::config::{embedded_builtin_theme, get_custom_themes_dir, get_themes_dir, package_dir_override};
use crate::core::source_info::SourceInfo;
use crate::core::tools::node_fs::{node_fs_error, node_read_error};
use crate::utils::chalk::{Chalk, Style};
use crate::utils::fs_watch::{close_watcher, watch_with_error_handler};
use crate::utils::syntax_highlight::{
    HighlightFormatter, HighlightOptions, HighlightTheme, highlight, supports_language,
};

// ============================================================================
// Types & Schema
// ============================================================================

/// A theme color token: a hex string, a variable reference, or a 256-color index.
#[derive(Clone, Debug, PartialEq)]
pub enum ColorValue {
    Text(JsString),
    Index(f64),
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
    BashMode,
}

impl ThemeColor {
    fn from_key(key: &JsString) -> Option<Self> {
        let key = COLOR_KEYS.iter().find(|candidate| key == **candidate).copied()?;
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
            Self::BashMode => "bashMode",
        }
    }
}

/// Background theme colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeBg {
    SelectedBg,
    UserMessageBg,
    CustomMessageBg,
    ToolPendingBg,
    ToolSuccessBg,
    ToolErrorBg,
}

impl ThemeBg {
    fn from_key(key: &JsString) -> Option<Self> {
        let key = COLOR_KEYS.iter().find(|candidate| key == **candidate).copied()?;
        let background = match key {
            "selectedBg" => Self::SelectedBg,
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

/// Validated (or built-in raw) theme JSON.
#[derive(Clone, Debug)]
pub struct ThemeJson {
    pub name: JsString,
    pub vars: JsObject,
    pub colors: JsObject,
    pub export: Option<JsObject>,
}

// ============================================================================
// Color Utilities
// ============================================================================

fn hex_to_rgb(hex: &JsString) -> Result<(f64, f64, f64), ThemeError> {
    let mut cleaned = hex.clone();
    if let Some(index) = cleaned.find('#') {
        cleaned.replace_range(index..index + 1, "");
    }
    if cleaned.len() != 6 {
        return Err(ThemeError::InvalidHexColor(hex.into()));
    }
    let channel = |start: usize| parse_int(&cleaned.units()[start..start + 2], 16);
    let (r, g, b) = (channel(0), channel(2), channel(4));
    if r.is_nan() || g.is_nan() || b.is_nan() {
        return Err(ThemeError::InvalidHexColor(hex.into()));
    }
    Ok((r, g, b))
}

const CUBE_VALUES: [f64; 6] = [0.0, 95.0, 135.0, 175.0, 215.0, 255.0];

fn gray_values() -> [f64; 24] {
    std::array::from_fn(|index| 8.0 + index as f64 * 10.0)
}

fn find_closest_cube_index(value: f64) -> usize {
    let mut min_dist = f64::INFINITY;
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

fn find_closest_gray_index(gray: f64) -> usize {
    let values = gray_values();
    let mut min_dist = f64::INFINITY;
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

fn color_distance(left: (f64, f64, f64), right: (f64, f64, f64)) -> f64 {
    let dr = left.0 - right.0;
    let dg = left.1 - right.1;
    let db = left.2 - right.2;
    dr * dr * 0.299 + dg * dg * 0.587 + db * db * 0.114
}

fn rgb_to_256(r: f64, g: f64, b: f64) -> i64 {
    let r_idx = find_closest_cube_index(r);
    let g_idx = find_closest_cube_index(g);
    let b_idx = find_closest_cube_index(b);
    let cube = (CUBE_VALUES[r_idx], CUBE_VALUES[g_idx], CUBE_VALUES[b_idx]);
    let cube_index = 16 + 36 * r_idx as i64 + 6 * g_idx as i64 + b_idx as i64;
    let cube_dist = color_distance((r, g, b), cube);

    let gray = pi_js_compat::number::math_round(0.299 * r + 0.587 * g + 0.114 * b);
    let gray_idx = find_closest_gray_index(gray);
    let gray_value = gray_values()[gray_idx];
    let gray_index = 232 + gray_idx as i64;
    let gray_dist = color_distance((r, g, b), (gray_value, gray_value, gray_value));

    let spread = r.max(g).max(b) - r.min(g).min(b);
    if spread < 10.0 && gray_dist < cube_dist {
        return gray_index;
    }
    cube_index
}

fn hex_to_256(hex: &JsString) -> Result<i64, ThemeError> {
    let (r, g, b) = hex_to_rgb(hex)?;
    Ok(rgb_to_256(r, g, b))
}

fn fg_ansi(color: &ColorValue, mode: ColorMode) -> Result<String, ThemeError> {
    match color {
        ColorValue::Text(text) if text.is_empty() => Ok("\x1b[39m".to_owned()),
        ColorValue::Index(index) => Ok(format!(
            "\x1b[38;5;{}m",
            pi_js_compat::number::js_number_to_string(*index)
        )),
        ColorValue::Text(text) if text.starts_with('#') => match mode {
            ColorMode::Truecolor => {
                let (r, g, b) = hex_to_rgb(text)?;
                Ok(format!("\x1b[38;2;{r};{g};{b}m"))
            }
            ColorMode::Color256 => Ok(format!("\x1b[38;5;{}m", hex_to_256(text)?)),
        },
        ColorValue::Text(text) => Err(ThemeError::InvalidColorValue(text.into())),
    }
}

fn bg_ansi(color: &ColorValue, mode: ColorMode) -> Result<String, ThemeError> {
    match color {
        ColorValue::Text(text) if text.is_empty() => Ok("\x1b[49m".to_owned()),
        ColorValue::Index(index) => Ok(format!(
            "\x1b[48;5;{}m",
            pi_js_compat::number::js_number_to_string(*index)
        )),
        ColorValue::Text(text) if text.starts_with('#') => match mode {
            ColorMode::Truecolor => {
                let (r, g, b) = hex_to_rgb(text)?;
                Ok(format!("\x1b[48;2;{r};{g};{b}m"))
            }
            ColorMode::Color256 => Ok(format!("\x1b[48;5;{}m", hex_to_256(text)?)),
        },
        ColorValue::Text(text) => Err(ThemeError::InvalidColorValue(text.into())),
    }
}

fn color_value_from_json(value: &Value) -> ColorValue {
    match value {
        Value::Number(number) => ColorValue::Index(*number),
        Value::String(text) => ColorValue::Text(text.clone()),
        _ => ColorValue::Text(JsString::default()),
    }
}

fn resolve_var_refs(value: &Value, vars: &JsObject, visited: &mut HashSet<JsString>) -> Result<ColorValue, ThemeError> {
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

fn resolve_theme_colors(colors: &JsObject, vars: &JsObject) -> Result<Vec<(JsString, ColorValue)>, ThemeError> {
    pi_js_compat::js_json::object_entries_lossless(colors)
        .into_iter()
        .map(|(key, value)| {
            let resolved = resolve_var_refs(value, vars, &mut HashSet::new())?;
            Ok((key.clone(), resolved))
        })
        .collect()
}

// ============================================================================
// Theme Class
// ============================================================================

/// A resolved theme: ANSI-wrapped color lookups plus text styles.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: Option<JsString>,
    pub source_path: Option<JsString>,
    pub source_info: Option<SourceInfo>,
    fg_colors: HashMap<ThemeColor, String>,
    bg_colors: HashMap<ThemeBg, String>,
    mode: ColorMode,
    chalk: Chalk,
}

impl Theme {
    pub fn new(
        fg_colors: &[(JsString, ColorValue)],
        bg_colors: &[(JsString, ColorValue)],
        mode: ColorMode,
        name: Option<JsString>,
        source_path: Option<JsString>,
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
    pub fn fg(&self, color: ThemeColor, text: impl Into<JsString>) -> JsString {
        let text = text.into();
        js_format!("{}{}\x1b[39m", self.fg_ansi(color), text)
    }

    /// Wrap `text` in the background color and reset only the background.
    pub fn bg(&self, color: ThemeBg, text: impl Into<JsString>) -> JsString {
        let text = text.into();
        js_format!("{}{}\x1b[49m", self.bg_ansi(color), text)
    }

    pub fn bold(&self, text: impl Into<JsString>) -> JsString {
        self.chalk.bold(text)
    }

    pub fn italic(&self, text: impl Into<JsString>) -> JsString {
        self.chalk.italic(text)
    }

    pub fn underline(&self, text: impl Into<JsString>) -> JsString {
        self.chalk.underline(text)
    }

    pub fn inverse(&self, text: impl Into<JsString>) -> JsString {
        self.chalk.inverse(text)
    }

    pub fn strikethrough(&self, text: impl Into<JsString>) -> JsString {
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
            ThinkingLevel::XHigh | ThinkingLevel::Max | ThinkingLevel::Ultra => ThemeColor::ThinkingXhigh,
        };
        let theme = Arc::clone(self);
        Rc::new(move |text: &JsString| theme.fg(color, text))
    }

    pub fn get_bash_mode_border_color(self: &Arc<Self>) -> StyleFn {
        let theme = Arc::clone(self);
        Rc::new(move |text: &JsString| theme.fg(ThemeColor::BashMode, text))
    }
}

// ============================================================================
// Theme Loading
// ============================================================================

#[derive(Default)]
struct ThemeState {
    builtin: Option<JsObject>,
    registered: indexmap::IndexMap<JsString, Arc<Theme>>,
    current_theme_name: Option<JsString>,
    global_theme: Option<Arc<Theme>>,
    on_theme_change: Option<Rc<dyn Fn()>>,
    watcher: Option<notify::RecommendedWatcher>,
    watcher_task: Option<tokio::task::JoinHandle<()>>,
    reload_timer: Option<tokio::task::JoinHandle<()>>,
}

thread_local! {
    static STATE: RefCell<ThemeState> = RefCell::new(ThemeState::default());
}

fn with_state<R>(f: impl FnOnce(&mut ThemeState) -> R) -> R {
    STATE.with(|state| f(&mut state.borrow_mut()))
}

fn read_theme_file(path: &JsString) -> Result<JsString, ThemeError> {
    let encoded_path = encode_node_utf8(path);
    fs::read(&encoded_path)
        .map(|bytes| JsString::from(String::from_utf8_lossy(&bytes).as_ref()))
        .map_err(|error| ThemeError::Io {
            message: node_read_error(&error, path),
        })
}

fn get_builtin_themes() -> Result<JsObject, ThemeError> {
    if let Some(builtin) = with_state(|state| state.builtin.clone()) {
        return Ok(builtin);
    }
    let themes_dir = get_themes_dir()?;
    let read_from_disk = package_dir_override()?.is_some();
    let mut builtin = JsObject::new();
    for name in ["dark", "light"] {
        let path = crate::utils::paths::node_join(&themes_dir, format!("{name}.json"));
        let content = match embedded_builtin_theme(name) {
            Some(embedded) if !read_from_disk => embedded.into(),
            _ => read_theme_file(&path)?,
        };
        let value: Value = parse_lossless(&content).map_err(|error| ThemeError::Syntax {
            message: error.message().clone(),
        })?;
        builtin.insert(name.into(), value);
    }
    with_state(|state| state.builtin = Some(builtin.clone()));
    Ok(builtin)
}

/// Names of all built-in, custom, and registered themes, sorted by name.
pub fn get_available_themes() -> Result<Vec<JsString>, ThemeError> {
    Ok(get_available_themes_with_paths()?
        .into_iter()
        .map(|info| info.name)
        .collect())
}

/// Theme name/path pairs; built-in themes first, then custom and registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeInfo {
    pub name: JsString,
    pub path: Option<JsString>,
}

pub fn get_available_themes_with_paths() -> Result<Vec<ThemeInfo>, ThemeError> {
    let themes_dir = get_themes_dir()?;
    let mut result: Vec<ThemeInfo> = Vec::new();
    let mut seen: HashSet<JsString> = HashSet::new();
    let mut add_theme = |theme_info: ThemeInfo| {
        if seen.contains(&theme_info.name) {
            return;
        }
        seen.insert(theme_info.name.clone());
        result.push(theme_info);
    };

    for name in get_builtin_themes()?.keys() {
        add_theme(ThemeInfo {
            name: name.clone(),
            path: Some(themes_dir.clone() + "/" + name + ".json"),
        });
    }

    for theme_info in get_custom_theme_infos()? {
        add_theme(theme_info);
    }

    let registered: Vec<(JsString, Option<JsString>)> = with_state(|state| {
        state
            .registered
            .iter()
            .map(|(name, theme)| (name.clone(), theme.source_path.clone()))
            .collect()
    });
    for (name, path) in registered {
        add_theme(ThemeInfo { name, path });
    }

    result.sort_by(|left, right| locale_compare_utf16(left.name.units(), right.name.units()));
    Ok(result)
}

fn get_custom_theme_infos() -> Result<Vec<ThemeInfo>, ThemeError> {
    let custom_themes_dir = get_custom_themes_dir()?;
    let mut result = Vec::new();
    if !Path::new(&encode_node_utf8(&custom_themes_dir)).exists() {
        return Ok(result);
    }
    let directory = custom_themes_dir.clone();
    let scan_error = |error| ThemeError::Io {
        message: node_fs_error(&error, "scandir", &directory),
    };
    let mut files = fs::read_dir(encode_node_utf8(&custom_themes_dir))
        .map_err(scan_error)?
        .map(|entry| entry.map(|entry| entry.file_name()).map_err(scan_error))
        .collect::<Result<Vec<_>, _>>()?;
    // libuv scandir sorts raw names before Node decodes them as UTF-8.
    files.sort();
    for file in files {
        let file = pi_js_compat::node_fs::decode_node_path(&file);
        if !file.ends_with(".json") {
            continue;
        }
        let theme_path = js_format!("{}/{}", directory, file);
        if let Ok(custom_theme) = load_theme_from_path(&theme_path, None)
            && let Some(name) = custom_theme.name.clone().filter(|name| !name.is_empty())
        {
            result.push(ThemeInfo {
                name,
                path: Some(theme_path),
            });
        }
    }
    Ok(result)
}

fn assert_theme_name_is_valid(name: &JsString) -> Result<(), ThemeError> {
    if name.contains('/') {
        return Err(ThemeError::NameWithSlash(name.into()));
    }
    Ok(())
}

fn json_type_errors(path: &JsString, value: &Value) -> Vec<JsString> {
    match value {
        Value::String(_) => Vec::new(),
        Value::Number(number) if number.fract() == 0.0 => {
            let index = *number;
            if index < 0.0 {
                vec![
                    js_format!("  - {}: must be string", path),
                    js_format!("  - {}: must be >= 0", path),
                    js_format!("  - {}: must match a schema in anyOf", path),
                ]
            } else if index > 255.0 {
                vec![
                    js_format!("  - {}: must be string", path),
                    js_format!("  - {}: must be <= 255", path),
                    js_format!("  - {}: must match a schema in anyOf", path),
                ]
            } else {
                Vec::new()
            }
        }
        _ => vec![
            js_format!("  - {}: must be string", path),
            js_format!("  - {}: must be integer", path),
            js_format!("  - {}: must match a schema in anyOf", path),
        ],
    }
}

/// TypeBox yields at most eight errors before its iterator stops.
const MAX_VALIDATION_ERRORS: usize = 8;

enum ThemeValidationError {
    MissingColors(Vec<String>),
    Other(JsString),
}

fn validate_theme_json(label: &JsString, json: &Value) -> Result<(), ThemeError> {
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
        errors.push(ThemeValidationError::Other(js_format!(
            "  - /: must have required properties {}",
            required.join(", ")
        )));
    }

    if let Some(schema) = root.get("$schema")
        && !schema.is_string()
    {
        errors.push(ThemeValidationError::Other("  - /$schema: must be string".into()));
    }

    if let Some(name) = root.get("name")
        && !name.is_string()
    {
        errors.push(ThemeValidationError::Other("  - /name: must be string".into()));
    }

    if let Some(vars) = root.get("vars") {
        match vars {
            Value::Object(vars) => {
                for (key, value) in vars {
                    errors.extend(
                        json_type_errors(&js_format!("/vars/{}", key), value)
                            .into_iter()
                            .map(ThemeValidationError::Other),
                    );
                }
            }
            _ => errors.push(ThemeValidationError::Other("  - /vars: must be object".into())),
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
                            json_type_errors(&js_format!("/colors/{}", key), value)
                                .into_iter()
                                .map(ThemeValidationError::Other),
                        );
                    }
                }
            }
            _ => errors.push(ThemeValidationError::Other("  - /colors: must be object".into())),
        }
    }

    if let Some(export) = root.get("export") {
        match export {
            Value::Object(export) => {
                for key in EXPORT_KEYS {
                    if let Some(value) = export.get(*key) {
                        errors.extend(
                            json_type_errors(&js_format!("/export/{}", key), value)
                                .into_iter()
                                .map(ThemeValidationError::Other),
                        );
                    }
                }
            }
            _ => errors.push(ThemeValidationError::Other("  - /export: must be object".into())),
        }
    }

    errors.truncate(MAX_VALIDATION_ERRORS);
    let mut missing_colors: Vec<String> = Vec::new();
    let mut other_errors: Vec<JsString> = Vec::new();
    for error in errors {
        match error {
            ThemeValidationError::MissingColors(missing) => missing_colors.extend(missing),
            ThemeValidationError::Other(message) => other_errors.push(message),
        }
    }

    if missing_colors.is_empty() && other_errors.is_empty() {
        return Ok(());
    }

    let mut message = js_format!("Invalid theme \"{}\":\n", label);
    if !missing_colors.is_empty() {
        missing_colors.sort();
        message.push_str("\nMissing required color tokens:\n");
        message.push_str(
            missing_colors
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
        message.push_str(JsString::join(&other_errors, "\n"));
    }
    Err(ThemeError::Invalid {
        label: label.into(),
        message,
    })
}

fn invalid_theme(label: &JsString, path: &str, message: &str) -> ThemeError {
    ThemeError::Invalid {
        label: label.into(),
        message: js_format!(
            "Invalid theme \"{}\":\n\n\nOther errors:\n  - {}: {}",
            label,
            path,
            message
        ),
    }
}

fn theme_json_from_value(value: &Value) -> ThemeJson {
    let object = value.as_object().cloned().unwrap_or_default();
    ThemeJson {
        name: object
            .get("name")
            .and_then(Value::as_string)
            .cloned()
            .unwrap_or_default(),
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

fn parse_theme_json(label: &JsString, json: &Value) -> Result<ThemeJson, ThemeError> {
    validate_theme_json(label, json)?;
    let theme_json = theme_json_from_value(json);
    assert_theme_name_is_valid(&theme_json.name)?;
    Ok(theme_json)
}

fn parse_theme_json_content(label: &JsString, content: &JsString) -> Result<ThemeJson, ThemeError> {
    let json: Value = parse_lossless(content).map_err(|error| ThemeError::Parse {
        label: label.into(),
        message: js_format!("SyntaxError: {}", error.message()),
    })?;
    parse_theme_json(label, &json)
}

fn load_theme_json(name: &JsString) -> Result<ThemeJson, ThemeError> {
    let builtin_themes = get_builtin_themes()?;
    if let Some(builtin) = builtin_themes.get(name) {
        return Ok(theme_json_from_value(builtin));
    }
    let registered = with_state(|state| state.registered.get(name).cloned());
    if let Some(registered) = registered {
        let Some(source_path) = registered.source_path.clone().filter(|path| !path.is_empty()) else {
            return Err(ThemeError::NoSourcePath(name.into()));
        };
        let content = read_theme_file(&source_path)?;
        return parse_theme_json_content(&source_path, &content);
    }
    let theme_path = get_custom_themes_dir()? + "/" + name + ".json";
    if !Path::new(&encode_node_utf8(&theme_path)).exists() {
        return Err(ThemeError::NotFound(name.into()));
    }
    let content = read_theme_file(&theme_path)?;
    parse_theme_json_content(name, &content)
}

fn create_theme(
    theme_json: &ThemeJson,
    mode: Option<ColorMode>,
    source_path: Option<JsString>,
) -> Result<Theme, ThemeError> {
    let color_mode = mode.unwrap_or(if get_capabilities().true_color {
        ColorMode::Truecolor
    } else {
        ColorMode::Color256
    });
    let resolved = resolve_theme_colors(&theme_json.colors, &theme_json.vars)?;
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
pub fn load_theme_from_path(theme_path: &JsString, mode: Option<ColorMode>) -> Result<Theme, ThemeError> {
    let content = read_theme_file(theme_path)?;
    let theme_json = parse_theme_json_content(theme_path, &content)?;
    create_theme(&theme_json, mode, Some(theme_path.clone()))
}

fn load_theme(name: &JsString, mode: Option<ColorMode>) -> Result<Arc<Theme>, ThemeError> {
    if let Some(registered) = with_state(|state| state.registered.get(name).cloned()) {
        return Ok(registered);
    }
    let theme_json = load_theme_json(name)?;
    Ok(Arc::new(create_theme(&theme_json, mode, None)?))
}

/// Load a theme by name, returning `None` when loading fails.
pub fn get_theme_by_name(name: &JsString) -> Option<Arc<Theme>> {
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
    pub light_theme: JsString,
    pub dark_theme: JsString,
}

pub fn parse_auto_theme_setting(theme_setting: Option<&JsString>) -> Option<AutoThemeSetting> {
    let theme_setting = theme_setting?;
    if theme_setting.is_empty() {
        return None;
    }
    let slash_index = theme_setting.find('/')?;
    if theme_setting.slice(slash_index + 1, theme_setting.len()).contains('/') {
        return None;
    }
    let light_theme = theme_setting.slice(0, slash_index).trim();
    let dark_theme = theme_setting.slice(slash_index + 1, theme_setting.len()).trim();
    if light_theme.is_empty() || dark_theme.is_empty() {
        return None;
    }
    Some(AutoThemeSetting {
        light_theme,
        dark_theme,
    })
}

pub fn resolve_theme_setting(theme_setting: Option<&JsString>, terminal_theme: TerminalTheme) -> Option<JsString> {
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
        let background = parse_int(&part.trim().encode_utf16().collect::<Vec<_>>(), 10);
        if background.fract() == 0.0 && (0.0..=255.0).contains(&background) {
            return Some(background as i64);
        }
    }
    None
}

fn get_rgb_color_luminance((r, g, b): (f64, f64, f64)) -> f64 {
    let to_linear = |channel: f64| {
        let value = channel / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b)
}

fn get_ansi_color_luminance(index: i64) -> f64 {
    get_rgb_color_luminance(hex_to_rgb(&ansi256_to_hex(index).into()).expect("ansi256 colors are valid hex"))
}

pub fn get_theme_for_rgb_color(rgb: RgbColor) -> TerminalTheme {
    if get_rgb_color_luminance((rgb.r, rgb.g, rgb.b)) >= 0.5 {
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
            detail: format!(
                "OSC 11 background rgb({}, {}, {})",
                pi_js_compat::number::js_number_to_string(rgb.r),
                pi_js_compat::number::js_number_to_string(rgb.g),
                pi_js_compat::number::js_number_to_string(rgb.b)
            ),
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
    if let Some(Ok(Some(theme))) = ui.query_terminal_color_scheme(timeout) {
        return theme;
    }
    detect_terminal_background_theme(ui, timeout, env).theme
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

/// Theme lookup used by components. An uninitialized theme is an uncaught
/// TypeScript throw (`theme` proxy), so it reaches the process-level handler.
pub fn current_theme() -> Arc<Theme> {
    try_current_theme()
        .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message()))
}

/// Fallible [`current_theme`]: `Err` when `initTheme` has not installed a theme.
pub fn try_current_theme() -> Result<Arc<Theme>, ThemeError> {
    with_state(|state| state.global_theme.clone()).ok_or(ThemeError::NotInitialized)
}

/// Non-panicking [`current_theme`] for callers that surface the error.
pub fn current_theme_if_initialized() -> Option<Arc<Theme>> {
    with_state(|state| state.global_theme.clone())
}

/// Run `f` with the global theme.
pub fn with_theme<R>(f: impl FnOnce(&Theme) -> R) -> R {
    let theme = current_theme();
    f(&theme)
}

/// Replace the registered theme set.
pub fn set_registered_themes(themes: Vec<Arc<Theme>>) -> Result<(), ThemeError> {
    with_state(|state| state.registered.clear());
    for theme in themes {
        let Some(name) = theme.name.clone().filter(|name| !name.is_empty()) else {
            continue;
        };
        assert_theme_name_is_valid(&name)?;
        with_state(|state| state.registered.insert(name, theme));
    }
    Ok(())
}

fn set_global_theme(theme: Arc<Theme>) {
    with_state(|state| state.global_theme = Some(theme));
}

/// Initialize the global theme, falling back to `dark` on load failure.
///
/// The fallback can fail too (missing built-in themes); TypeScript lets that
/// second throw escape `initTheme`, so the error is returned to the caller.
pub fn init_theme(theme_name: Option<&JsString>, enable_watcher: bool) -> Result<(), ThemeError> {
    let name = theme_name
        .cloned()
        .unwrap_or_else(|| get_default_theme().as_str().into());
    with_state(|state| state.current_theme_name = Some(name.clone()));
    let initialized = load_theme(&name, None).and_then(|theme| {
        set_global_theme(theme);
        if enable_watcher {
            start_theme_watcher()?;
        }
        Ok(())
    });
    if initialized.is_err() {
        with_state(|state| state.current_theme_name = Some("dark".into()));
        let dark = load_theme(&"dark".into(), None)?;
        set_global_theme(dark);
    }
    Ok(())
}

/// Result of [`set_theme`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetThemeResult {
    pub success: bool,
    pub error: Option<JsString>,
}

/// Switch the global theme, falling back to `dark` and reporting the failure.
///
/// The dark fallback is loaded outside the `try` in TypeScript, so its failure
/// escapes `setTheme` and is returned here.
pub fn set_theme(name: &JsString, enable_watcher: bool) -> Result<SetThemeResult, ThemeError> {
    with_state(|state| state.current_theme_name = Some(name.to_owned()));
    let result = crate::utils::uncaught::catch_panics(|| {
        let theme = load_theme(name, None)?;
        set_global_theme(theme);
        if enable_watcher {
            start_theme_watcher()?;
        }
        let callback = with_state(|state| state.on_theme_change.clone());
        if let Some(callback) = callback {
            callback();
        }
        Ok::<_, ThemeError>(())
    });
    let error = match result {
        Ok(Ok(())) => {
            return Ok(SetThemeResult {
                success: true,
                error: None,
            });
        }
        Ok(Err(error)) => error.message(),
        Err(error) => caught_theme_error_message(error.as_ref()),
    };
    with_state(|state| state.current_theme_name = Some("dark".into()));
    let dark = load_theme(&"dark".into(), None)?;
    set_global_theme(dark);
    Ok(SetThemeResult {
        success: false,
        error: Some(error),
    })
}

fn caught_theme_error_message(error: &(dyn std::any::Any + Send)) -> JsString {
    if let Some(error) = error.downcast_ref::<pi_js_compat::node_error::NativeNodeError>() {
        return error.message.clone();
    }
    if let Some(error) = error.downcast_ref::<crate::utils::uncaught::UncaughtError>() {
        return error.message.clone();
    }
    if let Some(message) = error.downcast_ref::<String>() {
        return message.into();
    }
    if let Some(message) = error.downcast_ref::<&str>() {
        return (*message).into();
    }
    crate::utils::uncaught::format_uncaught(error)
}

/// Use an in-memory theme instance; stops the file watcher.
pub fn set_theme_instance(theme: Arc<Theme>) {
    set_global_theme(theme);
    with_state(|state| state.current_theme_name = Some("<in-memory>".into()));
    stop_theme_watcher();
    let callback = with_state(|state| state.on_theme_change.clone());
    if let Some(callback) = callback {
        callback();
    }
}

pub fn on_theme_change(callback: Rc<dyn Fn()>) {
    with_state(|state| state.on_theme_change = Some(callback));
}

fn start_theme_watcher() -> Result<(), ThemeError> {
    stop_theme_watcher();

    let Some(watched_theme_name) = with_state(|state| state.current_theme_name.clone()) else {
        return Ok(());
    };
    if watched_theme_name == "dark" || watched_theme_name == "light" {
        return Ok(());
    }

    let custom_themes_dir = get_custom_themes_dir()?;
    let watched_file_name = watched_theme_name.clone() + ".json";
    let theme_file = custom_themes_dir.clone() + "/" + &watched_file_name;
    if !Path::new(&encode_node_utf8(&theme_file)).exists() {
        return Ok(());
    }

    let (sender, mut events) = tokio::sync::mpsc::unbounded_channel();
    let errors = sender.clone();
    let watcher = watch_with_error_handler(
        Path::new(&encode_node_utf8(&custom_themes_dir)),
        move |_event, filename| {
            let _ = sender.send(ThemeWatchEvent::File(filename));
        },
        move || {
            let _ = errors.send(ThemeWatchEvent::Error);
        },
    );
    let task = tokio::task::spawn_local(async move {
        while let Some(event) = events.recv().await {
            match event {
                ThemeWatchEvent::Error => with_state(|state| close_watcher(state.watcher.take())),
                ThemeWatchEvent::File(filename) => {
                    if !is_current_theme(&watched_theme_name) {
                        continue;
                    }
                    if let Some(filename) = filename
                        && filename
                            .file_name()
                            .map(pi_js_compat::node_fs::decode_node_path)
                            .as_ref()
                            != Some(&watched_file_name)
                    {
                        continue;
                    }
                    schedule_theme_reload(&watched_theme_name, &theme_file);
                }
            }
        }
    });
    with_state(|state| {
        state.watcher = watcher;
        state.watcher_task = Some(task);
    });
    Ok(())
}

enum ThemeWatchEvent {
    File(Option<PathBuf>),
    Error,
}

fn is_current_theme(name: &JsString) -> bool {
    with_state(|state| state.current_theme_name.as_ref() == Some(name))
}

fn schedule_theme_reload(watched_name: &JsString, theme_file: &JsString) {
    with_state(|state| {
        if let Some(timer) = state.reload_timer.take() {
            timer.abort();
        }
    });
    let watched_name = watched_name.clone();
    let theme_file = theme_file.clone();
    let timer = tokio::task::spawn_local(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        with_state(|state| state.reload_timer = None);
        if !is_current_theme(&watched_name) || !Path::new(&encode_node_utf8(&theme_file)).exists() {
            return;
        }
        crate::utils::uncaught::swallow_panics(|| reload_watched_theme(watched_name, &theme_file));
    });
    with_state(|state| state.reload_timer = Some(timer));
}

fn reload_watched_theme(watched_name: JsString, theme_file: &JsString) {
    let Ok(reloaded_theme) = load_theme_from_path(theme_file, None) else {
        return;
    };
    let theme = Arc::new(reloaded_theme);
    let callback = with_state(|state| {
        state.registered.insert(watched_name, theme.clone());
        state.global_theme = Some(theme);
        state.on_theme_change.clone()
    });
    if let Some(callback) = callback {
        callback();
    }
}

pub fn stop_theme_watcher() {
    with_state(|state| {
        if let Some(timer) = state.reload_timer.take() {
            timer.abort();
        }
        if let Some(task) = state.watcher_task.take() {
            task.abort();
        }
        close_watcher(state.watcher.take());
    });
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
pub fn get_resolved_theme_colors(theme_name: Option<&JsString>) -> Result<Vec<(JsString, JsString)>, ThemeError> {
    let name = match theme_name {
        Some(name) => name.to_owned(),
        None => {
            with_state(|state| state.current_theme_name.clone()).unwrap_or_else(|| get_default_theme().as_str().into())
        }
    };
    let is_light = name == "light";
    let theme_json = load_theme_json(&name)?;
    let resolved = resolve_theme_colors(&theme_json.colors, &theme_json.vars)?;
    let default_text = if is_light { "#000000" } else { "#e5e5e7" };
    Ok(resolved
        .into_iter()
        .map(|(key, value)| {
            let css = match value {
                ColorValue::Index(index) => ansi256_to_hex(index as i64).into(),
                ColorValue::Text(text) if text.is_empty() => default_text.into(),
                ColorValue::Text(text) => text,
            };
            (key, css)
        })
        .collect())
}

pub fn is_light_theme(theme_name: Option<&JsString>) -> bool {
    theme_name.is_some_and(|name| name == "light")
}

/// Explicit HTML export colors from the theme JSON.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeExportColors {
    pub page_bg: Option<JsString>,
    pub card_bg: Option<JsString>,
    pub info_bg: Option<JsString>,
}

pub fn get_theme_export_colors(theme_name: Option<&JsString>) -> ThemeExportColors {
    let name = match theme_name {
        Some(name) => name.to_owned(),
        None => {
            with_state(|state| state.current_theme_name.clone()).unwrap_or_else(|| get_default_theme().as_str().into())
        }
    };
    let Ok(theme_json) = load_theme_json(&name) else {
        return ThemeExportColors::default();
    };
    let Some(export_section) = theme_json.export else {
        return ThemeExportColors::default();
    };
    let resolve = |key: &str| -> Option<JsString> {
        let value = export_section.get(key)?;
        match resolve_var_refs(value, &theme_json.vars, &mut HashSet::new()) {
            Ok(ColorValue::Index(index)) => Some(ansi256_to_hex(index as i64).into()),
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
    std::sync::Arc::new(move |text: &JsString| with_theme(|theme| theme.fg(color, text)))
}

fn style_formatter(style: Style) -> HighlightFormatter {
    let chalk = Chalk::stdout();
    std::sync::Arc::new(move |text: &JsString| chalk.paint(&[style], text))
}

fn build_cli_highlight_theme() -> HighlightTheme {
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

fn styled_lines(code: &JsString) -> Vec<JsString> {
    let theme = current_theme();
    code.split('\n')
        .map(|line| theme.fg(ThemeColor::MdCodeBlock, line))
        .collect()
}

/// Named language from the explicit extension table; prototype leaks are excluded.
pub type HighlightLanguage = JsString;

/// Highlight a named language, preserving the source's grammar catch.
pub fn highlight_code(
    code: &JsString,
    lang: Option<&HighlightLanguage>,
) -> Result<Vec<JsString>, Box<pi_js_compat::node_error::NativeNodeError>> {
    let valid_lang = lang.filter(|language| supports_language(language));
    let Some(valid_lang) = valid_lang else {
        return Ok(styled_lines(code));
    };
    Ok(try_highlight_code(code, valid_lang).unwrap_or_else(|| code.split('\n').collect()))
}

fn try_highlight_code(code: &JsString, language: &JsString) -> Option<Vec<JsString>> {
    let options = HighlightOptions {
        language: Some(language.clone()),
        ignore_illegals: Some(true),
        language_subset: None,
        theme: build_cli_highlight_theme(),
    };
    // Source catches both grammar errors and throws from theme formatters.
    let highlighted = crate::utils::uncaught::catch_panics(|| highlight(code, &options))
        .ok()?
        .ok()?;
    Some(highlighted.split('\n').collect())
}

/// Get a language identifier from a file path extension.
pub fn get_language_from_path(file_path: &JsString) -> Option<HighlightLanguage> {
    let extension = file_path.split('.').last()?.to_lowercase();
    if extension.is_empty() {
        return None;
    }
    // Extensions with unmatched surrogates name no language.
    let extension = extension.to_utf8().ok()?;
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
    Some(language.into())
}

struct PiMarkdownTheme;

impl MarkdownTheme for PiMarkdownTheme {
    fn heading(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.fg(ThemeColor::MdHeading, text))
    }

    fn link(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.fg(ThemeColor::MdLink, text))
    }

    fn link_url(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.fg(ThemeColor::MdLinkUrl, text))
    }

    fn code(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.fg(ThemeColor::MdCode, text))
    }

    fn code_block(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.fg(ThemeColor::MdCodeBlock, text))
    }

    fn code_block_border(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.fg(ThemeColor::MdCodeBlockBorder, text))
    }

    fn quote(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.fg(ThemeColor::MdQuote, text))
    }

    fn quote_border(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.fg(ThemeColor::MdQuoteBorder, text))
    }

    fn hr(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.fg(ThemeColor::MdHr, text))
    }

    fn list_bullet(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.fg(ThemeColor::MdListBullet, text))
    }

    fn bold(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.bold(text))
    }

    fn italic(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.italic(text))
    }

    fn strikethrough(&self, text: &JsString) -> JsString {
        Chalk::stdout().strikethrough(text)
    }

    fn underline(&self, text: &JsString) -> JsString {
        with_theme(|theme| theme.underline(text))
    }

    fn highlight_code(&self, code: &JsString, lang: Option<&JsString>) -> Option<Vec<JsString>> {
        let valid_lang = lang.filter(|language| supports_language(language));
        Some(match valid_lang {
            Some(language) => try_highlight_code(code, language).unwrap_or_else(|| styled_lines(code)),
            None => styled_lines(code),
        })
    }
}

pub fn get_markdown_theme() -> Box<dyn MarkdownTheme> {
    Box::new(PiMarkdownTheme)
}

pub fn get_select_list_theme() -> SelectListTheme {
    SelectListTheme {
        selected_prefix: Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Accent, text))),
        selected_text: Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Accent, text))),
        description: Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
        scroll_info: Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
        no_match: Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
    }
}

pub fn get_editor_theme() -> EditorTheme {
    EditorTheme {
        border_color: Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::BorderMuted, text))),
        prompt_prefix: Some(Rc::new(|text: &JsString| {
            with_theme(|theme| theme.fg(ThemeColor::Accent, text))
        })),
        select_list: get_select_list_theme(),
    }
}

pub fn get_settings_list_theme() -> SettingsListTheme {
    SettingsListTheme {
        label: Rc::new(|text: &JsString, selected: bool| {
            if selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, text))
            } else {
                text.clone()
            }
        }),
        value: Rc::new(|text: &JsString, selected: bool| {
            if selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, text))
            } else {
                with_theme(|theme| theme.fg(ThemeColor::Muted, text))
            }
        }),
        description: Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Dim, text))),
        cursor: with_theme(|theme| theme.fg(ThemeColor::Accent, "→ ")),
        hint: Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Dim, text))),
    }
}
