//! Port of `coding-agent/src/modes/interactive/interactive-mode.ts`.
//!
//! The TypeScript class is the composition root for the interactive TUI. This
//! module keeps the same structure: pure formatting helpers first, then the
//! mode itself. Component factories and the TUI event loop are Rust-native;
//! deviations are recorded in `docs/porting/traceability/interactive-mode.md`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use indexmap::IndexMap;
use pi_agent_core::agent::types::{AgentEvent, AgentMessage};
use pi_ai::types::{AssistantMessage, ImageContent, Model};
use pi_tui::EditorComponent;
use pi_tui::components::editor::EditorOptions;
use pi_tui::components::scroll_view::{
    ScrollFollow, ScrollOverscroll, ScrollView, ScrollViewOptions, ScrollViewScrollbar,
};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::stack::StackOptions;
use pi_tui::components::text::Text;
use pi_tui::components::v_stack::VStack;
use pi_tui::layout_node::StackLayoutEntry;
use pi_tui::terminal::Terminal;
use pi_tui::tui::{
    Component, ComponentRef, Container, FlowRenderRegion, Focusable, OverlayHandle, OverlayOptions, RenderRegion,
    TuiExt, TuiInputListenerResult, TuiMode, ViewportTui, component,
};
use pi_tui::{ProcessTerminal, TerminalRawModeError, TuiAltScreen, TuiAltScreenOptions, TuiMainScreen};
use regex::Regex;

use crate::core::agent_session::{AgentSession, AgentSessionEvent, CycleDirection, PromptOptions, StreamingBehavior};
use crate::core::agent_session_runtime::AgentSessionRuntime;
use crate::core::diagnostics::{ResourceDiagnostic, ResourceDiagnosticType};
use crate::core::extensions::types::{
    BoxFuture, ExtensionUiComponent, ExtensionUiComponentFactory, ExtensionUiContext, ExtensionUiCustomFactory,
    ExtensionUiCustomOptions, ExtensionUiDialogOptions, ExtensionUiDone, ExtensionUiFooterFactory, ExtensionUiTui,
    ExtensionUiTuiBackend, ExtensionWidgetOptions, NotifyType, TerminalInputHandler, ThemeInfo, ThemeSetResult,
    WidgetPlacement, WorkingIndicatorOptions,
};
use crate::core::footer_data_provider::FooterDataProvider;
use crate::core::keybindings::KeybindingsManager;
use crate::core::source_info::{SourceInfo, SourceScope};
use crate::modes::interactive::components::assistant_message::AssistantMessageComponent;
use crate::modes::interactive::components::bash_execution::{BashExecutionComponent, BashExecutionOptions};
use crate::modes::interactive::components::branch_summary_message::BranchSummaryMessageComponent;
use crate::modes::interactive::components::compaction_summary_message::CompactionSummaryMessageComponent;
use crate::modes::interactive::components::custom_editor::{AppActionHandler, CustomEditor, CustomEditorOptions};
use crate::modes::interactive::components::custom_message::CustomMessageComponent;
use crate::modes::interactive::components::extension_editor::{ExtensionEditorComponent, ExtensionEditorHost};
use crate::modes::interactive::components::extension_input::{ExtensionInputComponent, ExtensionInputOptions};
use crate::modes::interactive::components::extension_selector::{ExtensionSelectorComponent, ExtensionSelectorOptions};
use crate::modes::interactive::components::footer::FooterComponent;
use crate::modes::interactive::components::skill_invocation_message::SkillInvocationMessageComponent;
use crate::modes::interactive::components::thinking_selector::ThinkingSelectorComponent;
use crate::modes::interactive::components::tool_execution::{
    ToolExecutionComponent, ToolExecutionOptions, ToolExecutionResult,
};
use crate::modes::interactive::components::user_message::UserMessageComponent;
use crate::modes::interactive::interactive_root_compositor::{
    InteractiveRootCompositorOptions, create_interactive_root_compositor,
};
use crate::modes::interactive::theme::{Theme, ThemeBg, ThemeColor, with_theme};
use crate::utils::git::parse_git_url;
use crate::utils::paths::{get_cwd_relative_path, relative_path};
use serde_json::Value;

pub(crate) const PARTIAL_UPDATE_RENDER_THROTTLE_MS: u64 = 50;
pub(crate) const FOLLOW_UP_BODY_PREVIEW_LENGTH: usize = 50;

pub(crate) fn is_unknown_model(model: Option<&pi_ai::types::Model>) -> bool {
    model.is_some_and(|model| model.provider == "unknown" && model.id == "unknown" && model.api == "unknown")
}

/// Shell-quote a value the way the source resume hint does.
pub(crate) fn quote_if_needed(value: &str) -> String {
    let safe = !value.is_empty()
        && !value
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || "_-./~:@".contains(character)));
    if safe {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn is_skill_invocation(
    text: &str,
    prompt_templates: &[crate::core::prompt_templates::PromptTemplate],
) -> bool {
    if crate::parse_skill_block(text).is_some() || text.starts_with("/skill:") {
        return true;
    }
    crate::parse_skill_block(&crate::core::prompt_templates::expand_prompt_template(
        text,
        prompt_templates,
    ))
    .is_some()
}

/// `formatResumeCommand`: printable `pi --session ...` hint.
pub fn format_resume_command(session_manager: &crate::core::session_manager::SessionManager) -> Option<String> {
    use std::io::IsTerminal;

    if !std::io::stdout().is_terminal() {
        return None;
    }
    if !session_manager.is_persisted() {
        return None;
    }
    let session_file = session_manager.get_session_file()?;
    if !Path::new(session_file).exists() {
        return None;
    }

    let mut args = vec![crate::config::package_config().app_name.clone()];
    if !session_manager.uses_default_session_dir() {
        args.push("--session-dir".to_owned());
        args.push(quote_if_needed(session_manager.get_session_dir()));
    }
    args.push("--session".to_owned());
    args.push(session_manager.get_session_id().to_owned());
    Some(args.join(" "))
}

pub(crate) fn format_follow_up_preview(message: &str) -> String {
    let sender = agent_sender(message)
        .map(|sender| sender.trim().to_owned())
        .unwrap_or_else(|| {
            if message.trim() == "Continue working toward the active goal." {
                "goal".to_owned()
            } else {
                "unknown".to_owned()
            }
        });
    let body_marker = "\nMessage:\n";
    let body_text = message.split_once(body_marker).map(|(_, rest)| rest).unwrap_or(message);
    let body = body_text.split('\n').next().unwrap_or_default().trim();
    let body_preview = if body.chars().count() > FOLLOW_UP_BODY_PREVIEW_LENGTH {
        format!(
            "{}…",
            body.chars().take(FOLLOW_UP_BODY_PREVIEW_LENGTH).collect::<String>()
        )
    } else {
        body.to_owned()
    };
    format!("Follow-up from {sender}: {body_preview}")
}

fn agent_sender(message: &str) -> Option<String> {
    static AGENT_LINE: OnceLock<Regex> = OnceLock::new();
    let regex = AGENT_LINE.get_or_init(|| Regex::new(r"^- agent:\s*(.+)$").expect("valid agent-line regex"));
    for line in message.split('\n') {
        if let Some(captures) = regex.captures(line) {
            return captures.get(1).map(|capture| capture.as_str().to_owned());
        }
    }
    None
}

/// JS `Number.prototype.toLocaleString()` for token counts under the default
/// locale used by the source (`en-US` grouping).
pub(crate) fn to_locale_string(value: f64) -> String {
    if !value.is_finite() {
        return pi_js_compat::number::js_number_to_string(value);
    }
    let rounded = value.trunc();
    let negative = rounded < 0.0;
    let digits = format!("{:.0}", rounded.abs());
    let mut grouped = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(character);
    }
    if negative { format!("-{grouped}") } else { grouped }
}

pub(crate) fn format_session_token_info(tokens: &crate::core::agent_session::SessionTokenStats) -> String {
    let prompt_tokens = tokens.input + tokens.cache_read + tokens.cache_write;
    with_theme(|theme| {
        let mut info = format!("{}\n", theme.bold("Tokens"));
        info.push_str(&format!(
            "{} {}\n",
            theme.fg(ThemeColor::Dim, "Input:"),
            to_locale_string(prompt_tokens)
        ));
        if prompt_tokens > 0.0 && (tokens.cache_read > 0.0 || tokens.cache_write > 0.0) {
            let hit_rate = theme.fg(
                ThemeColor::Dim,
                &format!(
                    "({}%)",
                    pi_js_compat::number::to_fixed(tokens.cache_read / prompt_tokens * 100.0, 1)
                ),
            );
            info.push_str(&format!(
                "  {} {} {}\n",
                theme.fg(ThemeColor::Dim, "Cached:"),
                to_locale_string(tokens.cache_read),
                hit_rate
            ));
            let written = if tokens.cache_write > 0.0 {
                format!(
                    " {}",
                    theme.fg(
                        ThemeColor::Dim,
                        &format!("({} written to cache)", to_locale_string(tokens.cache_write))
                    )
                )
            } else {
                String::new()
            };
            info.push_str(&format!(
                "  {} {}{}\n",
                theme.fg(ThemeColor::Dim, "Uncached:"),
                to_locale_string(tokens.input + tokens.cache_write),
                written
            ));
        }
        info.push_str(&format!(
            "{} {}\n",
            theme.fg(ThemeColor::Dim, "Output:"),
            to_locale_string(tokens.output)
        ));
        info.push_str(&format!(
            "{} {}\n",
            theme.fg(ThemeColor::Dim, "Total:"),
            to_locale_string(tokens.total)
        ));
        info
    })
}

pub(crate) fn format_session_cost_info(
    cost: f64,
    usage_breakdown: &[crate::core::usage_totals::UsageCostBreakdownEntry],
    cache_waste: &crate::core::cache_stats::CacheWasteTotals,
) -> String {
    if cost <= 0.0 && cache_waste.missed_tokens <= 0.0 {
        return String::new();
    }
    with_theme(|theme| {
        let mut info = format!("\n{}\n", theme.bold("Cost"));
        info.push_str(&format!(
            "{} ${}",
            theme.fg(ThemeColor::Dim, "Total:"),
            pi_js_compat::number::to_fixed(cost, 3)
        ));
        if usage_breakdown.len() > 1 {
            for entry in usage_breakdown {
                info.push_str(&format!(
                    "\n  {} ${} {}",
                    theme.fg(ThemeColor::Dim, &format!("{}:", entry.key)),
                    pi_js_compat::number::to_fixed(entry.cost, 3),
                    theme.fg(
                        ThemeColor::Dim,
                        &format!(
                            "({} tokens)",
                            crate::modes::interactive::components::footer::format_tokens(entry.tokens)
                        )
                    )
                ));
            }
        }
        if cache_waste.missed_tokens <= 0.0 {
            return info;
        }
        let miss_label = if cache_waste.miss_count == 1 {
            "1 miss".to_owned()
        } else {
            format!("{} misses", cache_waste.miss_count)
        };
        let detail = format!("{} tokens, {miss_label}", to_locale_string(cache_waste.missed_tokens));
        if cache_waste.missed_cost >= 0.0001 {
            info.push_str(&format!(
                "\n{} ${} {}",
                theme.fg(ThemeColor::Dim, "Cache Re-billed:"),
                pi_js_compat::number::to_fixed(cache_waste.missed_cost, 3),
                theme.fg(ThemeColor::Dim, &format!("({detail})"))
            ));
        } else {
            info.push_str(&format!("\n{} {detail}", theme.fg(ThemeColor::Dim, "Cache Re-billed:")));
        }
        info
    })
}

// ============================================================================
// Loaded-resource formatting
// ============================================================================

fn package_path_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^(.*/node_modules)/(@?[^/]+(?:/[^/]+)?)$").expect("valid node_modules regex"))
}

fn node_modules_path_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"node_modules/(@?[^/]+(?:/[^/]+)?)/(.*)").expect("valid npm path regex"))
}

fn git_path_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"git/[^/]+/[^/]+/(.*)").expect("valid git path regex"))
}

fn home_directory() -> String {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME").unwrap_or_default()
    }
}

pub(crate) fn format_display_path(path: &str) -> String {
    let home = home_directory();
    if !home.is_empty() && path.starts_with(&home) {
        return format!("~{}", &path[home.len()..]);
    }
    path.to_owned()
}

pub(crate) fn format_extension_display_path(path: &str) -> String {
    let display = format_display_path(path);
    display
        .strip_suffix("/index.ts")
        .or_else(|| display.strip_suffix("/index.js"))
        .unwrap_or(&display)
        .to_owned()
}

pub(crate) fn format_context_path(path: &str, cwd: &str) -> String {
    let cwd = resolve_path_string(cwd);
    let absolute = if Path::new(path).is_absolute() {
        resolve_path_string(path)
    } else {
        resolve_path_string(&format!("{cwd}/{path}"))
    };
    if let Ok(Some(relative)) = get_cwd_relative_path(&absolute, &cwd) {
        return relative;
    }
    format_display_path(&absolute)
}

fn resolve_path_string(path: &str) -> String {
    std::path::absolute(Path::new(path))
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_owned())
}

fn normalize_slashes(path: &str) -> String {
    path.replace('\\', "/")
}

fn posix_relative(from: &str, to: &str) -> String {
    let from_parts: Vec<&str> = from
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    let to_parts: Vec<&str> = to.split('/').filter(|part| !part.is_empty() && *part != ".").collect();
    let common = from_parts
        .iter()
        .zip(&to_parts)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts: Vec<&str> = Vec::new();
    parts.extend(std::iter::repeat_n("..", from_parts.len() - common));
    parts.extend(&to_parts[common..]);
    parts.join("/")
}

pub(crate) fn get_short_path(full_path: &str, source_info: Option<&SourceInfo>) -> String {
    let normalized_full_path = normalize_slashes(full_path);
    if let Some(source_info) = source_info
        && let Some(base_dir) = source_info.base_dir.as_deref()
        && is_package_source(Some(source_info))
    {
        let normalized_base_dir = normalize_slashes(base_dir);
        if let Some(captures) = package_path_regex().captures(&normalized_base_dir) {
            let root = captures.get(1).map(|capture| capture.as_str()).unwrap_or_default();
            if !root.is_empty() && normalized_full_path.starts_with(&format!("{root}/")) {
                return posix_relative(&normalized_base_dir, &normalized_full_path);
            }
        }

        let relative = relative_path(&resolve_path_string(base_dir), &resolve_path_string(full_path));
        if !relative.is_empty()
            && relative != "."
            && !relative.starts_with("..")
            && !relative.starts_with(&format!("..{}", std::path::MAIN_SEPARATOR))
            && !Path::new(&relative).is_absolute()
        {
            return normalize_slashes(&relative);
        }
    }

    let source = source_info.map(|info| info.source.as_str()).unwrap_or("");
    if let Some(captures) = node_modules_path_regex().captures(&normalized_full_path)
        && source.starts_with("npm:")
        && let Some(path) = captures.get(2)
    {
        return path.as_str().to_owned();
    }
    if let Some(captures) = git_path_regex().captures(&normalized_full_path)
        && source.starts_with("git:")
        && let Some(path) = captures.get(1)
    {
        return path.as_str().to_owned();
    }
    format_display_path(full_path)
}

pub(crate) fn get_compact_path_label(resource_path: &str, source_info: Option<&SourceInfo>) -> String {
    let short_path = get_short_path(resource_path, source_info);
    let normalized_path = normalize_slashes(&short_path);
    let segments: Vec<&str> = normalized_path
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "~")
        .collect();
    if let Some(last) = segments.last() {
        return (*last).to_owned();
    }
    short_path
}

pub(crate) fn get_compact_package_source_label(source_info: Option<&SourceInfo>) -> String {
    let source = source_info.map(|info| info.source.as_str()).unwrap_or("");
    if let Some(stripped) = source.strip_prefix("npm:") {
        return if stripped.is_empty() {
            source.to_owned()
        } else {
            stripped.to_owned()
        };
    }
    if let Some(git_source) = parse_git_url(source) {
        return if git_source.path.is_empty() {
            source.to_owned()
        } else {
            git_source.path
        };
    }
    source.to_owned()
}

pub(crate) fn get_compact_extension_label(resource_path: &str, source_info: Option<&SourceInfo>) -> String {
    if !is_package_source(source_info) {
        return get_compact_path_label(resource_path, source_info);
    }
    let source_label = get_compact_package_source_label(source_info);
    if source_label.is_empty() {
        return get_compact_path_label(resource_path, source_info);
    }
    let short_path = normalize_slashes(&get_short_path(resource_path, source_info));
    let package_path = short_path.strip_prefix("extensions/").unwrap_or(&short_path);
    let parsed = parse_posix_path(package_path);
    if parsed.name == "index" {
        if parsed.dir.is_empty() || parsed.dir == "." {
            return source_label;
        }
        return format!("{source_label}:{}", parsed.dir);
    }
    format!("{source_label}:{package_path}")
}

struct ParsedPath {
    dir: String,
    name: String,
}

fn parse_posix_path(path: &str) -> ParsedPath {
    let (dir, base) = match path.rsplit_once('/') {
        Some((dir, base)) => (dir, base),
        None => ("", path),
    };
    let name = base.rsplit_once('.').map(|(name, _)| name).unwrap_or(base);
    ParsedPath {
        dir: dir.to_owned(),
        name: name.to_owned(),
    }
}

pub(crate) fn get_compact_display_path_segments(resource_path: &str) -> Vec<String> {
    normalize_slashes(&format_display_path(resource_path))
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "~")
        .map(str::to_owned)
        .collect()
}

fn compact_non_package_extension_label(resource_path: &str, index: usize, all_paths: &[Vec<String>]) -> String {
    let Some(segments) = all_paths.get(index) else {
        return get_compact_path_label(resource_path, None);
    };
    if segments.is_empty() {
        return get_compact_path_label(resource_path, None);
    }
    for segment_count in 1..=segments.len() {
        let start = segments.len() - segment_count;
        let candidate = segments[start..].join("/");
        let unique = all_paths.iter().enumerate().all(|(item_index, item)| {
            if item_index == index {
                return true;
            }
            let start = item.len().saturating_sub(segment_count);
            item[start..].join("/") != candidate
        });
        if unique {
            return candidate;
        }
    }
    segments.join("/")
}

