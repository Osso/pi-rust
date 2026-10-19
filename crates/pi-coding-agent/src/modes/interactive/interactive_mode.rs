//! Port of `coding-agent/src/modes/interactive/interactive-mode.ts`.
//!
//! The TypeScript class is the composition root for the interactive TUI. This
//! module keeps the same structure: pure formatting helpers first, then the
//! mode itself. Component factories and the TUI event loop are Rust-native;
//! deviations are recorded in `docs/porting/traceability/interactive-mode.md`.

use std::collections::HashMap;
use std::path::{Component as PathComponent, Path, PathBuf};
use std::sync::OnceLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::core::diagnostics::{ResourceDiagnostic, ResourceDiagnosticType};
use crate::core::source_info::{SourceInfo, SourceScope};
use crate::modes::interactive::theme::{ThemeColor, with_theme};
use crate::utils::git::parse_git_url;
use crate::utils::paths::{get_cwd_relative_path, relative_path};

pub(crate) const PARTIAL_UPDATE_RENDER_THROTTLE_MS: u64 = 50;
pub(crate) const MAX_AGENT_LOG_VIEW_BYTES: usize = 64 * 1024;
pub(crate) const CHILD_TRANSCRIPT_RELOAD_DEBOUNCE_MS: u64 = 50;
pub(crate) const MAX_CHILD_TRANSCRIPT_RELOAD_RETRIES: usize = 3;
pub(crate) const FOLLOW_UP_BODY_PREVIEW_LENGTH: usize = 50;

pub(crate) const ANTHROPIC_SUBSCRIPTION_AUTH_WARNING: &str = "Anthropic subscription auth is active. Third-party harness usage draws from extra usage and is billed per token, not your Claude plan limits. Manage extra usage at https://claude.ai/settings/usage. Disable this warning in /settings.";

pub(crate) const DEAD_TERMINAL_ERROR_CODES: [&str; 3] = ["EIO", "EPIPE", "ENOTCONN"];

/// Resolves the persisted scope/pattern pair used by `/models`.
pub(crate) fn resolve_enabled_models_persist_target(
    project_enabled_models: Option<&[String]>,
    patterns: Option<Vec<String>>,
) -> (Option<Vec<String>>, &'static str) {
    match project_enabled_models {
        None => (patterns, "global"),
        Some(_) => (Some(patterns.unwrap_or_default()), "project"),
    }
}

pub(crate) fn is_dead_terminal_error(code: Option<&str>) -> bool {
    code.is_some_and(|code| DEAD_TERMINAL_ERROR_CODES.contains(&code))
}

pub(crate) fn is_anthropic_subscription_auth_key(api_key: Option<&str>) -> bool {
    api_key.is_some_and(|api_key| api_key.starts_with("sk-ant-oat"))
}

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
    for _ in common..from_parts.len() {
        parts.push("..");
    }
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
    fn resolve_enabled_models_target_uses_project_scope_for_explicit_list() {
        assert_eq!(
            resolve_enabled_models_persist_target(None, Some(vec!["a".to_owned()])),
            (Some(vec!["a".to_owned()]), "global")
        );
        assert_eq!(
            resolve_enabled_models_persist_target(Some(&[]), None),
            (Some(Vec::new()), "project")
        );
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
            Some(PathComponent::RootDir)
        ));
    }
}