pub(crate) fn get_compact_extension_labels(extensions: &[(String, Option<SourceInfo>)]) -> Vec<String> {
    let non_package: Vec<&(String, Option<SourceInfo>)> = extensions
        .iter()
        .filter(|(_, source_info)| !is_package_source(source_info.as_ref()))
        .collect();
    let non_package_segments: Vec<Vec<String>> = non_package
        .iter()
        .map(|(path, _)| {
            let mut segments = get_compact_display_path_segments(path);
            if segments.len() > 1
                && segments
                    .last()
                    .is_some_and(|last| last == "index.ts" || last == "index.js")
            {
                segments.pop();
            }
            segments
        })
        .collect();

    extensions
        .iter()
        .map(|(path, source_info)| {
            if is_package_source(source_info.as_ref()) {
                return get_compact_extension_label(path, source_info.as_ref());
            }
            match non_package.iter().position(|(candidate, _)| candidate == path) {
                Some(index) => compact_non_package_extension_label(path, index, &non_package_segments),
                None => get_compact_path_label(path, source_info.as_ref()),
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisplaySourceInfo {
    pub label: String,
    pub scope_label: Option<String>,
    pub color: ThemeColor,
}

pub(crate) fn get_display_source_info(source_info: Option<&SourceInfo>) -> DisplaySourceInfo {
    let source = source_info.map(|info| info.source.as_str()).unwrap_or("local");
    let scope = source_info.map(|info| info.scope).unwrap_or(SourceScope::Project);
    if source == "local" {
        return match scope {
            SourceScope::User => DisplaySourceInfo {
                label: "user".to_owned(),
                scope_label: None,
                color: ThemeColor::Muted,
            },
            SourceScope::Project => DisplaySourceInfo {
                label: "project".to_owned(),
                scope_label: None,
                color: ThemeColor::Muted,
            },
            SourceScope::Temporary => DisplaySourceInfo {
                label: "path".to_owned(),
                scope_label: Some("temp".to_owned()),
                color: ThemeColor::Muted,
            },
        };
    }
    if source == "cli" {
        return DisplaySourceInfo {
            label: "path".to_owned(),
            scope_label: (scope == SourceScope::Temporary).then(|| "temp".to_owned()),
            color: ThemeColor::Muted,
        };
    }
    let scope_label = match scope {
        SourceScope::User => Some("user".to_owned()),
        SourceScope::Project => Some("project".to_owned()),
        SourceScope::Temporary => Some("temp".to_owned()),
    };
    DisplaySourceInfo {
        label: source.to_owned(),
        scope_label,
        color: ThemeColor::Accent,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeGroupKind {
    User,
    Project,
    Path,
}

impl ScopeGroupKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
            Self::Path => "path",
        }
    }
}

pub(crate) fn get_scope_group(source_info: Option<&SourceInfo>) -> ScopeGroupKind {
    let source = source_info.map(|info| info.source.as_str()).unwrap_or("local");
    let scope = source_info.map(|info| info.scope).unwrap_or(SourceScope::Project);
    if source == "cli" || scope == SourceScope::Temporary {
        return ScopeGroupKind::Path;
    }
    match scope {
        SourceScope::User => ScopeGroupKind::User,
        SourceScope::Project => ScopeGroupKind::Project,
        SourceScope::Temporary => ScopeGroupKind::Path,
    }
}

pub(crate) fn is_package_source(source_info: Option<&SourceInfo>) -> bool {
    source_info
        .map(|info| info.source.as_str())
        .unwrap_or("")
        .starts_with("npm:")
        || source_info
            .map(|info| info.source.as_str())
            .unwrap_or("")
            .starts_with("git:")
}

#[derive(Debug, Clone)]
pub(crate) struct ScopeGroupItem {
    pub path: String,
    pub source_info: Option<SourceInfo>,
}

#[derive(Debug, Clone)]
pub(crate) struct ScopeGroup {
    pub scope: ScopeGroupKind,
    pub paths: Vec<ScopeGroupItem>,
    pub packages: IndexMap<String, Vec<ScopeGroupItem>>,
}

pub(crate) fn build_scope_groups(items: &[ScopeGroupItem]) -> Vec<ScopeGroup> {
    let mut user = ScopeGroup {
        scope: ScopeGroupKind::User,
        paths: Vec::new(),
        packages: IndexMap::new(),
    };
    let mut project = ScopeGroup {
        scope: ScopeGroupKind::Project,
        paths: Vec::new(),
        packages: IndexMap::new(),
    };
    let mut path = ScopeGroup {
        scope: ScopeGroupKind::Path,
        paths: Vec::new(),
        packages: IndexMap::new(),
    };
    for item in items {
        let group = match get_scope_group(item.source_info.as_ref()) {
            ScopeGroupKind::User => &mut user,
            ScopeGroupKind::Project => &mut project,
            ScopeGroupKind::Path => &mut path,
        };
        let source = item
            .source_info
            .as_ref()
            .map(|info| info.source.clone())
            .unwrap_or_else(|| "local".to_owned());
        if is_package_source(item.source_info.as_ref()) {
            group.packages.entry(source).or_default().push(item.clone());
        } else {
            group.paths.push(item.clone());
        }
    }
    [project, user, path]
        .into_iter()
        .filter(|group| !group.paths.is_empty() || !group.packages.is_empty())
        .collect()
}

pub(crate) fn format_scope_groups(
    groups: &[ScopeGroup],
    format_path: impl Fn(&ScopeGroupItem) -> String,
    format_package_path: impl Fn(&ScopeGroupItem, &str) -> String,
) -> String {
    with_theme(|theme| {
        let mut lines: Vec<String> = Vec::new();
        for group in groups {
            lines.push(format!("  {}", theme.fg(ThemeColor::Accent, group.scope.as_str())));

            let mut sorted_paths: Vec<&ScopeGroupItem> = group.paths.iter().collect();
            sorted_paths.sort_by(|left, right| pi_js_compat::collator::locale_compare(&left.path, &right.path));
            for item in sorted_paths {
                lines.push(theme.fg(ThemeColor::Dim, &format!("    {}", format_path(item))));
            }

            let mut sorted_packages: Vec<(&String, &Vec<ScopeGroupItem>)> = group.packages.iter().collect();
            sorted_packages.sort_by(|(left, _), (right, _)| pi_js_compat::collator::locale_compare(left, right));
            for (source, items) in sorted_packages {
                lines.push(format!("    {}", theme.fg(ThemeColor::MdLink, source)));
                let mut sorted_items: Vec<&ScopeGroupItem> = items.iter().collect();
                sorted_items.sort_by(|left, right| pi_js_compat::collator::locale_compare(&left.path, &right.path));
                for item in sorted_items {
                    lines.push(theme.fg(ThemeColor::Dim, &format!("      {}", format_package_path(item, source))));
                }
            }
        }
        lines.join("\n")
    })
}

pub(crate) fn find_source_info_for_path<'a>(
    path: &str,
    source_infos: &'a HashMap<String, SourceInfo>,
) -> Option<&'a SourceInfo> {
    if let Some(exact) = source_infos.get(path) {
        return Some(exact);
    }
    let mut current = path;
    while let Some(index) = current.rfind('/') {
        current = &current[..index];
        if let Some(parent) = source_infos.get(current) {
            return Some(parent);
        }
    }
    None
}

pub(crate) fn format_path_with_source(path: &str, source_info: Option<&SourceInfo>) -> String {
    if let Some(source_info) = source_info {
        let short_path = get_short_path(path, Some(source_info));
        let display = get_display_source_info(Some(source_info));
        let label_text = match display.scope_label {
            Some(scope_label) => format!("{} ({scope_label})", display.label),
            None => display.label,
        };
        return format!("{label_text} {short_path}");
    }
    format_display_path(path)
}

pub(crate) fn format_diagnostics(
    diagnostics: &[ResourceDiagnostic],
    source_infos: &HashMap<String, SourceInfo>,
) -> String {
    with_theme(|theme| {
        let mut lines: Vec<String> = Vec::new();
        let mut collisions: IndexMap<String, Vec<&ResourceDiagnostic>> = IndexMap::new();
        let mut other: Vec<&ResourceDiagnostic> = Vec::new();
        for diagnostic in diagnostics {
            if diagnostic.diagnostic_type == ResourceDiagnosticType::Collision
                && let Some(collision) = &diagnostic.collision
            {
                collisions.entry(collision.name.clone()).or_default().push(diagnostic);
            } else {
                other.push(diagnostic);
            }
        }

        for (name, collision_list) in &collisions {
            let Some(first) = collision_list
                .first()
                .and_then(|diagnostic| diagnostic.collision.as_ref())
            else {
                continue;
            };
            lines.push(theme.fg(ThemeColor::Warning, &format!("  \"{name}\" collision:")));
            lines.push(theme.fg(
                ThemeColor::Dim,
                &format!(
                    "    {} {}",
                    theme.fg(ThemeColor::Success, "✓"),
                    format_path_with_source(
                        &first.winner_path,
                        find_source_info_for_path(&first.winner_path, source_infos)
                    )
                ),
            ));
            for diagnostic in collision_list {
                if let Some(collision) = &diagnostic.collision {
                    lines.push(theme.fg(
                        ThemeColor::Dim,
                        &format!(
                            "    {} {} (skipped)",
                            theme.fg(ThemeColor::Warning, "✗"),
                            format_path_with_source(
                                &collision.loser_path,
                                find_source_info_for_path(&collision.loser_path, source_infos)
                            )
                        ),
                    ));
                }
            }
        }

        for diagnostic in other {
            let color = if diagnostic.diagnostic_type == ResourceDiagnosticType::Error {
                ThemeColor::Error
            } else {
                ThemeColor::Warning
            };
            match &diagnostic.path {
                Some(path) => {
                    lines.push(theme.fg(
                        color,
                        &format!(
                            "  {}",
                            format_path_with_source(path, find_source_info_for_path(path, source_infos))
                        ),
                    ));
                    lines.push(theme.fg(color, &format!("    {}", diagnostic.message)));
                }
                None => lines.push(theme.fg(color, &format!("  {}", diagnostic.message))),
            }
        }

        lines.join("\n")
    })
}

pub(crate) fn format_compact_list(items: &[String], sort: bool) -> String {
    let mut labels: Vec<String> = items
        .iter()
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect();
    if sort {
        labels.sort_by(|left, right| pi_js_compat::collator::locale_compare(left, right));
    }
    with_theme(|theme| theme.fg(ThemeColor::Dim, &format!("  {}", labels.join(", "))))
}

pub(crate) fn section_header(name: &str, color: ThemeColor) -> String {
    with_theme(|theme| theme.fg(color, &format!("[{name}]")))
}

/// `ExpandableText`: text whose content depends on the expansion state.
pub struct ExpandableText {
    collapsed_text: String,
    expanded_text: String,
    expanded: bool,
    padding_x: usize,
    padding_y: usize,
    text: pi_tui::components::text::Text,
}

impl ExpandableText {
    pub fn new(
        collapsed_text: impl Fn() -> String + 'static,
        expanded_text: impl Fn() -> String + 'static,
        expanded: bool,
        padding_x: usize,
        padding_y: usize,
    ) -> Self {
        let collapsed = collapsed_text();
        let expanded_text_value = expanded_text();
        let text = pi_tui::components::text::Text::new(
            if expanded {
                expanded_text_value.clone()
            } else {
                collapsed.clone()
            },
            padding_x,
            padding_y,
        );
        Self {
            collapsed_text: collapsed,
            expanded_text: expanded_text_value,
            expanded,
            padding_x,
            padding_y,
            text,
        }
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
        self.text.set_text(if expanded {
            self.expanded_text.clone()
        } else {
            self.collapsed_text.clone()
        });
    }

    pub fn expanded(&self) -> bool {
        self.expanded
    }

    pub fn padding(&self) -> (usize, usize) {
        (self.padding_x, self.padding_y)
    }
}

impl pi_tui::tui::Component for ExpandableText {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.text.render(width)
    }

    fn invalidate(&mut self) {
        pi_tui::tui::Component::invalidate(&mut self.text);
    }
}

// ============================================================================
// Interactive TUI handle
// ============================================================================

/// Active terminal renderer (`createInteractiveTui`).
pub enum Renderer {
    Main(Rc<TuiMainScreen>),
    Fullscreen(Rc<TuiAltScreen>),
}

impl Renderer {
    pub fn mode(&self) -> TuiMode {
        match self {
            Self::Main(_) => TuiMode::Regular,
            Self::Fullscreen(_) => TuiMode::Fullscreen,
        }
    }

    pub fn terminal(&self) -> Rc<RefCell<dyn Terminal>> {
        match self {
            Self::Main(tui) => TuiExt::terminal(tui.as_ref()),
            Self::Fullscreen(tui) => TuiExt::terminal(tui.as_ref()),
        }
    }

    pub fn start(&self) -> Result<(), TerminalRawModeError> {
        match self {
            Self::Main(tui) => TuiExt::start(tui.as_ref()),
            Self::Fullscreen(tui) => TuiExt::start(tui.as_ref()),
        }
    }

    pub fn stop(&self) {
        match self {
            Self::Main(tui) => TuiExt::stop(tui.as_ref()),
            Self::Fullscreen(tui) => TuiExt::stop(tui.as_ref()),
        }
    }
}

/// `InteractiveTuiOptions`.
pub struct InteractiveTuiOptions {
    pub tui_mode: TuiMode,
    pub show_hardware_cursor: bool,
    pub log_directory: String,
    pub terminal: Option<Rc<RefCell<dyn Terminal>>>,
    pub on_right_click_paste: Option<Rc<dyn Fn()>>,
}

/// Composition root for selecting the interactive terminal renderer.
pub fn create_interactive_tui(options: InteractiveTuiOptions) -> Renderer {
    let terminal = options
        .terminal
        .unwrap_or_else(|| Rc::new(RefCell::new(ProcessTerminal::new())));
    if options.tui_mode == TuiMode::Fullscreen {
        let style_search_match = Rc::new(|text: &str| {
            with_theme(|theme| theme.bg(ThemeBg::SearchMatchBg, &theme.fg(ThemeColor::SearchMatchText, text)))
        });
        let underline_style = Rc::clone(&style_search_match);
        let current_style = Rc::clone(&style_search_match);
        return Renderer::Fullscreen(TuiAltScreen::with_options(
            terminal,
            Some(options.show_hardware_cursor),
            Some(PathBuf::from(&options.log_directory)),
            TuiAltScreenOptions {
                search_match_style: Some(Rc::new(move |text: &str| {
                    with_theme(|theme| theme.underline(&underline_style(text)))
                })),
                search_current_match_style: Some(Rc::new(move |text: &str| {
                    with_theme(|theme| theme.bold(&theme.inverse(&current_style(text))))
                })),
                open_url: Some(Rc::new(|url: &str| {
                    crate::utils::open_browser::open_browser(url);
                })),
                on_right_click_paste: options.on_right_click_paste,
                ..TuiAltScreenOptions::default()
            },
        ));
    }
    Renderer::Main(TuiMainScreen::with_options(
        terminal,
        Some(options.show_hardware_cursor),
        Some(PathBuf::from(&options.log_directory)),
    ))
}

struct NoopFixedCell;

impl pi_tui::components::editor::EditorFixedCell for NoopFixedCell {
    fn place(&self, _row: i64, _col: i64) {}
    fn clear(&self) {}
    fn update(&self, _value: &str) -> bool {
        false
    }
}

/// Stable TUI handle used by components while the mode replaces the renderer.
pub struct InteractiveTuiHandle {
    renderer: RefCell<Renderer>,
}

impl InteractiveTuiHandle {
    pub fn new(renderer: Renderer) -> Rc<Self> {
        Rc::new(Self {
            renderer: RefCell::new(renderer),
        })
    }

    pub fn replace_renderer(&self, renderer: Renderer) {
        *self.renderer.borrow_mut() = renderer;
    }

    pub fn mode(&self) -> TuiMode {
        self.renderer.borrow().mode()
    }

    pub fn terminal(&self) -> Rc<RefCell<dyn Terminal>> {
        self.renderer.borrow().terminal()
    }

    pub fn add_child(&self, child: ComponentRef) {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::add_child(tui.as_ref(), child),
            Renderer::Fullscreen(tui) => TuiExt::add_child(tui.as_ref(), child),
        }
    }

    pub fn clear(&self) {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::clear(tui.as_ref()),
            Renderer::Fullscreen(tui) => TuiExt::clear(tui.as_ref()),
        }
    }

    pub fn set_focus(&self, component: Option<ComponentRef>) {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::set_focus(tui.as_ref(), component),
            Renderer::Fullscreen(tui) => TuiExt::set_focus(tui.as_ref(), component),
        }
    }

    pub fn get_focused_component(&self) -> Option<ComponentRef> {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::get_focused_component(tui.as_ref()),
            Renderer::Fullscreen(tui) => TuiExt::get_focused_component(tui.as_ref()),
        }
    }

    pub fn request_render(&self) {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::request_render(tui.as_ref(), false),
            Renderer::Fullscreen(tui) => TuiExt::request_render(tui.as_ref(), false),
        }
    }

    pub fn render_now(&self) {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::render_now(tui.as_ref(), true),
            Renderer::Fullscreen(tui) => TuiExt::render_now(tui.as_ref(), true),
        }
    }

    pub fn invalidate(&self) {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::invalidate(tui.as_ref()),
            Renderer::Fullscreen(tui) => TuiExt::invalidate(tui.as_ref()),
        }
    }

    pub fn start(&self) -> Result<(), TerminalRawModeError> {
        self.renderer.borrow().start()
    }

    pub fn stop(&self) {
        self.renderer.borrow().stop();
    }

    pub fn process_events(&self) {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::process_events(tui.as_ref()),
            Renderer::Fullscreen(tui) => TuiExt::process_events(tui.as_ref()),
        }
    }

    pub fn get_show_hardware_cursor(&self) -> bool {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::get_show_hardware_cursor(tui.as_ref()),
            Renderer::Fullscreen(tui) => TuiExt::get_show_hardware_cursor(tui.as_ref()),
        }
    }

    pub fn get_clear_on_shrink(&self) -> bool {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::get_clear_on_shrink(tui.as_ref()),
            Renderer::Fullscreen(tui) => TuiExt::get_clear_on_shrink(tui.as_ref()),
        }
    }

    pub fn set_clear_on_shrink(&self, enabled: bool) {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::set_clear_on_shrink(tui.as_ref(), enabled),
            Renderer::Fullscreen(tui) => TuiExt::set_clear_on_shrink(tui.as_ref(), enabled),
        }
    }

    pub fn show_overlay(&self, component: ComponentRef, options: Option<OverlayOptions>) -> OverlayHandle {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::show_overlay(tui.as_ref(), component, options),
            Renderer::Fullscreen(tui) => TuiExt::show_overlay(tui.as_ref(), component, options),
        }
    }

    pub fn hide_overlay(&self) {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::hide_overlay(tui.as_ref()),
            Renderer::Fullscreen(tui) => TuiExt::hide_overlay(tui.as_ref()),
        }
    }

    pub fn has_overlay_entries(&self) -> bool {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => tui.base().has_overlay_entries(),
            Renderer::Fullscreen(tui) => tui.base().has_overlay_entries(),
        }
    }

    pub fn add_input_listener(&self, listener: Rc<dyn Fn(&str) -> TuiInputListenerResult>) -> usize {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::add_input_listener(tui.as_ref(), listener),
            Renderer::Fullscreen(tui) => TuiExt::add_input_listener(tui.as_ref(), listener),
        }
    }

    pub fn remove_input_listener(&self, id: usize) {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::remove_input_listener(tui.as_ref(), id),
            Renderer::Fullscreen(tui) => TuiExt::remove_input_listener(tui.as_ref(), id),
        }
    }

    pub fn set_on_debug(&self, handler: Rc<dyn Fn()>) {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::set_on_debug(tui.as_ref(), handler),
            Renderer::Fullscreen(tui) => TuiExt::set_on_debug(tui.as_ref(), handler),
        }
    }

    pub fn set_layout_root(&self, component: Option<ComponentRef>) {
        if let Renderer::Fullscreen(tui) = &*self.renderer.borrow() {
            ViewportTui::set_layout_root(tui.as_ref(), component);
        }
    }

    pub fn create_flow_render_region(&self, component: ComponentRef) -> Option<FlowRenderRegion> {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => Some(TuiMainScreen::create_flow_render_region(tui, component)),
            Renderer::Fullscreen(_) => None,
        }
    }

    pub fn create_render_region(&self, component: ComponentRef) -> Option<RenderRegion> {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => Some(TuiMainScreen::create_render_region(tui, component)),
            Renderer::Fullscreen(_) => None,
        }
    }
}

/// Local host wrapper because `EditorHost` cannot be implemented for `Rc<T>`.
pub struct InteractiveTuiHost(pub Rc<InteractiveTuiHandle>);

impl pi_tui::components::editor::EditorHost for InteractiveTuiHost {
    fn terminal_rows(&self) -> usize {
        self.0.terminal().borrow().rows()
    }

    fn request_render(&self) {
        InteractiveTuiHandle::request_render(self.0.as_ref());
    }

    fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        match &*self.0.renderer.borrow() {
            Renderer::Main(tui) => Rc::new(TuiMainScreen::create_fixed_cell(tui)),
            Renderer::Fullscreen(_) => Rc::new(NoopFixedCell),
        }
    }
}

impl ExtensionEditorHost for InteractiveTuiHost {
    fn suspend(&self) {
        self.0.stop();
    }

    fn resume(&self) {
        let _ = self.0.start();
    }

    fn request_forced_render(&self) {
        match &*self.0.renderer.borrow() {
            Renderer::Main(tui) => TuiExt::request_render(tui.as_ref(), true),
            Renderer::Fullscreen(tui) => TuiExt::request_render(tui.as_ref(), true),
        }
    }
}

/// Component adapter so the mode can hold a typed container and mount it.
pub struct SharedContainer(pub Rc<RefCell<Container>>);

impl SharedContainer {
    pub fn create() -> (Rc<RefCell<Container>>, ComponentRef) {
        let container = Rc::new(RefCell::new(Container::new()));
        let component = component(SharedContainer(Rc::clone(&container)));
        (container, component)
    }
}

impl Component for SharedContainer {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.0.borrow_mut().render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.0.borrow_mut().invalidate();
    }
}

/// Adapter exposing [`CustomEditor`] through both component contracts.
pub struct EditorHandle(pub Rc<RefCell<CustomEditor>>);

impl Component for EditorHandle {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.0.borrow_mut().render(width)
    }

    fn handle_input(&mut self, data: &str) {
        self.0.borrow_mut().handle_input(data);
    }

    fn wants_input(&self) -> bool {
        true
    }

    fn invalidate(&mut self) {
        self.0.borrow_mut().invalidate();
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        None
    }
}

/// Callback receiving submitted main-loop input.
pub type InputCallback = Rc<dyn Fn(String)>;

/// `InteractiveModeOptions`.
#[derive(Default)]
pub struct InteractiveModeOptions {
    pub migrated_providers: Vec<String>,
    pub model_fallback_message: Option<String>,
    pub auto_trust_on_reload_cwd: Option<String>,
    pub initial_message: Option<String>,
    pub initial_images: Vec<ImageContent>,
    pub initial_messages: Vec<String>,
    pub verbose: bool,
    pub tui_mode: Option<TuiMode>,
    pub control_db_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompactionQueuedMessage {
    text: String,
    mode: &'static str,
}

/// Async operations queued by synchronous TUI callbacks.
enum ModeAction {
    Submit(String),
    Shutdown,
    ExternalEditor,
    FollowUp,
    Dequeue,
    CycleModel(CycleDirection),
    NewSession,
    Copy { flash_confirmation: bool },
}

/// Shared, thread-safe counters used by the extension UI bridge.
pub struct UiShared {
    rows: std::sync::atomic::AtomicUsize,
    columns: std::sync::atomic::AtomicUsize,
    fullscreen: std::sync::atomic::AtomicBool,
}

impl UiShared {
    fn new() -> Self {
        Self {
            rows: std::sync::atomic::AtomicUsize::new(24),
            columns: std::sync::atomic::AtomicUsize::new(80),
            fullscreen: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

/// Requests crossing from extension threads into the TUI thread.
enum UiRequest {
    Select {
        title: String,
        options: Vec<String>,
        timeout: Option<f64>,
        respond: tokio::sync::oneshot::Sender<Option<String>>,
    },
    Confirm {
        title: String,
        message: String,
        timeout: Option<f64>,
        respond: tokio::sync::oneshot::Sender<bool>,
    },
    Input {
        title: String,
        placeholder: Option<String>,
        timeout: Option<f64>,
        secret: bool,
        respond: tokio::sync::oneshot::Sender<Option<String>>,
    },
    Editor {
        title: String,
        prefill: Option<String>,
        respond: tokio::sync::oneshot::Sender<Option<String>>,
    },
    Notify {
        message: String,
        notify_type: Option<NotifyType>,
    },
    RequestRender,
    SetStatus {
        key: String,
        text: Option<String>,
    },
    SetWorkingMessage(Option<String>),
    SetWorkingVisible(bool),
    SetWorkingIndicator(Option<WorkingIndicatorOptions>),
    SetHiddenThinkingLabel(Option<String>),
    SetWidget {
        key: String,
        content: Option<Vec<String>>,
        placement: WidgetPlacement,
    },
    SetTitle(String),
    PasteToEditor(String),
    SetEditorText(String),
    SetToolsExpanded(bool),
    SetFooter(Option<ExtensionUiFooterFactory>),
    SetDefaultFooter(Option<ExtensionUiFooterFactory>),
    SetHeader(Option<ExtensionUiComponentFactory>),
    Custom {
        factory: ExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
        respond: tokio::sync::oneshot::Sender<Option<Value>>,
    },
    AddTerminalInput {
        handler: TerminalInputHandler,
        respond: std::sync::mpsc::Sender<usize>,
    },
    RemoveTerminalInput(usize),
    ShowApprovalSelector,
    ShowSandboxSelector,
    RuntimeNewSession {
        options: Option<crate::core::extensions::types::NewSessionOptions>,
        respond: tokio::sync::oneshot::Sender<Result<crate::core::extensions::types::SessionTransitionResult, String>>,
    },
    RuntimeFork {
        entry_id: String,
        options: Option<crate::core::extensions::types::ForkOptions>,
        respond: tokio::sync::oneshot::Sender<Result<crate::core::extensions::types::SessionTransitionResult, String>>,
    },
    RuntimeNavigateTree {
        entry_id: String,
        options: Option<crate::core::extensions::types::NavigateTreeOptions>,
        respond: tokio::sync::oneshot::Sender<Result<crate::core::extensions::types::SessionTransitionResult, String>>,
    },
    RuntimeSwitchSession {
        path: String,
        respond: tokio::sync::oneshot::Sender<Result<crate::core::extensions::types::SessionTransitionResult, String>>,
    },
    RuntimeReload {
        respond: tokio::sync::oneshot::Sender<()>,
    },
    RuntimeRestart {
        options: Option<crate::core::extensions::types::RestartOptions>,
        respond: tokio::sync::oneshot::Sender<()>,
    },
    AbortAndRestoreQueued,
    ShutdownRequested,
}

thread_local! {
    static MODE_REGISTRY: std::cell::RefCell<HashMap<usize, std::rc::Weak<InteractiveMode>>> =
        std::cell::RefCell::new(HashMap::new());
    static NEXT_MODE_ID: std::cell::Cell<usize> = const { std::cell::Cell::new(1) };
}

fn register_mode(mode: &Rc<InteractiveMode>) -> usize {
    NEXT_MODE_ID.with(|next| {
        let id = next.get();
        next.set(id + 1);
        MODE_REGISTRY.with(|registry| {
            registry.borrow_mut().insert(id, Rc::downgrade(mode));
        });
        id
    })
}

fn with_mode<R>(id: usize, f: impl FnOnce(Rc<InteractiveMode>) -> R) -> Option<R> {
    MODE_REGISTRY.with(|registry| {
        let mode = registry.borrow().get(&id).and_then(std::rc::Weak::upgrade)?;
        Some(f(mode))
    })
}

/// TUI-thread implementation of `ExtensionUiContext`.
struct InteractiveUiContext {
    id: usize,
    sender: std::sync::mpsc::Sender<UiRequest>,
}

impl InteractiveUiContext {
    fn send(&self, request: UiRequest) {
        if self.sender.send(request).is_err() {
            // The TUI thread is gone; extension requests become no-ops, matching
            // the source's noOpUIContext after shutdown.
        }
    }
}

impl ExtensionUiContext for InteractiveUiContext {
    fn select<'a>(
        &'a self,
        title: &str,
        options: &[String],
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<String>, String>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Select {
            title: title.to_owned(),
            options: options.to_vec(),
            timeout: opts.and_then(|opts| opts.timeout),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| error.to_string()) })
    }

    fn confirm<'a>(
        &'a self,
        title: &str,
        message: &str,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, String>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Confirm {
            title: title.to_owned(),
            message: message.to_owned(),
            timeout: opts.and_then(|opts| opts.timeout),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| error.to_string()) })
    }

    fn input<'a>(
        &'a self,
        title: &str,
        placeholder: Option<&str>,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<String>, String>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Input {
            title: title.to_owned(),
            placeholder: placeholder.map(str::to_owned),
            timeout: opts.and_then(|opts| opts.timeout),
            secret: opts.and_then(|opts| opts.secret).unwrap_or(false),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| error.to_string()) })
    }

    fn notify(&self, message: &str, notify_type: Option<NotifyType>) {
        self.send(UiRequest::Notify {
            message: message.to_owned(),
            notify_type,
        });
    }

    fn request_render(&self) {
        self.send(UiRequest::RequestRender);
    }

    fn on_terminal_input(&self, handler: TerminalInputHandler) -> Box<dyn Fn() + Send + Sync> {
        let (respond, receiver) = std::sync::mpsc::channel();
        self.send(UiRequest::AddTerminalInput { handler, respond });
        let id = receiver.recv().unwrap_or_default();
        let sender = self.sender.clone();
        Box::new(move || {
            let _ = sender.send(UiRequest::RemoveTerminalInput(id));
        })
    }

    fn set_status(&self, key: &str, text: Option<&str>) {
        self.send(UiRequest::SetStatus {
            key: key.to_owned(),
            text: text.map(str::to_owned),
        });
    }

    fn set_working_message(&self, message: Option<&str>) {
        self.send(UiRequest::SetWorkingMessage(message.map(str::to_owned)));
    }

    fn set_working_visible(&self, visible: bool) {
        self.send(UiRequest::SetWorkingVisible(visible));
    }

    fn set_working_indicator(&self, options: Option<&WorkingIndicatorOptions>) {
        self.send(UiRequest::SetWorkingIndicator(options.cloned()));
    }

    fn set_hidden_thinking_label(&self, label: Option<&str>) {
        self.send(UiRequest::SetHiddenThinkingLabel(label.map(str::to_owned)));
    }

    fn set_widget(&self, key: &str, content: Option<&[String]>, options: Option<&ExtensionWidgetOptions>) {
        self.send(UiRequest::SetWidget {
            key: key.to_owned(),
            content: content.map(<[String]>::to_vec),
            placement: options
                .and_then(|options| options.placement)
                .unwrap_or(WidgetPlacement::AboveEditor),
        });
    }

    fn set_widget_component(
        &self,
        key: &str,
        content: Option<ExtensionUiComponentFactory>,
        _options: Option<&ExtensionWidgetOptions>,
    ) {
        if let Some(factory) = content {
            let (respond, receiver) = tokio::sync::oneshot::channel();
            self.send(UiRequest::Custom {
                factory: Arc::new(move |context, done| {
                    done.call(None);
                    factory(context)
                }),
                options: Some(ExtensionUiCustomOptions {
                    overlay: Some(false),
                    ..ExtensionUiCustomOptions::default()
                }),
                respond,
            });
            drop(receiver);
            // The widget key/placement are handled by the interactive adapter
            // in a later slice; the factory is still mounted as a focused view.
            self.set_status(key, Some("component widget"));
        }
    }

    fn set_footer(&self, factory: Option<ExtensionUiFooterFactory>) {
        self.send(UiRequest::SetFooter(factory));
    }

    fn set_default_footer(&self, factory: Option<ExtensionUiFooterFactory>) {
        self.send(UiRequest::SetDefaultFooter(factory));
    }

    fn set_header(&self, factory: Option<ExtensionUiComponentFactory>) {
        self.send(UiRequest::SetHeader(factory));
    }

    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<Value>, String>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Custom {
            factory,
            options,
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| error.to_string()) })
    }

    fn set_title(&self, title: &str) {
        self.send(UiRequest::SetTitle(title.to_owned()));
    }

    fn paste_to_editor(&self, text: &str) {
        self.send(UiRequest::PasteToEditor(text.to_owned()));
    }

    fn set_editor_text(&self, text: &str) {
        self.send(UiRequest::SetEditorText(text.to_owned()));
    }

    fn get_editor_text(&self) -> String {
        with_mode(self.id, |mode| mode.editor.borrow().get_expanded_text()).unwrap_or_default()
    }

    fn editor<'a>(&'a self, title: &str, prefill: Option<&str>) -> BoxFuture<'a, Result<Option<String>, String>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Editor {
            title: title.to_owned(),
            prefill: prefill.map(str::to_owned),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| error.to_string()) })
    }

    fn get_all_themes(&self) -> Vec<ThemeInfo> {
        crate::modes::interactive::theme::get_available_themes_with_paths()
            .into_iter()
            .map(|info| crate::core::extensions::types::ThemeInfo {
                name: info.name,
                path: info.path.map(|path| path.display().to_string()),
            })
            .collect()
    }

    fn get_theme(&self, name: &str) -> Option<Arc<Theme>> {
        crate::modes::interactive::theme::get_theme_by_name(name)
    }

    fn set_theme(&self, name: &str) -> ThemeSetResult {
        with_mode(self.id, |mode| mode.set_theme_name(name)).unwrap_or(ThemeSetResult {
            success: false,
            error: Some("UI not available".to_owned()),
        })
    }

    fn get_tools_expanded(&self) -> bool {
        with_mode(self.id, |mode| mode.tool_output_expanded.get()).unwrap_or(false)
    }

    fn set_tools_expanded(&self, expanded: bool) {
        self.send(UiRequest::SetToolsExpanded(expanded));
    }

    fn theme(&self) -> Arc<Theme> {
        crate::modes::interactive::theme::current_theme()
    }
}

/// Thread-safe TUI handle passed to extension component factories.
struct InteractiveUiTuiBackend {
    shared: Arc<UiShared>,
    sender: std::sync::mpsc::Sender<UiRequest>,
}

impl ExtensionUiTuiBackend for InteractiveUiTuiBackend {
    fn request_render(&self) {
        let _ = self.sender.send(UiRequest::RequestRender);
    }

    fn terminal_rows(&self) -> usize {
        self.shared.rows.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn terminal_columns(&self) -> usize {
        self.shared.columns.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn mode(&self) -> TuiMode {
        if self.shared.fullscreen.load(std::sync::atomic::Ordering::Relaxed) {
            TuiMode::Fullscreen
        } else {
            TuiMode::Regular
        }
    }
}

/// Adapts a thread-safe extension component to the `pi-tui` component contract.
struct ExtensionComponentAdapter {
    inner: Box<dyn ExtensionUiComponent>,
}

impl Component for ExtensionComponentAdapter {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.inner.render(width)
    }

    fn handle_input(&mut self, data: &str) {
        self.inner.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.inner.invalidate();
    }

    fn wants_input(&self) -> bool {
        self.inner.wants_input()
    }
}

// ============================================================================
// InteractiveMode
// ============================================================================

/// `InteractiveMode`: TUI rendering and user interaction for the coding agent.
pub struct InteractiveMode {
    runtime: tokio::sync::Mutex<AgentSessionRuntime>,
    session: RefCell<AgentSession>,
    options: InteractiveModeOptions,
    ui: Rc<InteractiveTuiHandle>,
    header_container: Rc<RefCell<Container>>,
    loaded_resources_container: Rc<RefCell<Container>>,
    chat_container: Rc<RefCell<Container>>,
    transcript_tail_container: Rc<RefCell<Container>>,
    pending_messages_container: Rc<RefCell<Container>>,
    status_container: Rc<RefCell<Container>>,
    widget_container_above: Rc<RefCell<Container>>,
    widget_container_below: Rc<RefCell<Container>>,
    editor_container: Rc<RefCell<Container>>,
    footer_container: Rc<RefCell<Container>>,
    root_compositor: RefCell<Option<ComponentRef>>,
    fullscreen_layout_root: RefCell<Option<ComponentRef>>,
    transcript_tail_region: RefCell<Option<FlowRenderRegion>>,
    status_render_region: RefCell<Option<RenderRegion>>,
    editor: Rc<RefCell<CustomEditor>>,
    editor_handle: ComponentRef,
    footer: Rc<RefCell<FooterComponent>>,
    footer_data_provider: Arc<FooterDataProvider>,
    keybindings: KeybindingsManager,
    version: String,
    is_initialized: Cell<bool>,
    actions: RefCell<std::collections::VecDeque<ModeAction>>,
    events: RefCell<Option<tokio::sync::mpsc::UnboundedSender<AgentSessionEvent>>>,
    ui_requests: RefCell<Option<std::sync::mpsc::Receiver<UiRequest>>>,
    ui_sender: std::sync::mpsc::Sender<UiRequest>,
    ui_shared: Arc<UiShared>,
    ui_id: Cell<usize>,
    unsubscribe: Cell<Option<u64>>,
    shutdown_requested: Cell<bool>,
    tool_output_expanded: Cell<bool>,
    hide_thinking_block: Cell<bool>,
    hidden_thinking_label: RefCell<String>,
    is_bash_mode: Cell<bool>,
    streaming_component: RefCell<Option<ComponentRef>>,
    streaming_message: RefCell<Option<AssistantMessage>>,
    pending_tools: RefCell<IndexMap<String, ComponentRef>>,
    executing_tool_names: RefCell<IndexMap<String, String>>,
    executing_tool_started_at: RefCell<IndexMap<String, f64>>,
    extension_statuses: RefCell<IndexMap<String, String>>,
    extension_widgets_above: RefCell<IndexMap<String, Vec<String>>>,
    extension_widgets_below: RefCell<IndexMap<String, Vec<String>>>,
    on_input_callback: RefCell<Option<InputCallback>>,
    last_status_spacer: RefCell<Option<ComponentRef>>,
    last_status_text: RefCell<Option<ComponentRef>>,
    last_sigint_time: Cell<f64>,
    last_escape_time: Cell<f64>,
    working_message: RefCell<Option<String>>,
    working_visible: Cell<bool>,
    working_indicator_options: RefCell<Option<WorkingIndicatorOptions>>,
    compaction_queued_messages: RefCell<Vec<CompactionQueuedMessage>>,
    response_complete_notification:
        RefCell<Option<Box<dyn crate::core::desktop_notification::DesktopNotificationHandle>>>,
    custom_footer: RefCell<Option<ComponentRef>>,
    default_extension_footer: RefCell<Option<ComponentRef>>,
    custom_header: RefCell<Option<ComponentRef>>,
    changelog_markdown: RefCell<Option<String>>,
    terminal_current_directory: RefCell<Option<String>>,
}

impl InteractiveMode {
    /// Builds the interactive mode. Must run inside a tokio runtime context
    /// because [`FooterDataProvider`] captures the current runtime handle.
    pub fn new(
        runtime: AgentSessionRuntime,
        options: InteractiveModeOptions,
        terminal: Option<Rc<RefCell<dyn Terminal>>>,
    ) -> Result<Rc<Self>, String> {
        let session = runtime.session().clone();
        let settings = session.settings_manager();
        let tui_mode = options.tui_mode.unwrap_or_else(|| settings.get_tui_mode());
        let show_hardware_cursor = settings.get_show_hardware_cursor();
        let clear_on_shrink = settings.get_clear_on_shrink();
        let editor_padding_x = settings.get_editor_padding_x() as usize;
        let autocomplete_max_visible = settings.get_autocomplete_max_visible() as usize;
        let hide_thinking_block = settings.get_hide_thinking_block();
        let _output_pad = settings.get_output_pad();
        let theme_name = settings.get_theme().map(str::to_owned);
        drop(settings);

        let renderer = create_interactive_tui(InteractiveTuiOptions {
            tui_mode,
            show_hardware_cursor,
            log_directory: crate::config::get_agent_dir().display().to_string(),
            terminal,
            on_right_click_paste: None,
        });
        let ui = InteractiveTuiHandle::new(renderer);
        ui.set_clear_on_shrink(clear_on_shrink);

        let (header_container, header_ref) = SharedContainer::create();
        let (loaded_resources_container, loaded_ref) = SharedContainer::create();
        let (chat_container, chat_ref) = SharedContainer::create();
        let (transcript_tail_container, transcript_tail_ref) = SharedContainer::create();
        let (pending_messages_container, pending_ref) = SharedContainer::create();
        let (status_container, status_ref) = SharedContainer::create();
        let (widget_container_above, widget_above_ref) = SharedContainer::create();
        let (widget_container_below, widget_below_ref) = SharedContainer::create();
        let (editor_container, editor_ref) = SharedContainer::create();
        let (footer_container, footer_ref) = SharedContainer::create();

        let keybindings = KeybindingsManager::create(Some(&crate::config::get_agent_dir()));
        keybindings.clone().install();

        let editor = Rc::new(RefCell::new(
            CustomEditor::new(
                Rc::new(InteractiveTuiHost(Rc::clone(&ui))),
                crate::modes::interactive::theme::get_editor_theme(),
                keybindings.clone(),
                CustomEditorOptions {
                    editor: EditorOptions {
                        padding_x: Some(editor_padding_x),
                        autocomplete_max_visible: Some(autocomplete_max_visible),
                    },
                    legacy_prompt_history_path: Some(crate::config::get_prompt_history_path().display().to_string()),
                    prompt_history_control_db_path: None,
                    prompt_history_filter: Some(Rc::new(|text: &str| !is_skill_invocation(text, &[]))),
                },
            )
            .map_err(|error| error.to_string())?,
        ));
        let editor_handle: ComponentRef = component(EditorHandle(Rc::clone(&editor)));

        let footer_data_provider = Arc::new(FooterDataProvider::new(session.cwd()));
        let footer = Rc::new(RefCell::new(FooterComponent::new(
            Arc::new(session.clone()),
            Arc::clone(&footer_data_provider),
        )));
        footer
            .borrow_mut()
            .set_auto_compact_enabled(session.auto_compaction_enabled());

        let (ui_sender, ui_receiver) = std::sync::mpsc::channel();
        let shared = Arc::new(UiShared::new());

        let mode = Rc::new(Self {
            runtime: tokio::sync::Mutex::new(runtime),
            session: RefCell::new(session),
            options,
            ui,
            header_container,
            loaded_resources_container,
            chat_container,
            transcript_tail_container,
            pending_messages_container,
            status_container,
            widget_container_above,
            widget_container_below,
            editor_container,
            footer_container,
            root_compositor: RefCell::new(None),
            fullscreen_layout_root: RefCell::new(None),
            transcript_tail_region: RefCell::new(None),
            status_render_region: RefCell::new(None),
            editor,
            editor_handle,
            footer,
            footer_data_provider,
            keybindings,
            version: crate::config::package_config().version.clone(),
            is_initialized: Cell::new(false),
            actions: RefCell::new(std::collections::VecDeque::new()),
            events: RefCell::new(None),
            ui_requests: RefCell::new(Some(ui_receiver)),
            ui_sender,
            ui_shared: shared,
            ui_id: Cell::new(0),
            unsubscribe: Cell::new(None),
            shutdown_requested: Cell::new(false),
            tool_output_expanded: Cell::new(false),
            hide_thinking_block: Cell::new(hide_thinking_block),
            hidden_thinking_label: RefCell::new("Thinking...".to_owned()),
            is_bash_mode: Cell::new(false),
            streaming_component: RefCell::new(None),
            streaming_message: RefCell::new(None),
            pending_tools: RefCell::new(IndexMap::new()),
            executing_tool_names: RefCell::new(IndexMap::new()),
            executing_tool_started_at: RefCell::new(IndexMap::new()),
            extension_statuses: RefCell::new(IndexMap::new()),
            extension_widgets_above: RefCell::new(IndexMap::new()),
            extension_widgets_below: RefCell::new(IndexMap::new()),
            on_input_callback: RefCell::new(None),
            last_status_spacer: RefCell::new(None),
            last_status_text: RefCell::new(None),
            last_sigint_time: Cell::new(0.0),
            last_escape_time: Cell::new(0.0),
            working_message: RefCell::new(None),
            working_visible: Cell::new(true),
            working_indicator_options: RefCell::new(None),
            compaction_queued_messages: RefCell::new(Vec::new()),
            response_complete_notification: RefCell::new(None),
            custom_footer: RefCell::new(None),
            default_extension_footer: RefCell::new(None),
            custom_header: RefCell::new(None),
            changelog_markdown: RefCell::new(None),
            terminal_current_directory: RefCell::new(None),
        });

        let id = register_mode(&mode);
        mode.ui_id.set(id);

        mode.editor_container
            .borrow_mut()
            .add_child(Rc::clone(&mode.editor_handle));
        mode.footer_container
            .borrow_mut()
            .add_child(component(FooterHandle(Rc::clone(&mode.footer))));

        let _ = header_ref;
        let _ = loaded_ref;
        let _ = chat_ref;
        let _ = transcript_tail_ref;
        let _ = pending_ref;
        let _ = status_ref;
        let _ = widget_above_ref;
        let _ = widget_below_ref;
        let _ = editor_ref;
        let _ = footer_ref;
        let _ = theme_name;

        Ok(mode)
    }

    fn ui_context(&self) -> Arc<dyn ExtensionUiContext> {
        Arc::new(InteractiveUiContext {
            id: self.ui_id.get(),
            sender: self.ui_sender.clone(),
        })
    }

    fn session(&self) -> AgentSession {
        self.session.borrow().clone()
    }

    fn settings<R>(&self, f: impl FnOnce(&crate::core::settings_manager::SettingsManager) -> R) -> R {
        f(&self.session.borrow().settings_manager())
    }

    fn cwd(&self) -> String {
        self.session.borrow().cwd().to_owned()
    }

    fn markdown_theme(&self) -> Rc<dyn pi_tui::components::markdown::MarkdownTheme> {
        let indent = self.settings(|settings| settings.get_code_block_indent());
        Rc::new(SettingsMarkdownTheme {
            inner: Rc::from(crate::modes::interactive::theme::get_markdown_theme()),
            code_block_indent: indent,
        })
    }

    fn markdown_transformers(&self) -> Vec<crate::core::extensions::types::MarkdownTransformer> {
        self.session.borrow().extension_runner().get_markdown_transformers()
    }

    fn output_pad(&self) -> usize {
        self.settings(|settings| settings.get_output_pad()) as usize
    }

    fn rebuild_root_compositor(&self) {
        self.transcript_tail_region.borrow_mut().take();
        self.status_render_region.borrow_mut().take();
        let transcript_tail_ref = component(SharedContainer(Rc::clone(&self.transcript_tail_container)));
        let status_ref = component(SharedContainer(Rc::clone(&self.status_container)));
        let tail_region = self.ui.create_flow_render_region(Rc::clone(&transcript_tail_ref));
        let status_region = self.ui.create_render_region(Rc::clone(&status_ref));
        *self.transcript_tail_region.borrow_mut() = tail_region;
        *self.status_render_region.borrow_mut() = status_region;

        let height_ui = Rc::clone(&self.ui);
        let root = create_interactive_root_compositor(InteractiveRootCompositorOptions {
            get_height: Rc::new(move || height_ui.terminal().borrow().rows()),
            header: component(SharedContainer(Rc::clone(&self.header_container))),
            loaded_resources: component(SharedContainer(Rc::clone(&self.loaded_resources_container))),
            chat: component(SharedContainer(Rc::clone(&self.chat_container))),
            transcript_tail: transcript_tail_ref,
            pending_messages: component(SharedContainer(Rc::clone(&self.pending_messages_container))),
            on_transcript_tail_layout: Rc::new(|_| {}),
            status: status_ref,
            widget_above: component(SharedContainer(Rc::clone(&self.widget_container_above))),
            editor: component(SharedContainer(Rc::clone(&self.editor_container))),
            widget_below: component(SharedContainer(Rc::clone(&self.widget_container_below))),
            footer: component(SharedContainer(Rc::clone(&self.footer_container))),
            on_status_layout: Rc::new(|_| {}),
            on_editor_layout: Rc::new(|_| {}),
        });
        *self.root_compositor.borrow_mut() = Some(component(root));
    }

    fn mount_interactive_tui(&self) {
        if self.ui.mode() == TuiMode::Fullscreen {
            for child in [
                component(SharedContainer(Rc::clone(&self.header_container))),
                component(SharedContainer(Rc::clone(&self.loaded_resources_container))),
                component(SharedContainer(Rc::clone(&self.chat_container))),
                component(SharedContainer(Rc::clone(&self.pending_messages_container))),
                component(SharedContainer(Rc::clone(&self.status_container))),
                component(SharedContainer(Rc::clone(&self.widget_container_above))),
                component(SharedContainer(Rc::clone(&self.editor_container))),
                component(SharedContainer(Rc::clone(&self.widget_container_below))),
                component(SharedContainer(Rc::clone(&self.footer_container))),
            ] {
                self.ui.add_child(child);
            }
            if let Some(layout_root) = self.fullscreen_layout_root.borrow().clone() {
                self.ui.set_layout_root(Some(layout_root));
            }
        } else {
            self.rebuild_root_compositor();
            if let Some(root) = self.root_compositor.borrow().clone() {
                self.ui.add_child(root);
            }
        }
    }

    fn build_fullscreen_layout(&self) {
        let scroll_view = ScrollView::new(
            component(SharedContainer(Rc::clone(&self.chat_container))),
            ScrollViewOptions {
                follow: Some(ScrollFollow::End),
                primary: true,
                overscroll: Some(ScrollOverscroll::Chain),
                scrollbar: Some(ScrollViewScrollbar::Hidden),
                scrollbar_style: None,
                scrollbar_hide_delay_ms: None,
            },
        );
        let dock = VStack::new(
            vec![
                stack_entry(
                    component(SharedContainer(Rc::clone(&self.pending_messages_container))),
                    None,
                    None,
                    Some(1),
                    Some(0),
                ),
                stack_entry(
                    component(SharedContainer(Rc::clone(&self.status_container))),
                    None,
                    None,
                    Some(1),
                    Some(0),
                ),
                stack_entry(
                    component(SharedContainer(Rc::clone(&self.widget_container_above))),
                    None,
                    None,
                    Some(1),
                    Some(0),
                ),
                stack_entry(
                    component(SharedContainer(Rc::clone(&self.editor_container))),
                    None,
                    None,
                    Some(1),
                    Some(3),
                ),
                stack_entry(
                    component(SharedContainer(Rc::clone(&self.widget_container_below))),
                    None,
                    None,
                    Some(1),
                    Some(0),
                ),
                stack_entry(
                    component(SharedContainer(Rc::clone(&self.footer_container))),
                    None,
                    None,
                    Some(1),
                    Some(1),
                ),
            ],
            StackOptions::default(),
        );
        let root = VStack::new(
            vec![
                stack_entry(
                    component(scroll_view),
                    Some(pi_tui::layout_node::StackBasis::Cells(0)),
                    Some(1),
                    Some(1),
                    Some(1),
                ),
                stack_entry(
                    component(dock),
                    Some(pi_tui::layout_node::StackBasis::Auto),
                    Some(0),
                    Some(1),
                    Some(1),
                ),
            ],
            StackOptions::default(),
        );
        *self.fullscreen_layout_root.borrow_mut() = Some(component(root));
    }

    fn update_terminal_current_directory(&self) {
        let cwd = self.cwd();
        if self.terminal_current_directory.borrow().as_deref() == Some(cwd.as_str()) {
            return;
        }
        *self.terminal_current_directory.borrow_mut() = Some(cwd.clone());
        let sequence = crate::utils::terminal_current_directory::format_terminal_current_directory_sequence(&cwd);
        let terminal = self.ui.terminal();
        terminal.borrow_mut().write(&sequence);
    }

    fn update_terminal_title(&self) {
        let cwd = self.cwd();
        let basename = Path::new(&cwd)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| cwd.clone());
        let title = match self.session.borrow().session_name() {
            Some(name) => format!("{} - {name} - {basename}", crate::config::package_config().app_title),
            None => format!("{} - {basename}", crate::config::package_config().app_title),
        };
        self.ui.terminal().borrow_mut().set_title(&title);
    }

    /// `getStartupExpansionState`.
    fn startup_expansion_state(&self) -> bool {
        self.options.verbose || self.tool_output_expanded.get()
    }

    fn show_status(&self, message: &str) {
        let last = self.chat_container.borrow().children().last().cloned();
        let second_last = {
            let container = self.chat_container.borrow();
            let children = container.children();
            children.get(children.len().saturating_sub(2)).cloned()
        };
        let last_status_text = self.last_status_text.borrow().clone();
        let last_status_spacer = self.last_status_spacer.borrow().clone();
        if let (Some(last), Some(second_last), Some(last_text), Some(last_spacer)) =
            (last, second_last, last_status_text, last_status_spacer)
            && Rc::ptr_eq(&last, &last_text)
            && Rc::ptr_eq(&second_last, &last_spacer)
        {
            if let Some(text) = last
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<Text>())
            {
                text.set_text(with_theme(|theme| theme.fg(ThemeColor::Dim, message)));
            }
            self.ui.request_render();
            return;
        }

        let spacer = component(Spacer::new(1));
        let text = component(Text::new(with_theme(|theme| theme.fg(ThemeColor::Dim, message)), 1, 0));
        {
            let mut chat = self.chat_container.borrow_mut();
            chat.add_child(Rc::clone(&spacer));
            chat.add_child(Rc::clone(&text));
        }
        *self.last_status_spacer.borrow_mut() = Some(spacer);
        *self.last_status_text.borrow_mut() = Some(text);
        self.ui.request_render();
    }

    fn show_error(&self, message: &str) {
        let text = component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Error, message)),
            1,
            0,
        ));
        self.chat_container.borrow_mut().add_child(text);
        self.ui.request_render();
    }

    fn show_warning(&self, message: &str) {
        let text = component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Warning, message)),
            1,
            0,
        ));
        self.chat_container.borrow_mut().add_child(text);
        self.ui.request_render();
    }

    fn get_user_message_text(message: &AgentMessage) -> String {
        let AgentMessage::User(user) = message else {
            return String::new();
        };
        match &user.content {
            pi_ai::types::UserContent::Text(text) => text.clone(),
            pi_ai::types::UserContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| match block {
                    pi_ai::types::UserContentBlock::Text(text) => Some(text.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        }
    }

    fn add_message_to_chat(&self, message: &AgentMessage) {
        match message {
            AgentMessage::User(user) => {
                let text = Self::get_user_message_text(message);
                if text.is_empty() {
                    return;
                }
                if !self.chat_container.borrow().children().is_empty() {
                    self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
                }
                if let Some(skill_block) = crate::parse_skill_block(&text) {
                    let user_message = skill_block.user_message.clone();
                    let mut skill = SkillInvocationMessageComponent::new(
                        skill_block,
                        self.markdown_theme(),
                        crate::modes::interactive::theme::current_theme(),
                    );
                    skill.set_expanded(self.tool_output_expanded.get());
                    self.chat_container.borrow_mut().add_child(component(skill));
                    if let Some(user_message) = user_message {
                        self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
                        self.chat_container
                            .borrow_mut()
                            .add_child(component(UserMessageComponent::new(
                                user_message,
                                Some(self.markdown_theme()),
                                self.output_pad(),
                                self.markdown_transformers(),
                            )));
                    }
                } else {
                    self.chat_container
                        .borrow_mut()
                        .add_child(component(UserMessageComponent::new(
                            text,
                            Some(self.markdown_theme()),
                            self.output_pad(),
                            self.markdown_transformers(),
                        )));
                }
                let _ = user;
            }
            AgentMessage::Assistant(assistant) => {
                self.chat_container
                    .borrow_mut()
                    .add_child(component(AssistantMessageComponent::new(
                        Some((**assistant).clone()),
                        self.hide_thinking_block.get(),
                        Some(self.markdown_theme()),
                        Some(self.hidden_thinking_label.borrow().clone()),
                        self.output_pad(),
                        self.markdown_transformers(),
                    )));
            }
            AgentMessage::ToolResult(_) => {}
            AgentMessage::Custom(object) => self.add_custom_message_to_chat(object),
        }
    }

    fn add_custom_message_to_chat(&self, object: &serde_json::Map<String, Value>) {
        let role = object.get("role").and_then(Value::as_str).unwrap_or_default();
        match role {
            "bashExecution" => {
                let command = json_string(object, "command");
                let output = json_string(object, "output");
                let exit_code = object.get("exitCode").and_then(Value::as_f64);
                let cancelled = object.get("cancelled").and_then(Value::as_bool).unwrap_or(false);
                let truncated = object.get("truncated").and_then(Value::as_bool).unwrap_or(false);
                let full_output_path = object.get("fullOutputPath").and_then(Value::as_str).map(str::to_owned);
                let exclude_from_context = object
                    .get("excludeFromContext")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let mut component_value = BashExecutionComponent::new(
                    command,
                    Rc::new({
                        let ui = Rc::clone(&self.ui);
                        move || ui.request_render()
                    }),
                    exclude_from_context,
                    BashExecutionOptions {
                        show_elapsed: Some(false),
                    },
                );
                if !output.is_empty() {
                    component_value.append_output(&output);
                }
                component_value.set_complete(
                    exit_code.map(|code| code as i32),
                    cancelled,
                    truncated.then(|| crate::core::tools::truncate::TruncationResult {
                        content: String::new(),
                        truncated: true,
                        truncated_by: None,
                        total_lines: 0,
                        total_bytes: 0,
                        output_lines: 0,
                        output_bytes: 0,
                        last_line_partial: false,
                        first_line_exceeds_limit: false,
                        max_lines: 0,
                        max_bytes: 0,
                    }),
                    full_output_path,
                );
                self.chat_container.borrow_mut().add_child(component(component_value));
            }
            "compactionSummary" => {
                let Ok(message) = serde_json::from_value::<crate::core::messages::CompactionSummaryMessage>(
                    Value::Object(object.clone()),
                ) else {
                    return;
                };
                self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
                let mut summary = CompactionSummaryMessageComponent::new(message, Some(self.markdown_theme()));
                summary.set_expanded(self.tool_output_expanded.get());
                self.chat_container.borrow_mut().add_child(component(summary));
            }
            "branchSummary" => {
                let message = crate::core::messages::BranchSummaryMessage {
                    role: pi_agent_core::harness::messages::BranchSummaryRole::BranchSummary,
                    summary: json_string(object, "summary"),
                    from_id: json_string(object, "fromId"),
                    timestamp: object.get("timestamp").and_then(Value::as_f64).unwrap_or_default(),
                };
                self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
                let mut summary = BranchSummaryMessageComponent::new(message, Some(self.markdown_theme()));
                summary.set_expanded(self.tool_output_expanded.get());
                self.chat_container.borrow_mut().add_child(component(summary));
            }
            _ => {
                if object.get("display").and_then(Value::as_bool) == Some(true) {
                    let renderer = self.session.borrow().extension_runner().get_message_renderer(role);
                    let content = match object.get("content") {
                        Some(Value::String(text)) => {
                            crate::core::extensions::types::CustomMessageContent::Text(text.clone())
                        }
                        Some(Value::Array(_)) => crate::core::extensions::types::CustomMessageContent::Blocks(
                            serde_json::from_value(object.get("content").cloned().unwrap_or_default())
                                .unwrap_or_default(),
                        ),
                        _ => crate::core::extensions::types::CustomMessageContent::Text(String::new()),
                    };
                    let message = crate::core::extensions::types::CustomMessageValue {
                        custom_type: role.to_owned(),
                        content,
                        display: true,
                        details: object.get("details").cloned(),
                        timestamp: object.get("timestamp").and_then(Value::as_f64).unwrap_or_default(),
                    };
                    let mut custom =
                        CustomMessageComponent::new(message, renderer, Some(self.markdown_theme()), self.output_pad());
                    custom.set_expanded(self.tool_output_expanded.get());
                    self.chat_container.borrow_mut().add_child(component(custom));
                }
            }
        }
    }

    fn render_initial_messages(&self) {
        for message in self.session.borrow().messages() {
            self.add_message_to_chat(&message);
        }
        self.ui.request_render();
    }

    fn show_loaded_resources(&self, force: bool, show_diagnostics_when_quiet: bool) {
        self.loaded_resources_container.borrow_mut().clear();
        let quiet = self.settings(|settings| settings.get_quiet_startup());
        let show_listing = force || self.options.verbose || !quiet;
        let show_diagnostics = show_listing || show_diagnostics_when_quiet;
        if !show_listing && !show_diagnostics {
            return;
        }

        let session = self.session.borrow();
        let loader = session.resource_loader();
        let skills = loader.get_skills();
        let prompts = loader.get_prompts();
        let themes = loader.get_themes();
        let agents_files = loader.get_agents_files();
        let rules_files = loader.get_rules_files();
        let system_prompt_source = loader.get_system_prompt_source();
        let append_sources = loader.get_append_system_prompt_sources();
        let extensions = loader.get_extensions();
        let active_tools = session.get_active_tool_names();
        let prompt_templates = session.prompt_templates();
        let extension_runner = session.extension_runner();
        drop(session);

        let mut source_infos: HashMap<String, SourceInfo> = HashMap::new();
        for extension in &extensions.extensions {
            source_infos.insert(extension.path.clone(), extension.source_info.clone());
        }
        for skill in &skills.skills {
            source_infos.insert(skill.file_path.clone(), skill.source_info.clone());
        }
        for prompt in &prompts.prompts {
            source_infos.insert(prompt.file_path.clone(), prompt.source_info.clone());
        }
        for theme in &themes.themes {
            if let (Some(source_path), Some(source_info)) = (&theme.source_path, &theme.source_info) {
                source_infos.insert(source_path.display().to_string(), source_info.clone());
            }
        }

        let expanded = self.startup_expansion_state();
        let cwd = self.cwd();

        if show_listing {
            let mut context_files: Vec<String> = Vec::new();
            if let Some(source) = &system_prompt_source {
                context_files.push(source.path.clone());
            }
            context_files.extend(append_sources.iter().map(|source| source.path.clone()));
            context_files.extend(agents_files.agents_files.iter().map(|file| file.path.clone()));

            if !context_files.is_empty() || !rules_files.rules_files.is_empty() {
                self.loaded_resources_container
                    .borrow_mut()
                    .add_child(component(Spacer::new(1)));
            }
            if !context_files.is_empty() {
                let expanded_body = context_files
                    .iter()
                    .map(|path| {
                        with_theme(|theme| theme.fg(ThemeColor::Dim, &format!("  {}", format_display_path(path))))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let compact_items = context_files
                    .iter()
                    .map(|path| format_context_path(path, &cwd))
                    .collect::<Vec<_>>();
                self.add_loaded_section(
                    "Context",
                    &format_compact_list(&compact_items, false),
                    &expanded_body,
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
            if !rules_files.rules_files.is_empty() {
                let expanded_body = rules_files
                    .rules_files
                    .iter()
                    .map(|file| {
                        with_theme(|theme| theme.fg(ThemeColor::Dim, &format!("  {}", format_display_path(&file.path))))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let compact_items = rules_files
                    .rules_files
                    .iter()
                    .map(|file| format_display_path(&file.path))
                    .collect::<Vec<_>>();
                self.add_loaded_section(
                    "User Rules",
                    &format_compact_list(&compact_items, false),
                    &expanded_body,
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
            if !active_tools.is_empty() {
                self.add_loaded_section(
                    "Tools",
                    &format_compact_list(&active_tools, true),
                    &format_compact_list(&active_tools, true),
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
            if !skills.skills.is_empty() {
                let groups = build_scope_groups(
                    &skills
                        .skills
                        .iter()
                        .map(|skill| ScopeGroupItem {
                            path: skill.file_path.clone(),
                            source_info: Some(skill.source_info.clone()),
                        })
                        .collect::<Vec<_>>(),
                );
                let expanded_body = format_scope_groups(
                    &groups,
                    |item| format_display_path(&item.path),
                    |item, _source| get_short_path(&item.path, item.source_info.as_ref()),
                );
                let compact_items = skills.skills.iter().map(|skill| skill.name.clone()).collect::<Vec<_>>();
                self.add_loaded_section(
                    "Skills",
                    &format_compact_list(&compact_items, true),
                    &expanded_body,
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
            if !prompt_templates.is_empty() {
                let groups = build_scope_groups(
                    &prompt_templates
                        .iter()
                        .map(|template| ScopeGroupItem {
                            path: template.file_path.clone(),
                            source_info: Some(template.source_info.clone()),
                        })
                        .collect::<Vec<_>>(),
                );
                let by_path: HashMap<&str, &str> = prompt_templates
                    .iter()
                    .map(|template| (template.file_path.as_str(), template.name.as_str()))
                    .collect();
                let expanded_body = format_scope_groups(
                    &groups,
                    |item| match by_path.get(item.path.as_str()) {
                        Some(name) => format!("/{name}"),
                        None => format_display_path(&item.path),
                    },
                    |item, _source| match by_path.get(item.path.as_str()) {
                        Some(name) => format!("/{name}"),
                        None => format_display_path(&item.path),
                    },
                );
                let compact_items = prompt_templates
                    .iter()
                    .map(|template| format!("/{}", template.name))
                    .collect::<Vec<_>>();
                self.add_loaded_section(
                    "Prompts",
                    &format_compact_list(&compact_items, true),
                    &expanded_body,
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
            let listed_extensions: Vec<(String, Option<SourceInfo>)> = extensions
                .extensions
                .iter()
                .filter(|extension| extension.hidden != Some(true))
                .map(|extension| (extension.path.clone(), Some(extension.source_info.clone())))
                .collect();
            if !listed_extensions.is_empty() {
                let groups = build_scope_groups(
                    &listed_extensions
                        .iter()
                        .map(|(path, source_info)| ScopeGroupItem {
                            path: path.clone(),
                            source_info: source_info.clone(),
                        })
                        .collect::<Vec<_>>(),
                );
                let expanded_body = format_scope_groups(
                    &groups,
                    |item| format_extension_display_path(&item.path),
                    |item, _source| {
                        format_extension_display_path(&get_short_path(&item.path, item.source_info.as_ref()))
                    },
                );
                let compact_labels = get_compact_extension_labels(&listed_extensions);
                self.add_loaded_section(
                    "Extensions",
                    &format_compact_list(&compact_labels, true),
                    &expanded_body,
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
            let custom_themes: Vec<(String, Option<SourceInfo>, String)> = themes
                .themes
                .iter()
                .filter_map(|theme| {
                    let source_path = theme.source_path.as_ref()?;
                    Some((
                        source_path.display().to_string(),
                        theme.source_info.clone(),
                        theme.name.clone().unwrap_or_default(),
                    ))
                })
                .collect();
            if !custom_themes.is_empty() {
                let groups = build_scope_groups(
                    &custom_themes
                        .iter()
                        .map(|(path, source_info, _)| ScopeGroupItem {
                            path: path.clone(),
                            source_info: source_info.clone(),
                        })
                        .collect::<Vec<_>>(),
                );
                let expanded_body = format_scope_groups(
                    &groups,
                    |item| format_display_path(&item.path),
                    |item, _source| get_short_path(&item.path, item.source_info.as_ref()),
                );
                let compact_items = custom_themes
                    .iter()
                    .map(|(path, source_info, name)| {
                        if name.is_empty() {
                            get_compact_path_label(path, source_info.as_ref())
                        } else {
                            name.clone()
                        }
                    })
                    .collect::<Vec<_>>();
                self.add_loaded_section(
                    "Themes",
                    &format_compact_list(&compact_items, true),
                    &expanded_body,
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
        }

        if show_diagnostics {
            let sections = [
                ("Skill conflicts", &skills.diagnostics),
                ("Prompt conflicts", &prompts.diagnostics),
                ("Theme conflicts", &themes.diagnostics),
            ];
            for (label, diagnostics) in sections {
                if diagnostics.is_empty() {
                    continue;
                }
                let body = format_diagnostics(diagnostics, &source_infos);
                self.loaded_resources_container
                    .borrow_mut()
                    .add_child(component(Text::new(
                        format!(
                            "{}\n{body}",
                            with_theme(|theme| theme.fg(ThemeColor::Warning, &format!("[{label}]")))
                        ),
                        0,
                        0,
                    )));
                self.loaded_resources_container
                    .borrow_mut()
                    .add_child(component(Spacer::new(1)));
            }

            let mut extension_diagnostics: Vec<ResourceDiagnostic> = extensions
                .errors
                .iter()
                .map(|error| ResourceDiagnostic {
                    diagnostic_type: ResourceDiagnosticType::Error,
                    message: error.error.clone(),
                    path: Some(error.path.clone()),
                    collision: None,
                })
                .collect();
            extension_diagnostics.extend(extension_runner.get_command_diagnostics());
            extension_diagnostics.extend(extension_runner.get_shortcut_diagnostics());
            if !extension_diagnostics.is_empty() {
                let body = format_diagnostics(&extension_diagnostics, &source_infos);
                self.loaded_resources_container
                    .borrow_mut()
                    .add_child(component(Text::new(
                        format!(
                            "{}\n{body}",
                            with_theme(|theme| theme.fg(ThemeColor::Warning, "[Extension issues]"))
                        ),
                        0,
                        0,
                    )));
                self.loaded_resources_container
                    .borrow_mut()
                    .add_child(component(Spacer::new(1)));
            }
        }
        self.ui.request_render();
    }

    fn add_loaded_section(
        &self,
        name: &str,
        collapsed_body: &str,
        expanded_body: &str,
        color: ThemeColor,
        expanded: bool,
    ) {
        let collapsed = format!("{}\n{}", section_header(name, color), collapsed_body);
        let expanded_text = format!("{}\n{}", section_header(name, color), expanded_body);
        let section = ExpandableText::new(move || collapsed.clone(), move || expanded_text.clone(), expanded, 0, 0);
        self.loaded_resources_container
            .borrow_mut()
            .add_child(component(section));
        self.loaded_resources_container
            .borrow_mut()
            .add_child(component(Spacer::new(1)));
    }

    fn set_theme_name(&self, name: &str) -> ThemeSetResult {
        let result = crate::modes::interactive::theme::set_theme(name, false);
        if result.success {
            self.settings_mut(|settings| {
                if settings.get_theme() != Some(name) {
                    settings.set_theme(name);
                }
            });
            self.ui.invalidate();
            self.update_editor_border_color();
            self.ui.request_render();
        }
        ThemeSetResult {
            success: result.success,
            error: result.error,
        }
    }

    fn settings_mut<R>(&self, f: impl FnOnce(&mut crate::core::settings_manager::SettingsManager) -> R) -> R {
        f(&mut self.session.borrow().settings_manager())
    }

    fn update_editor_border_color(&self) {
        let color = if self.is_bash_mode.get() {
            ThemeColor::BashMode
        } else {
            ThemeColor::Border
        };
        let style: pi_tui::components::text::StyleFn =
            Rc::new(move |text: &str| with_theme(|theme| theme.fg(color, text)));
        self.editor.borrow_mut().set_border_color(style);
    }

    fn render_widgets(&self) {
        self.render_widget_container(&self.widget_container_above, &self.extension_widgets_above.borrow());
        self.render_widget_container(&self.widget_container_below, &self.extension_widgets_below.borrow());
    }

    fn render_widget_container(&self, container: &Rc<RefCell<Container>>, widgets: &IndexMap<String, Vec<String>>) {
        let mut container = container.borrow_mut();
        container.clear();
        if widgets.is_empty() {
            container.add_child(component(Spacer::new(1)));
            return;
        }
        container.add_child(component(Spacer::new(1)));
        for lines in widgets.values() {
            container.add_child(component(Text::new(lines.join("\n"), 1, 0)));
        }
    }

    fn add_extension_widget(&self, key: &str, content: Option<Vec<String>>, placement: WidgetPlacement) {
        let widgets = match placement {
            WidgetPlacement::AboveEditor => &self.extension_widgets_above,
            WidgetPlacement::BelowEditor => &self.extension_widgets_below,
        };
        match content {
            Some(lines) => {
                widgets.borrow_mut().insert(key.to_owned(), lines);
            }
            None => {
                widgets.borrow_mut().shift_remove(key);
            }
        }
        self.render_widgets();
        self.ui.request_render();
    }

    fn clear_extension_widgets(&self) {
        self.extension_widgets_above.borrow_mut().clear();
        self.extension_widgets_below.borrow_mut().clear();
        self.render_widgets();
    }

    fn update_pending_messages_display(&self) {
        let session = self.session.borrow();
        let steering = session.get_steering_messages();
        let follow_up = session.get_follow_up_messages();
        drop(session);
        let mut container = self.pending_messages_container.borrow_mut();
        container.clear();
        if steering.is_empty() && follow_up.is_empty() {
            return;
        }
        container.add_child(component(Spacer::new(1)));
        for message in steering {
            container.add_child(component(Text::new(
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Dim,
                        &format!(
                            "  {} {}",
                            with_theme(|theme| theme.fg(ThemeColor::Accent, "Steering:")),
                            message
                        ),
                    )
                }),
                1,
                0,
            )));
        }
        for message in follow_up {
            container.add_child(component(Text::new(
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Dim,
                        &format!(
                            "  {} {}",
                            with_theme(|theme| theme.fg(ThemeColor::Accent, "Follow-up:")),
                            format_follow_up_preview(&message)
                        ),
                    )
                }),
                1,
                0,
            )));
        }
    }

    fn add_submitted_text_to_history(&self, text: &str) {
        self.editor.borrow_mut().add_to_history_without_persistence(text);
    }

    fn clear_editor(&self) {
        self.editor.borrow_mut().set_text("");
        self.ui.request_render();
    }

    fn set_tools_expanded(&self, expanded: bool) {
        self.tool_output_expanded.set(expanded);
        self.settings_mut(|settings| settings.set_output_pad(if expanded { 0 } else { 1 }));
        self.ui.request_render();
    }

    fn toggle_tool_output_expansion(&self) {
        self.set_tools_expanded(!self.tool_output_expanded.get());
    }

    fn toggle_thinking_block_visibility(&self) {
        let hidden = !self.hide_thinking_block.get();
        self.hide_thinking_block.set(hidden);
        self.settings_mut(|settings| settings.set_hide_thinking_block(hidden));
        self.ui.request_render();
    }

    fn cycle_thinking_level(&self) {
        let session = self.session.borrow();
        if let Some(level) = session.cycle_thinking_level() {
            self.show_status(&format!("Thinking level: {}", level.as_str()));
        }
        drop(session);
        self.footer.borrow_mut().invalidate();
        self.ui.request_render();
    }

    fn handle_ctrl_c(&self) {
        let now = js_now_millis();
        if now - self.last_sigint_time.get() < 1000.0 {
            self.actions.borrow_mut().push_back(ModeAction::Shutdown);
        } else {
            self.last_sigint_time.set(now);
            self.clear_editor();
        }
    }

    fn handle_ctrl_d(&self) {
        if self.editor.borrow().get_text().is_empty() {
            self.actions.borrow_mut().push_back(ModeAction::Shutdown);
        }
    }

    fn handle_editor_escape(&self) {
        if self.is_bash_mode.get() {
            self.is_bash_mode.set(false);
            self.update_editor_border_color();
            return;
        }
        let session = self.session.borrow().clone();
        if session.is_streaming() || session.is_bash_running() || session.is_compacting() {
            self.actions.borrow_mut().push_back(ModeAction::Shutdown);
            return;
        }
        let now = js_now_millis();
        if now - self.last_escape_time.get() < 500.0 {
            self.actions.borrow_mut().push_back(ModeAction::Shutdown);
        } else {
            self.last_escape_time.set(now);
            self.editor.borrow_mut().set_text("");
            self.ui.request_render();
        }
    }

    fn stop(&self) {
        if let Some(id) = self.unsubscribe.get() {
            self.session.borrow().unsubscribe(id);
            self.unsubscribe.set(None);
        }
        self.ui.stop();
    }

    fn close_response_complete_notification(&self) {
        let notification = self.response_complete_notification.borrow_mut().take();
        if let Some(notification) = notification {
            notification.close();
        }
    }
}

/// Component adapter for the shared footer component.
pub struct FooterHandle(pub Rc<RefCell<FooterComponent>>);

impl Component for FooterHandle {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.0.borrow_mut().render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.0.borrow_mut().invalidate();
    }
}

fn stack_entry(
    component: ComponentRef,
    basis: Option<pi_tui::layout_node::StackBasis>,
    grow: Option<usize>,
    shrink: Option<usize>,
    min_size: Option<usize>,
) -> StackLayoutEntry {
    StackLayoutEntry {
        component,
        basis,
        grow,
        shrink,
        min_size,
        max_size: None,
        visible: None,
    }
}

/// `MarkdownTheme` wrapper applying the configured code-block indent.
struct SettingsMarkdownTheme {
    inner: Rc<dyn pi_tui::components::markdown::MarkdownTheme>,
    code_block_indent: String,
}

impl pi_tui::components::markdown::MarkdownTheme for SettingsMarkdownTheme {
    fn heading(&self, text: &str) -> String {
        self.inner.heading(text)
    }
    fn link(&self, text: &str) -> String {
        self.inner.link(text)
    }
    fn link_url(&self, text: &str) -> String {
        self.inner.link_url(text)
    }
    fn code(&self, text: &str) -> String {
        self.inner.code(text)
    }
    fn code_block(&self, text: &str) -> String {
        self.inner.code_block(text)
    }
    fn code_block_border(&self, text: &str) -> String {
        self.inner.code_block_border(text)
    }
    fn quote(&self, text: &str) -> String {
        self.inner.quote(text)
    }
    fn quote_border(&self, text: &str) -> String {
        self.inner.quote_border(text)
    }
    fn hr(&self, text: &str) -> String {
        self.inner.hr(text)
    }
    fn list_bullet(&self, text: &str) -> String {
        self.inner.list_bullet(text)
    }
    fn bold(&self, text: &str) -> String {
        self.inner.bold(text)
    }
    fn italic(&self, text: &str) -> String {
        self.inner.italic(text)
    }
    fn strikethrough(&self, text: &str) -> String {
        self.inner.strikethrough(text)
    }
    fn underline(&self, text: &str) -> String {
        self.inner.underline(text)
    }
    fn highlight_code(&self, code: &str, lang: Option<&str>) -> Option<Vec<String>> {
        self.inner.highlight_code(code, lang)
    }
    fn code_block_indent(&self) -> &str {
        &self.code_block_indent
    }
}

/// Entry point used by `ca-main`: runs interactive mode on a local task set.
pub async fn run_interactive_mode(
    runtime: AgentSessionRuntime,
    options: InteractiveModeOptions,
    terminal: Option<Rc<RefCell<dyn Terminal>>>,
) -> Result<(), String> {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let mode = InteractiveMode::new(runtime, options, terminal)?;
            mode.run().await
        })
        .await
}

fn json_string(object: &serde_json::Map<String, Value>, key: &str) -> String {
    object.get(key).and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn js_now_millis() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as f64)
        .unwrap_or_default()
}

impl InteractiveMode {
    /// Installs synchronous TUI callbacks that enqueue async work.
    fn bind_callbacks(self: &Rc<Self>) {
        let mode = Rc::clone(self);
        self.editor.borrow_mut().editor_mut().on_submit = Some(Rc::new(move |text: &str| {
            mode.actions.borrow_mut().push_back(ModeAction::Submit(text.to_owned()));
        }));

        let mode = Rc::clone(self);
        self.editor.borrow_mut().editor_mut().on_change = Some(Rc::new(move |text: &str| {
            let was_bash = mode.is_bash_mode.get();
            mode.is_bash_mode.set(text.trim_start().starts_with('!'));
            if was_bash != mode.is_bash_mode.get() {
                mode.update_editor_border_color();
            }
        }));

        let mode = Rc::clone(self);
        self.editor.borrow_mut().on_escape = Some(Rc::new(move || {
            mode.handle_editor_escape();
        }));
        let mode = Rc::clone(self);
        self.editor.borrow_mut().on_ctrl_d = Some(Rc::new(move || {
            mode.handle_ctrl_d();
        }));

        fn action(mode: &Rc<InteractiveMode>, handler: impl Fn(&InteractiveMode) + 'static) -> AppActionHandler {
            let mode = Rc::clone(mode);
            Rc::new(move || {
                handler(&mode);
                true
            })
        }

        self.editor
            .borrow_mut()
            .on_action("app.clear", action(self, |mode| mode.handle_ctrl_c()));
        self.editor.borrow_mut().on_action(
            "app.suspend",
            action(self, |mode| {
                mode.show_status("Suspend is not supported by the Rust TUI host.")
            }),
        );
        self.editor.borrow_mut().on_action(
            "app.thinking.cycle",
            action(self, InteractiveMode::cycle_thinking_level),
        );
        self.editor.borrow_mut().on_action(
            "app.model.cycleForward",
            action(self, |mode| {
                mode.actions
                    .borrow_mut()
                    .push_back(ModeAction::CycleModel(CycleDirection::Forward));
            }),
        );
        self.editor.borrow_mut().on_action(
            "app.model.cycleBackward",
            action(self, |mode| {
                mode.actions
                    .borrow_mut()
                    .push_back(ModeAction::CycleModel(CycleDirection::Backward));
            }),
        );
        self.editor.borrow_mut().on_action(
            "app.tools.expand",
            action(self, InteractiveMode::toggle_tool_output_expansion),
        );
        self.editor.borrow_mut().on_action(
            "app.thinking.toggle",
            action(self, InteractiveMode::toggle_thinking_block_visibility),
        );
        self.editor.borrow_mut().on_action(
            "app.editor.external",
            action(self, |mode| {
                mode.actions.borrow_mut().push_back(ModeAction::ExternalEditor);
            }),
        );
        self.editor.borrow_mut().on_action(
            "app.message.copy",
            action(self, |mode| {
                mode.actions.borrow_mut().push_back(ModeAction::Copy {
                    flash_confirmation: true,
                });
            }),
        );
        self.editor.borrow_mut().on_action(
            "app.message.followUp",
            action(self, |mode| {
                mode.actions.borrow_mut().push_back(ModeAction::FollowUp);
            }),
        );
        self.editor.borrow_mut().on_action(
            "app.message.dequeue",
            action(self, |mode| {
                mode.actions.borrow_mut().push_back(ModeAction::Dequeue);
            }),
        );
        self.editor.borrow_mut().on_action(
            "app.session.new",
            action(self, |mode| {
                mode.actions.borrow_mut().push_back(ModeAction::NewSession);
            }),
        );
        self.editor.borrow_mut().on_action(
            "app.session.tree",
            action(self, |mode| mode.show_status("Tree selector is not ported yet.")),
        );
        self.editor.borrow_mut().on_action(
            "app.session.fork",
            action(self, |mode| mode.show_status("Fork selector is not ported yet.")),
        );
        self.editor.borrow_mut().on_action(
            "app.session.resume",
            action(self, |mode| mode.show_status("Session selector is not ported yet.")),
        );
        self.editor.borrow_mut().on_action(
            "app.agent.select",
            action(self, |mode| mode.show_status("Agent switcher is not ported yet.")),
        );

        let mode = Rc::clone(self);
        self.ui.set_on_debug(Rc::new(move || {
            let mode = Rc::clone(&mode);
            tokio::task::spawn_local(async move {
                if let Err(error) = mode.session().prompt("/debug", PromptOptions::default()).await {
                    mode.show_error(error.message());
                }
            });
        }));
        self.update_editor_border_color();
    }

    /// Initializes the TUI, resources, extensions, and initial messages.
    pub async fn init(self: &Rc<Self>) -> Result<(), String> {
        if self.is_initialized.get() {
            return Ok(());
        }

        if crate::modes::interactive::theme::current_theme_if_initialized().is_none() {
            crate::modes::interactive::theme::init_theme(None, false);
        }
        let loader_themes = self.session.borrow().resource_loader().get_themes();
        let registered: Vec<Arc<Theme>> = loader_themes.themes.into_iter().map(Arc::new).collect();
        if !registered.is_empty() {
            let _ = crate::modes::interactive::theme::set_registered_themes(registered);
        }

        self.bind_callbacks();
        self.build_fullscreen_layout();
        self.render_widgets();
        self.mount_interactive_tui();
        let editor_ref = Rc::clone(&self.editor_handle);
        self.ui.set_focus(Some(editor_ref));
        self.ui.start().map_err(|error| error.to_string())?;
        self.is_initialized.set(true);
        self.update_terminal_current_directory();
        self.update_terminal_title();
        self.build_header();
        self.show_loaded_resources(false, false);
        self.bind_extensions().await;
        self.render_initial_messages();
        self.subscribe_to_agent();
        self.update_ui_shared();
        self.ui.request_render();
        Ok(())
    }

    fn update_ui_shared(&self) {
        let terminal = self.ui.terminal();
        let terminal = terminal.borrow();
        self.ui_shared
            .rows
            .store(terminal.rows(), std::sync::atomic::Ordering::Relaxed);
        self.ui_shared
            .columns
            .store(terminal.columns(), std::sync::atomic::Ordering::Relaxed);
        self.ui_shared.fullscreen.store(
            self.ui.mode() == TuiMode::Fullscreen,
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    fn build_header(&self) {
        let mut header = self.header_container.borrow_mut();
        header.clear();
        let quiet = self.settings(|settings| settings.get_quiet_startup());
        if !self.options.verbose && quiet {
            header.add_child(component(Text::new("", 0, 0)));
            return;
        }
        let logo = format!(
            "{} {}",
            with_theme(|theme| theme.bold(&theme.fg(ThemeColor::Accent, &crate::config::package_config().app_name))),
            with_theme(|theme| theme.fg(ThemeColor::Dim, &format!("v{}", self.version)))
        );
        let instructions = [
            "Esc to interrupt",
            "Ctrl+C to clear",
            "Ctrl+C twice to exit",
            "Ctrl+D to exit (empty)",
            "Shift+Tab to cycle thinking level",
            "Ctrl+P/Ctrl+N to cycle models",
            "Ctrl+L to select model",
            "Ctrl+O to expand tools",
            "Ctrl+T to expand thinking",
            "Ctrl+G for external editor",
            "/ for commands",
            "! to run bash",
            "!! to run bash (no context)",
            "Alt+Enter to queue follow-up",
            "Alt+Up to edit queued messages",
        ]
        .join("\n");
        let onboarding = with_theme(|theme| {
            theme.fg(
                ThemeColor::Dim,
                "Pi can explain its own features and look up its docs. Ask it how to use or extend Pi.",
            )
        });
        let collapsed = format!("{logo}\n{instructions}\n\n{onboarding}");
        let expanded = collapsed.clone();
        header.add_child(component(Spacer::new(1)));
        header.add_child(component(ExpandableText::new(
            move || collapsed.clone(),
            move || expanded.clone(),
            self.startup_expansion_state(),
            1,
            0,
        )));
        header.add_child(component(Spacer::new(1)));
    }

    async fn bind_extensions(self: &Rc<Self>) {
        let session = self.session();
        let ui_context = self.ui_context();
        let footer_data = Arc::clone(&self.footer_data_provider);
        let sender = self.ui_sender.clone();
        let abort_sender = sender.clone();
        let shutdown_sender = sender.clone();
        let actions = crate::core::extensions::types::ExtensionCommandContextActions {
            show_approval_selector: {
                let sender = sender.clone();
                Arc::new(move || {
                    let _ = sender.send(UiRequest::ShowApprovalSelector);
                    Ok(())
                })
            },
            show_sandbox_selector: {
                let sender = sender.clone();
                Arc::new(move || {
                    let _ = sender.send(UiRequest::ShowSandboxSelector);
                    Ok(())
                })
            },
            wait_for_idle: {
                let session = session.clone();
                Arc::new(move || {
                    let agent = session.agent().clone();
                    Box::pin(async move {
                        agent.wait_for_idle().await;
                    })
                })
            },
            new_session: {
                let sender = sender.clone();
                Arc::new(move |options| {
                    let (respond, receiver) = tokio::sync::oneshot::channel();
                    let _ = sender.send(UiRequest::RuntimeNewSession { options, respond });
                    Box::pin(async move {
                        receiver
                            .await
                            .map_err(|error| crate::core::extensions::types::HandlerError::new(error.to_string()))?
                            .map_err(crate::core::extensions::types::HandlerError::new)
                    })
                })
            },
            fork: {
                let sender = sender.clone();
                Arc::new(move |entry_id, options| {
                    let (respond, receiver) = tokio::sync::oneshot::channel();
                    let _ = sender.send(UiRequest::RuntimeFork {
                        entry_id,
                        options,
                        respond,
                    });
                    Box::pin(async move {
                        receiver
                            .await
                            .map_err(|error| crate::core::extensions::types::HandlerError::new(error.to_string()))?
                            .map_err(crate::core::extensions::types::HandlerError::new)
                    })
                })
            },
            navigate_tree: {
                let sender = sender.clone();
                Arc::new(move |entry_id, options| {
                    let (respond, receiver) = tokio::sync::oneshot::channel();
                    let _ = sender.send(UiRequest::RuntimeNavigateTree {
                        entry_id,
                        options,
                        respond,
                    });
                    Box::pin(async move {
                        receiver
                            .await
                            .map_err(|error| crate::core::extensions::types::HandlerError::new(error.to_string()))?
                            .map_err(crate::core::extensions::types::HandlerError::new)
                    })
                })
            },
            switch_session: {
                let sender = sender.clone();
                Arc::new(move |path, _options| {
                    let (respond, receiver) = tokio::sync::oneshot::channel();
                    let _ = sender.send(UiRequest::RuntimeSwitchSession { path, respond });
                    Box::pin(async move {
                        receiver
                            .await
                            .map_err(|error| crate::core::extensions::types::HandlerError::new(error.to_string()))?
                            .map_err(crate::core::extensions::types::HandlerError::new)
                    })
                })
            },
            reload: {
                let sender = sender.clone();
                Arc::new(move || {
                    let (respond, receiver) = tokio::sync::oneshot::channel();
                    let _ = sender.send(UiRequest::RuntimeReload { respond });
                    Box::pin(async move {
                        let _ = receiver.await;
                    })
                })
            },
            restart: {
                let sender = sender.clone();
                Arc::new(move |options| {
                    let (respond, receiver) = tokio::sync::oneshot::channel();
                    let _ = sender.send(UiRequest::RuntimeRestart { options, respond });
                    Box::pin(async move {
                        let _ = receiver.await;
                    })
                })
            },
        };

        session
            .bind_extensions(crate::core::agent_session::ExtensionBindings {
                ui_context: Some(ui_context),
                footer_data: Some(footer_data),
                mode: Some(crate::core::extensions::types::ExtensionMode::Tui),
                control_db_path: self.options.control_db_path.clone(),
                command_context_actions: Some(actions),
                abort_handler: Some(Arc::new(move || {
                    let _ = abort_sender.send(UiRequest::AbortAndRestoreQueued);
                })),
                shutdown_handler: Some(Arc::new(move || {
                    let _ = shutdown_sender.send(UiRequest::ShutdownRequested);
                })),
                on_error: None,
            })
            .await;
        self.footer.borrow_mut().invalidate();
        self.ui.request_render();
    }

    fn subscribe_to_agent(self: &Rc<Self>) {
        if let Some(id) = self.unsubscribe.get() {
            self.session.borrow().unsubscribe(id);
        }
        let Some(sender) = self.events.borrow().clone() else {
            return;
        };
        let id = self
            .session
            .borrow()
            .subscribe(Arc::new(move |event: &AgentSessionEvent| {
                let _ = sender.send(event.clone());
            }));
        self.unsubscribe.set(Some(id));
    }

    /// Runs the interactive loop until shutdown.
    pub async fn run(self: Rc<Self>) -> Result<(), String> {
        self.bind_callbacks();
        let (event_sender, mut event_receiver) = tokio::sync::mpsc::unbounded_channel();
        *self.events.borrow_mut() = Some(event_sender);
        self.init().await?;

        if let Some(message) = self.options.initial_message.clone() {
            let mode = Rc::clone(&self);
            let images = self.options.initial_images.clone();
            tokio::task::spawn_local(async move {
                if let Err(error) = mode
                    .session()
                    .prompt(
                        &message,
                        PromptOptions {
                            images,
                            ..PromptOptions::default()
                        },
                    )
                    .await
                {
                    mode.show_error(error.message());
                }
            });
        }
        for message in self.options.initial_messages.clone() {
            let mode = Rc::clone(&self);
            tokio::task::spawn_local(async move {
                if let Err(error) = mode.session().prompt(&message, PromptOptions::default()).await {
                    mode.show_error(error.message());
                }
            });
        }

        loop {
            tokio::select! {
                Some(event) = event_receiver.recv() => {
                    self.handle_event(event).await;
                }
                _ = tokio::time::sleep(Duration::from_millis(PARTIAL_UPDATE_RENDER_THROTTLE_MS)) => {}
            }
            self.ui.process_events();
            self.handle_ui_requests().await;
            self.drain_actions().await;
            if self.shutdown_requested.get() {
                break;
            }
        }

        self.shutdown().await;
        Ok(())
    }

    async fn drain_actions(self: &Rc<Self>) {
        loop {
            let action = self.actions.borrow_mut().pop_front();
            let Some(action) = action else {
                return;
            };
            match action {
                ModeAction::Submit(text) => {
                    let mode = Rc::clone(self);
                    tokio::task::spawn_local(async move {
                        mode.submit(text).await;
                    });
                }
                ModeAction::Shutdown => {
                    self.shutdown_requested.set(true);
                }
                ModeAction::ExternalEditor => {
                    let mode = Rc::clone(self);
                    tokio::task::spawn_local(async move {
                        mode.handle_open_external_editor().await;
                    });
                }
                ModeAction::FollowUp => self.handle_follow_up(),
                ModeAction::Dequeue => self.handle_dequeue(),
                ModeAction::NewSession => {
                    let mode = Rc::clone(self);
                    tokio::task::spawn_local(async move {
                        mode.handle_clear_command().await;
                    });
                }
                ModeAction::Copy { flash_confirmation } => {
                    let mode = Rc::clone(self);
                    tokio::task::spawn_local(async move {
                        mode.handle_copy_command(flash_confirmation).await;
                    });
                }
                ModeAction::CycleModel(direction) => {
                    let mode = Rc::clone(self);
                    tokio::task::spawn_local(async move {
                        mode.cycle_model(direction).await;
                    });
                }
            }
        }
    }

    async fn submit(&self, text: String) {
        let submitted_text = text.clone();
        let text = text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.close_response_complete_notification();
        if text.starts_with('/') {
            self.add_submitted_text_to_history(&text);
        }

        if let Some(command) = text.strip_prefix('/') {
            let (name, args) = match command.split_once(' ') {
                Some((name, args)) => (name, args.trim()),
                None => (command, ""),
            };
            let handled = self.handle_slash_command(name, args).await;
            if handled {
                self.clear_editor();
                return;
            }
        }

        if let Some(command) = text.strip_prefix('!') {
            let exclude_from_context = command.starts_with('!');
            let command = if exclude_from_context {
                command[1..].trim()
            } else {
                command.trim()
            };
            if !command.is_empty() {
                if self.session.borrow().is_bash_running() {
                    self.show_warning("A bash command is already running. Press Esc to cancel it first.");
                    self.editor.borrow_mut().set_text(&text);
                    return;
                }
                self.add_submitted_text_to_history(&text);
                self.handle_bash_command(command.to_owned(), exclude_from_context).await;
                self.is_bash_mode.set(false);
                self.update_editor_border_color();
                return;
            }
        }

        let session = self.session();
        if session.is_compacting() {
            if self.is_extension_command(&text) {
                self.add_submitted_text_to_history(&text);
                self.clear_editor();
                if let Err(error) = session.prompt(&text, PromptOptions::default()).await {
                    self.show_error(error.message());
                }
            } else {
                self.compaction_queued_messages
                    .borrow_mut()
                    .push(CompactionQueuedMessage { text, mode: "steer" });
            }
            return;
        }

        self.add_submitted_text_to_history(&text);
        self.clear_editor();
        if session.is_streaming() {
            if let Err(error) = session
                .prompt(
                    &text,
                    PromptOptions {
                        streaming_behavior: Some(StreamingBehavior::Steer),
                        ..PromptOptions::default()
                    },
                )
                .await
            {
                self.show_error(error.message());
            }
            self.update_pending_messages_display();
            self.ui.request_render();
            return;
        }

        if let Some(callback) = self.on_input_callback.borrow().clone() {
            callback(text);
            return;
        }
        if let Err(error) = session
            .prompt(
                &submitted_text,
                PromptOptions {
                    streaming_behavior: Some(StreamingBehavior::Steer),
                    ..PromptOptions::default()
                },
            )
            .await
        {
            self.show_error(error.message());
        }
    }

    fn is_extension_command(&self, text: &str) -> bool {
        let Some(command) = text.strip_prefix('/') else {
            return false;
        };
        let name = command.split_whitespace().next().unwrap_or_default();
        self.session.borrow().extension_runner().get_command(name).is_some()
    }

    async fn handle_slash_command(&self, name: &str, args: &str) -> bool {
        match name {
            "help" | "hotkeys" => {
                self.show_status("Esc interrupt · Ctrl+C clear · Ctrl+D exit · Ctrl+O tools · Ctrl+T thinking");
            }
            "settings" => {
                self.show_status("Settings selector is not ported yet; edit settings.json directly.");
            }
            "scoped-models" | "models" => {
                self.show_status("Scoped-models selector is not ported yet.");
            }
            "model" => {
                let search = (!args.is_empty()).then(|| args.to_owned());
                self.handle_model_command(search).await;
            }
            "thinking" => {
                self.show_thinking_selector();
            }
            "theme" => {
                self.show_theme_selector();
            }
            "export" => {
                self.handle_export_command(format!("/export {args}")).await;
            }
            "import" => {
                self.handle_import_command(format!("/import {args}")).await;
            }
            "share" => {
                self.handle_share_command().await;
            }
            "copy" => {
                self.handle_copy_command(false).await;
            }
            "cd" => {
                self.handle_cd_command(format!("/cd {args}")).await;
            }
            "name" => self.handle_name_command(args),
            "unname" => self.handle_unname_command(),
            "session" => self.handle_session_command(),
            "tools" => self.handle_tools_command(),
            "extensions" => self.handle_extensions_command(),
            "changelog" => self.handle_changelog_command(),
            "fork" => self.show_status("Fork selector is not ported yet."),
            "clone" => self.handle_clone_command().await,
            "tree" => self.show_status("Tree selector is not ported yet."),
            "trust" => self.show_status("Trust selector is not ported yet."),
            "login" => self.show_status("Provider login is not ported yet."),
            "logout" => self.show_status("Provider logout is not ported yet."),
            "new" => self.handle_clear_command().await,
            "continue" => {
                if self.session.borrow().is_streaming() {
                    self.cancel_streaming_and_submit_queued_messages().await;
                } else {
                    self.clear_editor();
                    if let Err(error) = self.session().continue_run().await {
                        self.show_error(error.message());
                    }
                }
            }
            "compact" => {
                let instructions = (!args.is_empty()).then(|| args.to_owned());
                self.handle_compact_command(instructions).await;
            }
            "reload" => self.handle_reload_command().await,
            "resume" => self.show_status("Session selector is not ported yet."),
            "agents" => self.show_status("Agent switcher is not ported yet."),
            "sandbox" => self.show_status("Sandbox selector is not ported yet."),
            "quit" => self.shutdown_requested.set(true),
            "arminsayshi" | "dementedelves" | "daxnuts" => {
                self.show_status("Easter egg commands are not ported.");
            }
            _ => {
                if self.session.borrow().extension_runner().get_command(name).is_some() {
                    let text = if args.is_empty() {
                        format!("/{name}")
                    } else {
                        format!("/{name} {args}")
                    };
                    self.clear_editor();
                    if let Err(error) = self.session().prompt(&text, PromptOptions::default()).await {
                        self.show_error(error.message());
                    }
                    return true;
                }
                let templates = self.session().prompt_templates();
                if let Some(template) = templates.into_iter().find(|template| template.name == name) {
                    let expanded =
                        crate::core::prompt_templates::expand_prompt_template(&format!("/{name} {args}"), &[template]);
                    self.clear_editor();
                    if let Err(error) = self.session().prompt(&expanded, PromptOptions::default()).await {
                        self.show_error(error.message());
                    }
                    return true;
                }
                return false;
            }
        }
        true
    }

    fn show_thinking_selector(&self) {
        let session = self.session();
        let current = session.thinking_level();
        let available = session.get_available_thinking_levels();
        let mode = self.ui_id.get();
        let on_select: Rc<dyn Fn(pi_agent_core::harness::types::ThinkingLevel)> = Rc::new(move |level| {
            if let Some(session) = with_mode(mode, |mode| mode.session()) {
                session.set_thinking_level(level);
            }
            with_mode(mode, |mode| mode.restore_editor());
        });
        let mode = self.ui_id.get();
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode, |mode| mode.restore_editor());
        });
        let selector = ThinkingSelectorComponent::new(current, &available, on_select, on_cancel);
        self.show_selector(component(selector));
    }

    fn show_theme_selector(&self) {
        let current = self
            .settings(|settings| settings.get_theme().map(str::to_owned))
            .unwrap_or_else(|| "dark".to_owned());
        let mode = self.ui_id.get();
        let on_select: Rc<dyn Fn(String)> = Rc::new(move |name| {
            with_mode(mode, |mode| {
                mode.set_theme_name(&name);
                mode.restore_editor();
            });
        });
        let mode = self.ui_id.get();
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode, |mode| mode.restore_editor());
        });
        let on_preview: Rc<dyn Fn(String)> = Rc::new(|_| {});
        let selector = crate::modes::interactive::components::theme_selector::ThemeSelectorComponent::new(
            &current, on_select, on_cancel, on_preview,
        );
        self.show_selector(component(selector));
    }

    fn show_selector(&self, selector: ComponentRef) {
        self.editor_container.borrow_mut().clear();
        self.editor_container.borrow_mut().add_child(Rc::clone(&selector));
        self.ui.set_focus(Some(selector));
        self.ui.request_render();
    }

    fn restore_editor(&self) {
        self.editor_container.borrow_mut().clear();
        self.editor_container
            .borrow_mut()
            .add_child(Rc::clone(&self.editor_handle));
        let editor_ref = Rc::clone(&self.editor_handle);
        self.ui.set_focus(Some(editor_ref));
        self.ui.request_render();
    }

    async fn handle_model_command(&self, search_term: Option<String>) {
        let session = self.session();
        if is_unknown_model(Some(&session.model())) {
            self.show_warning("The restored session model is unknown; select a model with /model.");
        }
        let scoped = session.scoped_models();
        let models: Vec<Model> = if scoped.is_empty() {
            session.model_runtime().get_available_snapshot().into_iter().collect()
        } else {
            scoped.into_iter().map(|model| model.model).collect()
        };
        if models.is_empty() {
            self.show_error("No models available");
            return;
        }
        if let Some(search) = search_term {
            let needle = search.to_lowercase();
            let matches: Vec<&Model> = models
                .iter()
                .filter(|model| {
                    model.id.to_lowercase().contains(&needle)
                        || model.provider.to_lowercase().contains(&needle)
                        || format!("{}/{}", model.provider, model.id)
                            .to_lowercase()
                            .contains(&needle)
                })
                .collect();
            if matches.len() == 1 {
                let model = matches[0].clone();
                if let Err(error) = session.set_model(model).await {
                    self.show_error(error.message());
                }
                return;
            }
        }
        let labels: Vec<String> = models
            .iter()
            .map(|model| format!("{}/{}", model.provider, model.id))
            .collect();
        let by_label: HashMap<String, Model> = models
            .iter()
            .map(|model| (format!("{}/{}", model.provider, model.id), model.clone()))
            .collect();
        let mode = self.ui_id.get();
        let on_select: Rc<dyn Fn(String)> = Rc::new(move |label| {
            let model = by_label.get(&label).cloned();
            if let Some(mode) = with_mode(mode, |mode| mode) {
                mode.restore_editor();
                if let Some(model) = model {
                    let mode = Rc::clone(&mode);
                    tokio::task::spawn_local(async move {
                        if let Err(error) = mode.session().set_model(model).await {
                            mode.show_error(error.message());
                        }
                    });
                }
            }
        });
        let mode = self.ui_id.get();
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode, |mode| mode.restore_editor());
        });
        let selector = ExtensionSelectorComponent::new(
            "Select model",
            labels,
            on_select,
            on_cancel,
            ExtensionSelectorOptions {
                request_render: Some(Rc::new({
                    let ui = Rc::clone(&self.ui);
                    move || ui.request_render()
                })),
                ..ExtensionSelectorOptions::default()
            },
        );
        self.show_selector(component(selector));
    }

    async fn cycle_model(&self, direction: CycleDirection) {
        if let Err(error) = self.session().cycle_model(direction).await {
            self.show_error(error.message());
        }
        self.footer.borrow_mut().invalidate();
        self.ui.request_render();
    }

    async fn handle_open_external_editor(&self) {
        let text = self.editor.borrow().get_text();
        let command = self.settings(|settings| settings.get_external_editor_command());
        let result = crate::modes::interactive::external_editor::edit_in_external_editor(
            &crate::modes::interactive::external_editor::ExternalEditorOptions { command, content: text },
        );
        match result {
            crate::modes::interactive::external_editor::ExternalEditorResult::Complete { content } => {
                self.editor.borrow_mut().set_text(&content);
                self.ui.request_render();
            }
            crate::modes::interactive::external_editor::ExternalEditorResult::Failed => {
                self.show_error("External editor failed");
            }
        }
    }

    fn handle_follow_up(&self) {
        let text = self.editor.borrow().get_text().trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.add_submitted_text_to_history(&text);
        self.clear_editor();
        let mode = self.ui_id.get();
        let session = self.session();
        tokio::task::spawn_local(async move {
            if let Err(error) = session.follow_up(&text, &[]).await
                && let Some(mode) = with_mode(mode, |mode| mode)
            {
                mode.show_error(error.message());
            }
        });
    }

    fn handle_dequeue(&self) {
        let restored = self.restore_queued_messages_to_editor(false);
        if restored > 0 {
            self.show_status(&format!("Restored {restored} queued message(s)"));
        }
    }

    fn restore_queued_messages_to_editor(&self, abort: bool) -> usize {
        if abort {
            let session = self.session();
            tokio::task::spawn_local(async move {
                let _ = session.abort().await;
            });
        }
        let (steering, follow_up) = self.session.borrow().clear_queue();
        let count = steering.len() + follow_up.len();
        if count == 0 {
            return 0;
        }
        let mut text = self.editor.borrow().get_text();
        for message in steering.into_iter().chain(follow_up) {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&message);
        }
        self.editor.borrow_mut().set_text(&text);
        self.update_pending_messages_display();
        self.ui.request_render();
        count
    }

    async fn cancel_streaming_and_submit_queued_messages(&self) {
        let restored = self.restore_queued_messages_to_editor(true);
        if restored > 0 {
            self.show_status("Cancelled streaming; queued messages restored to editor");
        }
    }

    async fn handle_bash_command(&self, command: String, exclude_from_context: bool) {
        let mode = self.ui_id.get();
        let session = self.session();
        let render: Rc<dyn Fn()> = Rc::new({
            let ui = Rc::clone(&self.ui);
            move || ui.request_render()
        });
        let bash = BashExecutionComponent::new(
            command.clone(),
            Rc::clone(&render),
            exclude_from_context,
            BashExecutionOptions::default(),
        );
        let bash_ref = component(bash);
        self.pending_messages_container
            .borrow_mut()
            .add_child(Rc::clone(&bash_ref));
        self.ui.request_render();
        let result = session
            .execute_bash(
                &command,
                None,
                crate::core::agent_session::ExecuteBashOptions {
                    exclude_from_context: Some(exclude_from_context),
                    id: None,
                    operations: None,
                },
            )
            .await;
        match result {
            Ok(result) => {
                if let Some(component) = bash_ref
                    .borrow_mut()
                    .as_any_mut()
                    .and_then(|any| any.downcast_mut::<BashExecutionComponent>())
                {
                    component.set_complete(
                        result.exit_code,
                        result.cancelled,
                        result
                            .truncated
                            .then(|| crate::core::tools::truncate::TruncationResult {
                                content: String::new(),
                                truncated: true,
                                truncated_by: None,
                                total_lines: 0,
                                total_bytes: 0,
                                output_lines: 0,
                                output_bytes: 0,
                                last_line_partial: false,
                                first_line_exceeds_limit: false,
                                max_lines: 0,
                                max_bytes: 0,
                            }),
                        result.full_output_path.clone(),
                    );
                }
                self.pending_messages_container.borrow_mut().clear();
                self.chat_container.borrow_mut().add_child(bash_ref);
                self.footer.borrow_mut().invalidate();
            }
            Err(error) => {
                self.pending_messages_container.borrow_mut().clear();
                self.show_error(error.message());
            }
        }
        if let Some(mode) = with_mode(mode, |mode| mode) {
            let _ = mode;
        }
        self.ui.request_render();
    }

    async fn handle_compact_command(&self, custom_instructions: Option<String>) {
        let session = self.session();
        let result = session.compact(custom_instructions.as_deref()).await;
        match result {
            Ok(result) => {
                self.rebuild_chat_after_compaction(&result);
                self.footer.borrow_mut().invalidate();
                self.show_status("Compaction complete");
            }
            Err(error) => self.show_error(error.message()),
        }
        self.flush_compaction_queue(false).await;
    }

    fn rebuild_chat_after_compaction(&self, _result: &crate::core::compaction::CompactionResult) {
        self.chat_container.borrow_mut().clear();
        self.render_initial_messages();
    }

    async fn flush_compaction_queue(&self, _will_retry: bool) {
        let queued = std::mem::take(&mut *self.compaction_queued_messages.borrow_mut());
        for message in queued {
            if let Err(error) = self
                .session()
                .prompt(
                    &message.text,
                    PromptOptions {
                        streaming_behavior: Some(if message.mode == "steer" {
                            StreamingBehavior::Steer
                        } else {
                            StreamingBehavior::FollowUp
                        }),
                        ..PromptOptions::default()
                    },
                )
                .await
            {
                self.show_error(error.message());
            }
        }
    }

    async fn handle_clear_command(&self) {
        let mut runtime = self.runtime.lock().await;
        match runtime
            .new_session(crate::core::agent_session_runtime::NewSessionOptions {
                parent_session: None,
                setup: None,
                with_session: None,
            })
            .await
        {
            Ok(result) if !result.cancelled => {
                let session = runtime.session().clone();
                drop(runtime);
                self.adopt_session(session);
                self.chat_container.borrow_mut().clear();
                self.loaded_resources_container.borrow_mut().clear();
                self.render_widgets();
                self.show_loaded_resources(false, false);
                self.ui.request_render();
            }
            Ok(_) => {}
            Err(error) => self.show_error(&error),
        }
    }

    fn adopt_session(&self, session: AgentSession) {
        *self.session.borrow_mut() = session.clone();
        self.footer.borrow_mut().set_session(Arc::new(session.clone()));
        self.footer_data_provider.set_cwd(session.cwd());
        if let Some(sender) = self.events.borrow().clone() {
            if let Some(id) = self.unsubscribe.get() {
                self.session.borrow().unsubscribe(id);
            }
            let id = session.subscribe(Arc::new(move |event: &AgentSessionEvent| {
                let _ = sender.send(event.clone());
            }));
            self.unsubscribe.set(Some(id));
        }
        self.update_terminal_title();
        self.ui.request_render();
    }

    async fn handle_reload_command(&self) {
        match self.session().reload(None).await {
            Ok(()) => {
                self.show_status("Reloaded");
                self.ui.request_render();
            }
            Err(error) => self.show_error(error.message()),
        }
    }

    async fn handle_export_command(&self, text: String) {
        let path = Self::path_command_argument(&text, "/export");
        match self.session().export_to_html(path.as_deref()).await {
            Ok(path) => self.show_status(&format!("Exported session to {path}")),
            Err(error) => self.show_error(error.message()),
        }
    }

    async fn handle_import_command(&self, text: String) {
        let Some(path) = Self::path_command_argument(&text, "/import") else {
            self.show_error("Usage: /import <path.jsonl>");
            return;
        };
        let mut runtime = self.runtime.lock().await;
        match runtime.import_from_jsonl(&path, None).await {
            Ok(_) => {
                let session = runtime.session().clone();
                drop(runtime);
                self.adopt_session(session);
                self.show_status("Session imported");
            }
            Err(error) => self.show_error(&error),
        }
    }

    fn path_command_argument(text: &str, command: &str) -> Option<String> {
        let rest = text.strip_prefix(command)?.trim();
        if rest.is_empty() { None } else { Some(rest.to_owned()) }
    }

    async fn handle_share_command(&self) {
        self.show_status("Share viewer is not ported yet.");
    }

    async fn handle_copy_command(&self, flash_confirmation: bool) {
        let Some(text) = self.session.borrow().get_last_assistant_text() else {
            self.show_error("No assistant message to copy");
            return;
        };
        match crate::utils::clipboard::copy_to_clipboard(
            &text,
            &crate::utils::clipboard_image::ClipboardEnvironment::from_process(),
            None,
        )
        .await
        {
            Ok(()) => {
                if flash_confirmation && self.ui.mode() == TuiMode::Fullscreen {
                    self.show_status("Copied!");
                } else {
                    self.show_status("Copied last agent message to clipboard");
                }
            }
            Err(error) => self.show_error(&format!("Failed to copy: {error}")),
        }
    }

    fn handle_name_command(&self, args: &str) {
        let name = args.trim();
        if name.is_empty() {
            self.show_error("Usage: /name <name>");
            return;
        }
        self.session.borrow().set_session_name(name);
        self.update_terminal_title();
        self.ui.request_render();
    }

    fn handle_unname_command(&self) {
        self.session.borrow().clear_session_name();
        self.update_terminal_title();
        self.ui.request_render();
    }

    fn handle_session_command(&self) {
        let stats = self.session.borrow().get_session_stats();
        let mut info = format!(
            "Session: {}\nFile: {}\nMessages: {}\nUser: {}\nAssistant: {}\nTool calls: {}\nTool results: {}",
            stats.session_id,
            stats.session_file.as_deref().unwrap_or("(not persisted)"),
            stats.total_messages,
            stats.user_messages,
            stats.assistant_messages,
            stats.tool_calls,
            stats.tool_results
        );
        info.push('\n');
        info.push_str(&format_session_token_info(&stats.tokens));
        info.push_str(&format_session_cost_info(
            stats.cost,
            &[],
            &crate::core::cache_stats::CacheWasteTotals {
                missed_tokens: 0.0,
                missed_cost: 0.0,
                miss_count: 0,
            },
        ));
        self.show_status(&info);
    }

    fn handle_tools_command(&self) {
        let tools = self.session.borrow().get_all_tools();
        let active = self.session.borrow().get_active_tool_names();
        let lines: Vec<String> = tools
            .iter()
            .map(|tool| {
                let marker = if active.contains(&tool.name) { "●" } else { "○" };
                format!("{marker} {}", tool.name)
            })
            .collect();
        self.show_status(&format!("Tools:\n{}", lines.join("\n")));
    }

    fn handle_extensions_command(&self) {
        let extensions = self.session.borrow().resource_loader().get_extensions();
        let lines: Vec<String> = extensions
            .extensions
            .iter()
            .map(|extension| extension.path.clone())
            .collect();
        self.show_status(&format!("Extensions:\n{}", lines.join("\n")));
    }

    fn handle_changelog_command(&self) {
        match self.changelog_markdown.borrow().clone() {
            Some(markdown) => {
                self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
                self.chat_container.borrow_mut().add_child(component(
                    pi_tui::components::markdown::Markdown::with_shared_theme(
                        markdown,
                        1,
                        0,
                        self.markdown_theme(),
                        None,
                        pi_tui::components::markdown::MarkdownOptions::default(),
                    ),
                ));
                self.ui.request_render();
            }
            None => self.show_status("No changelog available"),
        }
    }

    async fn handle_cd_command(&self, text: String) {
        let Some(path) = Self::path_command_argument(&text, "/cd") else {
            self.show_error("Usage: /cd <path>");
            return;
        };
        let mut runtime = self.runtime.lock().await;
        match runtime.relocate(&path).await {
            Ok(()) => {
                let session = runtime.session().clone();
                drop(runtime);
                self.adopt_session(session);
                self.show_status(&format!("Changed directory to {path}"));
            }
            Err(error) => self.show_error(&error),
        }
    }

    async fn handle_clone_command(&self) {
        let leaf_id = self.session.borrow().session_manager().get_leaf_id().map(str::to_owned);
        let Some(leaf_id) = leaf_id else {
            self.show_error("No active entry to clone");
            return;
        };
        let mut runtime = self.runtime.lock().await;
        match runtime
            .fork(&leaf_id, crate::core::agent_session_runtime::ForkPosition::At, None)
            .await
        {
            Ok(result) if !result.cancelled => {
                let session = runtime.session().clone();
                drop(runtime);
                self.adopt_session(session);
                self.chat_container.borrow_mut().clear();
                self.render_initial_messages();
                self.show_status("Cloned session");
            }
            Ok(_) => {}
            Err(error) => self.show_error(&error),
        }
    }

    async fn shutdown(&self) {
        self.clear_extension_widgets();
        self.close_response_complete_notification();
        if let Some(id) = self.unsubscribe.get() {
            self.session.borrow().unsubscribe(id);
            self.unsubscribe.set(None);
        }
        self.stop();
        self.runtime.lock().await.dispose().await;
    }

    async fn handle_ui_requests(&self) {
        loop {
            let request = {
                let mut slot = self.ui_requests.borrow_mut();
                let Some(receiver) = slot.as_mut() else {
                    return;
                };
                receiver.try_recv()
            };
            match request {
                Ok(request) => self.handle_ui_request(request).await,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
            }
        }
    }

    async fn handle_ui_request(&self, request: UiRequest) {
        match request {
            UiRequest::Select {
                title,
                options,
                timeout,
                respond,
            } => {
                let respond = Rc::new(RefCell::new(Some(respond)));
                let select_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_select: Rc<dyn Fn(String)> = Rc::new(move |option| {
                    if let Some(respond) = select_respond.borrow_mut().take() {
                        let _ = respond.send(Some(option));
                    }
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let cancel_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    if let Some(respond) = cancel_respond.borrow_mut().take() {
                        let _ = respond.send(None);
                    }
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let selector = ExtensionSelectorComponent::new(
                    &title,
                    options,
                    on_select,
                    on_cancel,
                    ExtensionSelectorOptions {
                        request_render: Some(Rc::new({
                            let ui = Rc::clone(&self.ui);
                            move || ui.request_render()
                        })),
                        timeout: timeout.map(|timeout| timeout as u64),
                        on_toggle_tools_expanded: Some(Rc::new({
                            let mode = self.ui_id.get();
                            move || {
                                with_mode(mode, |mode| mode.toggle_tool_output_expansion());
                            }
                        })),
                    },
                );
                self.show_selector(component(selector));
            }
            UiRequest::Confirm {
                title,
                message,
                timeout,
                respond,
            } => {
                let respond = Rc::new(RefCell::new(Some(respond)));
                let yes_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_select: Rc<dyn Fn(String)> = Rc::new(move |option| {
                    if let Some(respond) = yes_respond.borrow_mut().take() {
                        let _ = respond.send(option == "Yes");
                    }
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let cancel_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    if let Some(respond) = cancel_respond.borrow_mut().take() {
                        let _ = respond.send(false);
                    }
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let selector = ExtensionSelectorComponent::new(
                    &format!("{title}\n{message}"),
                    vec!["Yes".to_owned(), "No".to_owned()],
                    on_select,
                    on_cancel,
                    ExtensionSelectorOptions {
                        request_render: Some(Rc::new({
                            let ui = Rc::clone(&self.ui);
                            move || ui.request_render()
                        })),
                        timeout: timeout.map(|timeout| timeout as u64),
                        on_toggle_tools_expanded: None,
                    },
                );
                self.show_selector(component(selector));
            }
            UiRequest::Input {
                title,
                placeholder,
                timeout,
                secret,
                respond,
            } => {
                let respond = Rc::new(RefCell::new(Some(respond)));
                let submit_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_submit: Rc<dyn Fn(String)> = Rc::new(move |value| {
                    if let Some(respond) = submit_respond.borrow_mut().take() {
                        let _ = respond.send(Some(value));
                    }
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let cancel_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    if let Some(respond) = cancel_respond.borrow_mut().take() {
                        let _ = respond.send(None);
                    }
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let input = ExtensionInputComponent::new(
                    &title,
                    placeholder.as_deref(),
                    on_submit,
                    on_cancel,
                    ExtensionInputOptions {
                        request_render: Some(Rc::new({
                            let ui = Rc::clone(&self.ui);
                            move || ui.request_render()
                        })),
                        timeout: timeout.map(|timeout| timeout as u64),
                        secret,
                    },
                );
                self.show_selector(component(input));
            }
            UiRequest::Editor {
                title,
                prefill,
                respond,
            } => {
                let respond = Rc::new(RefCell::new(Some(respond)));
                let submit_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_submit: Rc<dyn Fn(String)> = Rc::new(move |value| {
                    if let Some(respond) = submit_respond.borrow_mut().take() {
                        let _ = respond.send(Some(value));
                    }
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let cancel_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    if let Some(respond) = cancel_respond.borrow_mut().take() {
                        let _ = respond.send(None);
                    }
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let editor = ExtensionEditorComponent::new(
                    Rc::new(InteractiveTuiHost(Rc::clone(&self.ui))),
                    self.keybindings.clone(),
                    &title,
                    prefill.as_deref(),
                    on_submit,
                    on_cancel,
                    EditorOptions::default(),
                    Some(self.settings(|settings| settings.get_external_editor_command())),
                );
                self.show_selector(component(editor));
            }
            UiRequest::Notify { message, notify_type } => match notify_type {
                Some(NotifyType::Error) => self.show_error(&message),
                Some(NotifyType::Warning) => self.show_warning(&message),
                Some(NotifyType::Info) | None => self.show_status(&message),
            },
            UiRequest::RequestRender => self.ui.request_render(),
            UiRequest::SetStatus { key, text } => {
                if let Some(text) = &text {
                    self.extension_statuses.borrow_mut().insert(key.clone(), text.clone());
                } else {
                    self.extension_statuses.borrow_mut().shift_remove(&key);
                }
                self.footer_data_provider.set_extension_status(&key, text.as_deref());
                self.footer.borrow_mut().invalidate();
                self.ui.request_render();
            }
            UiRequest::SetWorkingMessage(message) => {
                *self.working_message.borrow_mut() = message;
                self.ui.request_render();
            }
            UiRequest::SetWorkingVisible(visible) => {
                self.working_visible.set(visible);
                self.ui.request_render();
            }
            UiRequest::SetWorkingIndicator(options) => {
                *self.working_indicator_options.borrow_mut() = options;
                self.ui.request_render();
            }
            UiRequest::SetHiddenThinkingLabel(label) => {
                *self.hidden_thinking_label.borrow_mut() = label.unwrap_or_else(|| "Thinking...".to_owned());
                self.ui.request_render();
            }
            UiRequest::SetWidget {
                key,
                content,
                placement,
            } => self.add_extension_widget(&key, content, placement),
            UiRequest::SetTitle(title) => self.ui.terminal().borrow_mut().set_title(&title),
            UiRequest::PasteToEditor(text) => {
                self.editor
                    .borrow_mut()
                    .handle_input(&format!("\x1b[200~{text}\x1b[201~"));
                self.ui.request_render();
            }
            UiRequest::SetEditorText(text) => {
                self.editor.borrow_mut().set_text(&text);
                self.ui.request_render();
            }
            UiRequest::SetToolsExpanded(expanded) => self.set_tools_expanded(expanded),
            UiRequest::SetFooter(factory) => {
                self.set_extension_footer(factory);
            }
            UiRequest::SetDefaultFooter(factory) => {
                self.set_default_extension_footer(factory);
            }
            UiRequest::SetHeader(factory) => self.set_extension_header(factory),
            UiRequest::Custom {
                factory,
                options,
                respond,
            } => self.show_extension_custom(factory, options, respond),
            UiRequest::AddTerminalInput { handler, respond } => {
                let handler = Arc::new(handler);
                let id = self.ui.add_input_listener(Rc::new(move |data: &str| {
                    let result = handler(data);
                    match result {
                        Some(result) => TuiInputListenerResult {
                            consume: result.consume.unwrap_or(false),
                            data: result.data,
                        },
                        None => TuiInputListenerResult::default(),
                    }
                }));
                let _ = respond.send(id);
            }
            UiRequest::RemoveTerminalInput(id) => self.ui.remove_input_listener(id),
            UiRequest::AbortAndRestoreQueued => {
                self.restore_queued_messages_to_editor(true);
            }
            UiRequest::ShutdownRequested => {
                self.shutdown_requested.set(true);
            }
            UiRequest::ShowApprovalSelector => {
                self.show_status("Approval selector is not ported yet.");
            }
            UiRequest::ShowSandboxSelector => {
                self.show_status("Sandbox selector is not ported yet.");
            }
            UiRequest::RuntimeNewSession { options, respond } => {
                let options = options.map(|options| {
                    let setup = options.setup.map(|setup| {
                        Arc::new(move |manager: &mut crate::core::session_manager::SessionManager| {
                            let future = setup(manager);
                            Box::pin(async move {
                                let _ = future.await;
                            }) as pi_ai::request::BoxFuture<'static, ()>
                        }) as crate::core::agent_session_runtime::SessionSetupCallback
                    });
                    let with_session = options.with_session.map(|callback| {
                        Arc::new(move |context: crate::core::extensions::types::ReplacedSessionContext| {
                            let future = callback(context);
                            Box::pin(async move {
                                let _ = future.await;
                            }) as pi_ai::request::BoxFuture<'static, ()>
                        }) as crate::core::agent_session_runtime::ReplacedSessionCallback
                    });
                    crate::core::agent_session_runtime::NewSessionOptions {
                        parent_session: options.parent_session,
                        setup,
                        with_session,
                    }
                });
                let mut runtime = self.runtime.lock().await;
                let result = runtime
                    .new_session(
                        options.unwrap_or(crate::core::agent_session_runtime::NewSessionOptions {
                            parent_session: None,
                            setup: None,
                            with_session: None,
                        }),
                    )
                    .await;
                if let Ok(result) = &result
                    && !result.cancelled
                {
                    let session = runtime.session().clone();
                    self.adopt_session(session);
                }
                let transition = result.map(|result| crate::core::extensions::types::SessionTransitionResult {
                    cancelled: result.cancelled,
                });
                let _ = respond.send(transition);
            }
            UiRequest::RuntimeFork {
                entry_id,
                options,
                respond,
            } => {
                let position = options
                    .as_ref()
                    .and_then(|options| options.position)
                    .map(|position| match position {
                        crate::core::extensions::types::ForkPosition::Before => {
                            crate::core::agent_session_runtime::ForkPosition::Before
                        }
                        crate::core::extensions::types::ForkPosition::At => {
                            crate::core::agent_session_runtime::ForkPosition::At
                        }
                    })
                    .unwrap_or(crate::core::agent_session_runtime::ForkPosition::At);
                let mut runtime = self.runtime.lock().await;
                let result = runtime.fork(&entry_id, position, None).await;
                if let Ok(result) = &result
                    && !result.cancelled
                {
                    let session = runtime.session().clone();
                    self.adopt_session(session);
                }
                let transition = result.map(|result| crate::core::extensions::types::SessionTransitionResult {
                    cancelled: result.cancelled,
                });
                let _ = respond.send(transition);
            }
            UiRequest::RuntimeNavigateTree {
                entry_id,
                options,
                respond,
            } => {
                let result = self
                    .session()
                    .navigate_tree(
                        &entry_id,
                        crate::core::agent_session::NavigateTreeOptions {
                            summarize: options.as_ref().and_then(|options| options.summarize),
                            custom_instructions: options
                                .as_ref()
                                .and_then(|options| options.custom_instructions.clone()),
                            replace_instructions: options.as_ref().and_then(|options| options.replace_instructions),
                            label: options.as_ref().and_then(|options| options.label.clone()),
                        },
                    )
                    .await;
                let transition = result
                    .map(|_| crate::core::extensions::types::SessionTransitionResult { cancelled: false })
                    .map_err(|error| error.message().to_owned());
                let _ = respond.send(transition);
            }
            UiRequest::RuntimeSwitchSession { path, respond } => {
                let mut runtime = self.runtime.lock().await;
                let result = runtime.switch_session(&path, None, None).await;
                if let Ok(result) = &result
                    && !result.cancelled
                {
                    let session = runtime.session().clone();
                    self.adopt_session(session);
                }
                let transition = result.map(|result| crate::core::extensions::types::SessionTransitionResult {
                    cancelled: result.cancelled,
                });
                let _ = respond.send(transition);
            }
            UiRequest::RuntimeReload { respond } => {
                let _ = self.session().reload(None).await;
                let _ = respond.send(());
            }
            UiRequest::RuntimeRestart { options, respond } => {
                let mut runtime = self.runtime.lock().await;
                let _ = runtime
                    .restart(
                        options.as_ref().and_then(|options| options.notice.as_deref()),
                        options.as_ref().and_then(|options| options.process).unwrap_or(false),
                    )
                    .await;
                let _ = respond.send(());
            }
        }
    }

    fn set_extension_footer(&self, factory: Option<ExtensionUiFooterFactory>) {
        self.custom_footer.borrow_mut().take();
        match factory {
            Some(factory) => {
                let component_ref = self.build_footer_component(&factory);
                *self.custom_footer.borrow_mut() = Some(Rc::clone(&component_ref));
                self.footer_container.borrow_mut().clear();
                self.footer_container.borrow_mut().add_child(component_ref);
            }
            None => self.restore_footer(),
        }
        self.ui.request_render();
    }

    fn set_default_extension_footer(&self, factory: Option<ExtensionUiFooterFactory>) {
        self.default_extension_footer.borrow_mut().take();
        if let Some(factory) = factory {
            let component_ref = self.build_footer_component(&factory);
            *self.default_extension_footer.borrow_mut() = Some(component_ref);
        }
        if self.custom_footer.borrow().is_none() {
            self.restore_footer();
        }
        self.ui.request_render();
    }

    fn restore_footer(&self) {
        self.footer_container.borrow_mut().clear();
        if let Some(default) = self.default_extension_footer.borrow().clone() {
            self.footer_container.borrow_mut().add_child(default);
        } else {
            self.footer_container
                .borrow_mut()
                .add_child(component(FooterHandle(Rc::clone(&self.footer))));
        }
    }

    fn build_footer_component(&self, factory: &ExtensionUiFooterFactory) -> ComponentRef {
        let context = crate::core::extensions::types::ExtensionUiFooterContext {
            tui: ExtensionUiTui::new(Arc::new(InteractiveUiTuiBackend {
                shared: Arc::clone(&self.ui_shared),
                sender: self.ui_sender.clone(),
            })),
            theme: crate::modes::interactive::theme::current_theme(),
            footer_data: (*self.footer_data_provider).clone(),
        };
        component(ExtensionComponentAdapter {
            inner: factory(context),
        })
    }

    fn set_extension_header(&self, factory: Option<ExtensionUiComponentFactory>) {
        self.custom_header.borrow_mut().take();
        self.header_container.borrow_mut().clear();
        match factory {
            Some(factory) => {
                let context = crate::core::extensions::types::ExtensionUiFactoryContext {
                    tui: ExtensionUiTui::new(Arc::new(InteractiveUiTuiBackend {
                        shared: Arc::clone(&self.ui_shared),
                        sender: self.ui_sender.clone(),
                    })),
                    theme: crate::modes::interactive::theme::current_theme(),
                    keybindings: self.keybindings.clone(),
                };
                let component_ref = component(ExtensionComponentAdapter {
                    inner: factory(context),
                });
                *self.custom_header.borrow_mut() = Some(Rc::clone(&component_ref));
                self.header_container.borrow_mut().add_child(component_ref);
            }
            None => self.build_header(),
        }
        self.ui.request_render();
    }

    fn show_extension_custom(
        &self,
        factory: ExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
        respond: tokio::sync::oneshot::Sender<Option<Value>>,
    ) {
        let context = crate::core::extensions::types::ExtensionUiFactoryContext {
            tui: ExtensionUiTui::new(Arc::new(InteractiveUiTuiBackend {
                shared: Arc::clone(&self.ui_shared),
                sender: self.ui_sender.clone(),
            })),
            theme: crate::modes::interactive::theme::current_theme(),
            keybindings: self.keybindings.clone(),
        };
        let respond = Arc::new(std::sync::Mutex::new(Some(respond)));
        let done_respond = Arc::clone(&respond);
        let mode = self.ui_id.get();
        let overlay = options.as_ref().and_then(|options| options.overlay).unwrap_or(false);
        let done = ExtensionUiDone::new(Arc::new(move |value| {
            let respond = done_respond
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if let Some(respond) = respond {
                let _ = respond.send(value);
            }
            if !overlay {
                with_mode(mode, |mode| mode.restore_editor());
            }
        }));
        let inner = factory(context, done);
        let component_ref = component(ExtensionComponentAdapter { inner });
        if overlay {
            self.ui.show_overlay(component_ref, None);
        } else {
            self.show_selector(component_ref);
        }
    }

    async fn handle_event(&self, event: AgentSessionEvent) {
        match event {
            AgentSessionEvent::Agent(AgentEvent::AgentStart) => {
                self.close_response_complete_notification();
                self.clear_pending_tools();
                self.executing_tool_names.borrow_mut().clear();
                self.executing_tool_started_at.borrow_mut().clear();
                self.ui.request_render();
            }
            AgentSessionEvent::Agent(AgentEvent::ModelRequestStart) => {
                self.ui.request_render();
            }
            AgentSessionEvent::Agent(AgentEvent::ModelRequestEnd) => {
                self.ui.request_render();
            }
            AgentSessionEvent::Agent(AgentEvent::MessageStart { message }) => match &message {
                AgentMessage::Assistant(assistant) => {
                    let mut component_value = AssistantMessageComponent::new(
                        None,
                        self.hide_thinking_block.get(),
                        Some(self.markdown_theme()),
                        Some(self.hidden_thinking_label.borrow().clone()),
                        self.output_pad(),
                        self.markdown_transformers(),
                    );
                    component_value.update_content((**assistant).clone(), true);
                    let component_ref = component(component_value);
                    self.chat_container.borrow_mut().add_child(Rc::clone(&component_ref));
                    *self.streaming_component.borrow_mut() = Some(component_ref);
                    *self.streaming_message.borrow_mut() = Some((**assistant).clone());
                    self.ui.request_render();
                }
                AgentMessage::Custom(_) => {
                    self.add_message_to_chat(&message);
                    self.ui.request_render();
                }
                AgentMessage::User(_) => {
                    self.add_message_to_chat(&message);
                    self.update_pending_messages_display();
                    self.ui.request_render();
                }
                AgentMessage::ToolResult(_) => {}
            },
            AgentSessionEvent::Agent(AgentEvent::MessageUpdate {
                message,
                assistant_message_event,
            }) => {
                let visible = match &*assistant_message_event {
                    pi_ai::types::AssistantMessageEvent::TextDelta { delta, .. } => !delta.is_empty(),
                    pi_ai::types::AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                        !self.hide_thinking_block.get() && !delta.is_empty()
                    }
                    _ => false,
                };
                if !visible {
                    return;
                }
                let streaming = self.streaming_component.borrow().clone();
                if let (Some(streaming), AgentMessage::Assistant(assistant)) = (streaming, &message) {
                    *self.streaming_message.borrow_mut() = Some((**assistant).clone());
                    if let Some(component_value) = streaming
                        .borrow_mut()
                        .as_any_mut()
                        .and_then(|any| any.downcast_mut::<AssistantMessageComponent>())
                    {
                        component_value.update_content((**assistant).clone(), true);
                    }
                    self.ui.request_render();
                }
            }
            AgentSessionEvent::Agent(AgentEvent::MessageEnd { message }) => {
                if let AgentMessage::Assistant(assistant) = &message {
                    let streaming = self.streaming_component.borrow_mut().take();
                    if let Some(streaming) = streaming {
                        if let Some(component_value) = streaming
                            .borrow_mut()
                            .as_any_mut()
                            .and_then(|any| any.downcast_mut::<AssistantMessageComponent>())
                        {
                            component_value.update_content((**assistant).clone(), false);
                        }
                        self.chat_container.borrow_mut().add_child(streaming);
                    }
                    for content in &assistant.content {
                        if let pi_ai::types::AssistantContent::ToolCall(call) = content {
                            self.ensure_tool_component(&call.name, &call.id, Value::Object(call.arguments.clone()));
                        }
                    }
                    *self.streaming_message.borrow_mut() = None;
                }
                self.footer.borrow_mut().invalidate();
                self.ui.request_render();
            }
            AgentSessionEvent::Agent(AgentEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
                ..
            }) => {
                self.executing_tool_names
                    .borrow_mut()
                    .insert(tool_call_id.clone(), tool_name.clone());
                self.ensure_tool_component(&tool_name, &tool_call_id, args);
                self.ui.request_render();
            }
            AgentSessionEvent::Agent(AgentEvent::ToolExecutionUpdate {
                tool_call_id,
                partial_result,
                ..
            }) => {
                let component_ref = self.pending_tools.borrow().get(&tool_call_id).cloned();
                if let Some(component_ref) = component_ref
                    && let Some(component_value) = component_ref
                        .borrow_mut()
                        .as_any_mut()
                        .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                {
                    component_value.update_result(
                        ToolExecutionResult {
                            content: partial_result.content.clone(),
                            details: partial_result.details.clone(),
                            is_error: false,
                        },
                        true,
                        None,
                    );
                }
            }
            AgentSessionEvent::Agent(AgentEvent::ToolExecutionEnd {
                tool_call_id,
                result,
                is_error,
                ..
            }) => {
                let component_ref = self.pending_tools.borrow_mut().shift_remove(&tool_call_id);
                if let Some(component_ref) = component_ref
                    && let Some(component_value) = component_ref
                        .borrow_mut()
                        .as_any_mut()
                        .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                {
                    component_value.update_result(
                        ToolExecutionResult {
                            content: result.content.clone(),
                            details: result.details.clone(),
                            is_error,
                        },
                        false,
                        None,
                    );
                }
                self.executing_tool_names.borrow_mut().shift_remove(&tool_call_id);
                self.executing_tool_started_at.borrow_mut().shift_remove(&tool_call_id);
                self.ui.request_render();
            }
            AgentSessionEvent::AgentEnd(event) => {
                self.clear_pending_tools();
                self.executing_tool_names.borrow_mut().clear();
                self.executing_tool_started_at.borrow_mut().clear();
                self.ui.request_render();
                if !event.will_retry {
                    self.notify_response_complete();
                }
            }
            AgentSessionEvent::QueueUpdate(_) => {
                self.update_pending_messages_display();
                self.ui.request_render();
            }
            AgentSessionEvent::SteeringMessageQueued(_) => {}
            AgentSessionEvent::AgentSettled(_) => {}
            AgentSessionEvent::CompactionStart(event) => {
                self.status_container.borrow_mut().clear();
                self.status_container.borrow_mut().add_child(component(Text::new(
                    with_theme(|theme| theme.fg(ThemeColor::Muted, &format!("Compacting ({})...", event.reason))),
                    1,
                    0,
                )));
                self.ui.request_render();
            }
            AgentSessionEvent::CompactionEnd(event) => {
                self.status_container.borrow_mut().clear();
                if event.aborted {
                    if event.reason == "manual" {
                        self.show_error("Compaction cancelled");
                    } else {
                        self.show_status("Auto-compaction cancelled");
                    }
                } else if let Some(result) = &event.result {
                    if let Ok(result) =
                        serde_json::from_value::<crate::core::compaction::CompactionResult>(result.clone())
                    {
                        self.rebuild_chat_after_compaction(&result);
                    }
                    self.footer.borrow_mut().invalidate();
                    self.show_status("Compaction complete");
                } else if let Some(error) = &event.error_message {
                    self.show_error(error);
                }
                self.flush_compaction_queue(event.will_retry).await;
                self.ui.request_render();
            }
            AgentSessionEvent::AutoRetryStart(event) => {
                self.status_container.borrow_mut().clear();
                self.status_container.borrow_mut().add_child(component(Text::new(
                    with_theme(|theme| {
                        theme.fg(
                            ThemeColor::Warning,
                            &format!(
                                "Retrying ({}/{}) in {}s... (Esc to cancel)",
                                event.attempt,
                                event.max_attempts,
                                (event.delay_ms / 1000.0).ceil() as u64
                            ),
                        )
                    }),
                    1,
                    0,
                )));
                self.ui.request_render();
            }
            AgentSessionEvent::AutoRetryEnd(event) => {
                self.status_container.borrow_mut().clear();
                if !event.success
                    && let Some(error) = &event.final_error
                {
                    self.show_error(error);
                }
                self.ui.request_render();
            }
            AgentSessionEvent::ProviderStreamRetry(event) => {
                let retry = &event.retry;
                self.chat_container.borrow_mut().add_child(component(Text::new(
                    with_theme(|theme| theme.fg(ThemeColor::Warning, &format!("provider retry: {}", retry.reason))),
                    1,
                    0,
                )));
                self.ui.request_render();
            }
            AgentSessionEvent::BashMessagesCommitted(_) => {
                self.footer.borrow_mut().invalidate();
                self.ui.request_render();
            }
            AgentSessionEvent::SessionInfoChanged(_) => {
                self.update_terminal_title();
                self.footer.borrow_mut().invalidate();
                self.ui.request_render();
            }
            AgentSessionEvent::ThinkingLevelChanged(_) => {
                self.footer.borrow_mut().invalidate();
                self.update_editor_border_color();
            }
            AgentSessionEvent::EntryAppended(_) => {}
            AgentSessionEvent::BashExecutionUpdate(_) => {}
            AgentSessionEvent::Agent(AgentEvent::AgentEnd { .. }) => {}
            AgentSessionEvent::Agent(AgentEvent::TurnStart) => {}
            AgentSessionEvent::Agent(AgentEvent::TurnEnd { .. }) => {}
            AgentSessionEvent::Message(_) => {}
        }
    }

    fn ensure_tool_component(&self, tool_name: &str, tool_call_id: &str, args: Value) {
        if let Some(existing) = self.pending_tools.borrow().get(tool_call_id).cloned() {
            if let Some(component_value) = existing
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
            {
                component_value.update_args(args);
            }
            return;
        }
        let request_render: Arc<dyn Fn() + Send + Sync> = {
            let sender = self.ui_sender.clone();
            Arc::new(move || {
                let _ = sender.send(UiRequest::RequestRender);
            })
        };
        let definition = self.session.borrow().extension_runner().get_tool_definition(tool_name);
        let mut component_value = ToolExecutionComponent::new(
            tool_name,
            tool_call_id,
            args,
            ToolExecutionOptions {
                show_images: Some(self.settings(|settings| settings.get_show_images())),
                image_width_cells: Some(self.settings(|settings| settings.get_image_width_cells()) as usize),
            },
            definition,
            request_render,
            self.cwd(),
        );
        component_value.set_expanded(self.tool_output_expanded.get());
        let component_ref = component(component_value);
        self.chat_container.borrow_mut().add_child(Rc::clone(&component_ref));
        self.pending_tools
            .borrow_mut()
            .insert(tool_call_id.to_owned(), component_ref);
    }

    fn clear_pending_tools(&self) {
        self.pending_tools.borrow_mut().clear();
    }

    fn notify_response_complete(&self) {
        self.close_response_complete_notification();
        let notification = crate::core::desktop_notification::send_desktop_notification(
            &crate::core::desktop_notification::DesktopNotification {
                title: "Pi response complete".to_owned(),
                body: "Pi is idle and ready for your next message.".to_owned(),
                urgency: Some(crate::core::desktop_notification::DesktopNotificationUrgency::Normal),
                expire_time_ms: Some(crate::core::desktop_notification::PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS),
            },
        );
        *self.response_complete_notification.borrow_mut() = notification;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_dark_theme() {
        let theme = crate::modes::interactive::theme::load_theme_from_path(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("theme").join("dark.json"),
            None,
        )
        .expect("dark theme");
        crate::modes::interactive::theme::set_theme_instance(std::sync::Arc::new(theme));
    }

    fn source_info(path: &str, source: &str, scope: SourceScope, base_dir: Option<&str>) -> SourceInfo {
        SourceInfo {
            path: path.to_owned(),
            source: source.to_owned(),
            scope,
            origin: crate::core::source_info::SourceOrigin::TopLevel,
            base_dir: base_dir.map(str::to_owned),
        }
    }

    #[test]
    fn quote_if_needed_matches_shell_safe_rule() {
        assert_eq!(quote_if_needed("simple-value_1.2"), "simple-value_1.2");
        assert_eq!(quote_if_needed(""), "''");
        assert_eq!(quote_if_needed("a b"), "'a b'");
        assert_eq!(quote_if_needed("it's"), "'it'\\''s'");
    }

    #[test]
    fn follow_up_preview_matches_source() {
        assert_eq!(
            format_follow_up_preview("- agent: helper\nMessage:\nPlease check the build now"),
            "Follow-up from helper: Please check the build now"
        );
        assert_eq!(
            format_follow_up_preview("Continue working toward the active goal."),
            "Follow-up from goal: Continue working toward the active goal."
        );
        let long = format!("- agent: a\nMessage:\n{}", "x".repeat(60));
        assert_eq!(
            format_follow_up_preview(&long),
            format!("Follow-up from a: {}…", "x".repeat(50))
        );
    }

    #[test]
    fn locale_string_groups_thousands() {
        assert_eq!(to_locale_string(0.0), "0");
        assert_eq!(to_locale_string(1234.0), "1,234");
        assert_eq!(to_locale_string(1234567.0), "1,234,567");
        assert_eq!(to_locale_string(-1234.0), "-1,234");
    }

    #[test]
    fn compact_extension_labels_follow_source_abbreviations() {
        let extensions = vec![
            ("/tmp/extensions/answer.ts".to_owned(), None),
            ("/tmp/extensions/btw.ts".to_owned(), None),
        ];
        assert_eq!(
            get_compact_extension_labels(&extensions),
            vec!["answer.ts".to_owned(), "btw.ts".to_owned()]
        );
    }

    #[test]
    fn compact_extension_labels_strip_index_and_disambiguate() {
        let extensions = vec![
            (
                "/tmp/alpha/one/index.ts".to_owned(),
                Some(source_info(
                    "/tmp/alpha/one/index.ts",
                    "cli",
                    SourceScope::Temporary,
                    Some("/tmp/alpha"),
                )),
            ),
            (
                "/tmp/beta/one/index.ts".to_owned(),
                Some(source_info(
                    "/tmp/beta/one/index.ts",
                    "cli",
                    SourceScope::Temporary,
                    Some("/tmp/beta"),
                )),
            ),
            (
                "/tmp/gamma/one/index.ts".to_owned(),
                Some(source_info(
                    "/tmp/gamma/one/index.ts",
                    "cli",
                    SourceScope::Temporary,
                    Some("/tmp/gamma"),
                )),
            ),
        ];
        assert_eq!(
            get_compact_extension_labels(&extensions),
            vec!["alpha/one".to_owned(), "beta/one".to_owned(), "gamma/one".to_owned()]
        );
    }

    #[test]
    fn compact_package_labels_match_source() {
        let npm = source_info(
            "/tmp/project/.pi/npm/node_modules/pi-markdown-preview/extensions/index.ts",
            "npm:pi-markdown-preview",
            SourceScope::Project,
            Some("/tmp/project/.pi/npm/node_modules/pi-markdown-preview"),
        );
        assert_eq!(
            get_compact_extension_label(
                "/tmp/project/.pi/npm/node_modules/pi-markdown-preview/extensions/index.ts",
                Some(&npm)
            ),
            "pi-markdown-preview"
        );

        let git = source_info(
            "/tmp/project/.pi/git/github.com/HazAT/pi-interactive-subagents/extensions/subagents/index.ts",
            "git:github.com/HazAT/pi-interactive-subagents",
            SourceScope::Project,
            Some("/tmp/project/.pi/git/github.com/HazAT/pi-interactive-subagents"),
        );
        assert_eq!(
            get_compact_extension_label(
                "/tmp/project/.pi/git/github.com/HazAT/pi-interactive-subagents/extensions/subagents/index.ts",
                Some(&git)
            ),
            "HazAT/pi-interactive-subagents:subagents"
        );
    }

    #[test]
    fn display_source_info_scope_labels_match_source() {
        assert_eq!(
            get_display_source_info(None),
            DisplaySourceInfo {
                label: "project".to_owned(),
                scope_label: None,
                color: ThemeColor::Muted,
            }
        );
        let temporary = source_info("/tmp/x.ts", "local", SourceScope::Temporary, None);
        assert_eq!(
            get_display_source_info(Some(&temporary)),
            DisplaySourceInfo {
                label: "path".to_owned(),
                scope_label: Some("temp".to_owned()),
                color: ThemeColor::Muted,
            }
        );
        let cli = source_info("/tmp/x.ts", "cli", SourceScope::Temporary, None);
        assert_eq!(get_display_source_info(Some(&cli)).label, "path");
    }

    #[test]
    fn scope_groups_order_project_user_path_and_sort_paths() {
        let items = vec![
            ScopeGroupItem {
                path: "/tmp/project/.pi/skills/b".to_owned(),
                source_info: None,
            },
            ScopeGroupItem {
                path: "/tmp/project/.pi/skills/a".to_owned(),
                source_info: None,
            },
            ScopeGroupItem {
                path: "/home/u/.pi/skills/c".to_owned(),
                source_info: Some(source_info("/home/u/.pi/skills/c", "local", SourceScope::User, None)),
            },
        ];
        let groups = build_scope_groups(&items);
        assert_eq!(
            groups.iter().map(|group| group.scope).collect::<Vec<_>>(),
            vec![ScopeGroupKind::Project, ScopeGroupKind::User]
        );
        assert_eq!(groups[0].paths.len(), 2);
    }

    #[test]
    fn find_source_info_walks_parent_directories() {
        let mut infos = HashMap::new();
        infos.insert(
            "/tmp/project/.pi/npm/node_modules/pkg".to_owned(),
            source_info(
                "/tmp/project/.pi/npm/node_modules/pkg",
                "npm:pkg",
                SourceScope::Project,
                None,
            ),
        );
        let found = find_source_info_for_path("/tmp/project/.pi/npm/node_modules/pkg/extensions/a.ts", &infos);
        assert_eq!(found.map(|info| info.source.as_str()), Some("npm:pkg"));
    }

    #[test]
    fn diagnostics_render_collision_groups_and_other_entries() {
        init_dark_theme();
        let diagnostics = vec![
            ResourceDiagnostic {
                diagnostic_type: ResourceDiagnosticType::Collision,
                message: "collision".to_owned(),
                path: None,
                collision: Some(crate::core::diagnostics::ResourceCollision {
                    resource_type: crate::core::diagnostics::ResourceType::Skill,
                    name: "commit".to_owned(),
                    winner_path: "/a/winner".to_owned(),
                    loser_path: "/b/loser".to_owned(),
                    winner_source: None,
                    loser_source: None,
                }),
            },
            ResourceDiagnostic {
                diagnostic_type: ResourceDiagnosticType::Warning,
                message: "bad file".to_owned(),
                path: Some("/c/file".to_owned()),
                collision: None,
            },
        ];
        let output = format_diagnostics(&diagnostics, &HashMap::new());
        let plain = pi_tui::utils::strip_terminal_sequences(&output);
        assert!(plain.contains("\"commit\" collision:"));
        assert!(plain.contains("✓ /a/winner"));
        assert!(plain.contains("✗ /b/loser (skipped)"));
        assert!(plain.contains("  /c/file"));
        assert!(plain.contains("    bad file"));
    }

    #[test]
    fn compact_list_sorts_and_trims() {
        init_dark_theme();
        let output = format_compact_list(
            &["read".to_owned(), " bash ".to_owned(), String::new(), "edit".to_owned()],
            true,
        );
        let plain = pi_tui::utils::strip_terminal_sequences(&output);
        assert_eq!(plain, "  bash, edit, read");
    }

    #[test]
    fn extension_display_path_strips_index() {
        assert_eq!(
            format_extension_display_path("/tmp/extensions/plan-mode/index.ts"),
            "/tmp/extensions/plan-mode"
        );
        assert_eq!(
            format_extension_display_path("/tmp/extensions/plan-mode/index.js"),
            "/tmp/extensions/plan-mode"
        );
        assert_eq!(
            format_extension_display_path("/tmp/extensions/webfetch.ts"),
            "/tmp/extensions/webfetch.ts"
        );
    }

    #[test]
    fn expandable_text_switches_bodies() {
        use pi_tui::tui::Component;

        let mut text = ExpandableText::new(|| "collapsed".to_owned(), || "expanded".to_owned(), false, 0, 0);
        assert_eq!(text.render(40)[0].trim_end(), "collapsed");
        text.set_expanded(true);
        assert_eq!(text.render(40)[0].trim_end(), "expanded");
        text.set_expanded(false);
        assert_eq!(text.render(40)[0].trim_end(), "collapsed");
    }

    #[test]
    fn unix_relative_paths_match_posix_relative() {
        assert_eq!(posix_relative("/a/b", "/a/b/c/d"), "c/d");
        assert_eq!(posix_relative("/a/b/c", "/a/d"), "../../d");
        assert_eq!(posix_relative("/a", "/a"), "");
    }

    #[test]
    fn path_segments_ignore_root_and_home_markers() {
        assert_eq!(
            get_compact_display_path_segments("/tmp/x/index.ts"),
            vec!["tmp".to_owned(), "x".to_owned(), "index.ts".to_owned()]
        );
    }

    #[test]
    fn parsed_posix_paths_match_node_shape() {
        assert_eq!(parse_posix_path("index.ts").dir, "");
        assert_eq!(parse_posix_path("index.ts").name, "index");
        assert_eq!(parse_posix_path("subagents/index.ts").dir, "subagents");
        assert_eq!(parse_posix_path("subagents/index.ts").name, "index");
        assert_eq!(parse_posix_path("answer.ts").name, "answer");
    }

    #[test]
    fn pathbuf_components_are_used_for_relative_paths() {
        assert_eq!(
            relative_path("/tmp/a", "/tmp/a/b"),
            PathBuf::from("b").to_string_lossy()
        );
        assert!(matches!(
            Path::new("/tmp/a").components().next(),
            Some(std::path::Component::RootDir)
        ));
    }
}
