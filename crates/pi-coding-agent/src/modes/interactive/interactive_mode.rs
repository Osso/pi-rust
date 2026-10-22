//! Port of `coding-agent/src/modes/interactive/interactive-mode.ts`.
//!
//! The TypeScript class is the composition root for the interactive TUI. This
//! module keeps the same structure: pure formatting helpers first, then the
//! mode itself. Component factories and the TUI event loop are Rust-native;
//! deviations are recorded in `docs/porting/traceability/interactive-mode.md`.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use indexmap::IndexMap;
use pi_agent_core::agent::types::{AgentEvent, AgentMessage};
use pi_ai::auth::types::{AuthEvent, AuthInteraction, AuthPrompt, AuthPromptKind, AuthType};
use pi_ai::request::{CallbackError, Fetch, HttpRequest};
use pi_ai::types::{AssistantMessage, ImageContent, Model};
use pi_js_compat::date::js_now_millis;
use pi_js_compat::js_format;
use pi_js_compat::js_regex::JsRegex as Regex;
use pi_tui::EditorComponent;
use pi_tui::TUI;
use pi_tui::autocomplete::{
    AutocompleteItem, AutocompleteProvider, CombinedAutocompleteProvider, CommandEntry, SlashCommand,
};
use pi_tui::components::editor::EditorOptions;
use pi_tui::components::loader::Loader;
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::fuzzy::fuzzy_filter;
use pi_tui::terminal::Terminal;
use pi_tui::tui::{
    Component, ComponentRef, Container, FlowRenderRegion, Focusable, OverlayHandle, OverlayOptions, RenderRegion,
    TuiExt, TuiInputListenerResult, TuiMode, component,
};
use pi_tui::{ProcessTerminal, TerminalRawModeError};

use crate::core::agent_session::{AgentSession, AgentSessionEvent, CycleDirection, PromptOptions, StreamingBehavior};
use crate::core::agent_session_runtime::AgentSessionRuntime;
use crate::core::diagnostics::{ResourceDiagnostic, ResourceDiagnosticType};
use crate::core::extensions::types::{
    BoxFuture, ExtensionAutocompleteProviderFactory, ExtensionEditorFactory, ExtensionUiComponent,
    ExtensionUiComponentFactory, ExtensionUiContext, ExtensionUiCustomFactory, ExtensionUiCustomOptions,
    ExtensionUiDialogOptions, ExtensionUiDone, ExtensionUiFooterFactory, ExtensionUiTui, ExtensionUiTuiBackend,
    ExtensionWidgetOptions, HandlerError, NotifyType, SessionTransitionResult, TerminalInputHandler, ThemeInfo,
    ThemeSetResult, WidgetPlacement, WorkingIndicatorOptions,
};
use crate::core::footer_data_provider::{ContextUsage, FooterDataProvider, FooterSessionOverride};
use crate::core::keybindings::KeybindingsManager;
use crate::core::model_resolver::resolve_model_scope_from_models;
use crate::core::multi_agent_store::{AgentLifecycle, AgentSnapshot, MultiAgentStore, lifecycle_name};
use crate::core::permissions::presets::{SandboxProfileName, SandboxProfileScope};
use crate::core::runtime_message_markers::is_duplicate_turn_assistant_message;
use crate::core::session_manager::{CompactionEntry, KnownSessionEntry, SessionEntry, SessionManager};
use crate::core::settings_manager::TreeFilterMode;
use crate::core::source_info::{SourceInfo, SourceScope};
use crate::core::trust_manager::ProjectTrustStore;
use crate::modes::interactive::components::agent_selection_banner::AgentSelectionBannerComponent;
use crate::modes::interactive::components::agent_switcher::AgentSwitcherComponent;
use crate::modes::interactive::components::approval_selector::{ApprovalSelectorComponent, ApprovalSelectorOptions};
use crate::modes::interactive::components::armin::ArminComponent;
use crate::modes::interactive::components::assistant_message::AssistantMessageComponent;
use crate::modes::interactive::components::bash_execution::{BashExecutionComponent, BashExecutionOptions};
use crate::modes::interactive::components::bordered_loader::{BorderedLoader, BorderedLoaderOptions};
use crate::modes::interactive::components::branch_summary_message::BranchSummaryMessageComponent;
use crate::modes::interactive::components::compaction_summary_message::CompactionSummaryMessageComponent;
use crate::modes::interactive::components::countdown_timer::CountdownTimer;
use crate::modes::interactive::components::custom_editor::{AppActionHandler, CustomEditor, CustomEditorOptions};
use crate::modes::interactive::components::custom_entry::CustomEntryComponent;
use crate::modes::interactive::components::custom_message::CustomMessageComponent;
use crate::modes::interactive::components::daxnuts::DaxnutsComponent;
use crate::modes::interactive::components::earendil_announcement::EarendilAnnouncementComponent;
use crate::modes::interactive::components::elapsed_time::{MIN_VISIBLE_ELAPSED_MS, format_elapsed_duration};
use crate::modes::interactive::components::extension_editor::{ExtensionEditorComponent, ExtensionEditorHost};
use crate::modes::interactive::components::extension_input::{ExtensionInputComponent, ExtensionInputOptions};
use crate::modes::interactive::components::extension_selector::{ExtensionSelectorComponent, ExtensionSelectorOptions};
use crate::modes::interactive::components::footer::{FooterComponent, format_tokens};
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::components::login_dialog::LoginDialogComponent;
use crate::modes::interactive::components::oauth_selector::{
    AuthSelectorMode, AuthSelectorProvider, OAuthSelectorComponent,
};
use crate::modes::interactive::components::sandbox_selector::{
    SandboxScope, SandboxSelectorComponent, SandboxSelectorOptions, SandboxSelectorSelection,
};
use crate::modes::interactive::components::scoped_models_selector::{
    EnabledIds, ModelsCallbacks, ModelsConfig, ScopedModelsSelectorComponent,
};
use crate::modes::interactive::components::session_selector::{
    ProgressCallback, RenameSessionFn, SessionListLoader, SessionSelectorComponent,
};
use crate::modes::interactive::components::settings_selector::{
    SettingsCallbacks, SettingsConfig, SettingsSelectorComponent, SteeringMode,
};
use crate::modes::interactive::components::skill_invocation_message::SkillInvocationMessageComponent;
use crate::modes::interactive::components::thinking_selector::ThinkingSelectorComponent;
use crate::modes::interactive::components::tool_execution::{
    ToolExecutionComponent, ToolExecutionOptions, ToolExecutionResult,
};
use crate::modes::interactive::components::tree_selector::{FilterMode, TreeSelectorComponent};
use crate::modes::interactive::components::trust_selector::{TrustSelectorComponent, TrustSelectorOptions};
use crate::modes::interactive::components::user_message::UserMessageComponent;
use crate::modes::interactive::components::user_message_selector::{UserMessageItem, UserMessageSelectorComponent};
use crate::modes::interactive::interactive_root_compositor::{
    InteractiveRootCompositorOptions, create_interactive_root_compositor,
};
use crate::modes::interactive::login_options::get_logout_provider_options;
use crate::modes::interactive::login_options::{
    format_login_provider_completion_description, get_login_provider_completion_options, get_login_provider_options,
    get_login_provider_search_text,
};
use crate::modes::interactive::model_search::{ModelSearchItem, get_model_search_candidates};
use crate::modes::interactive::theme::{Theme, ThemeColor, with_theme};
use crate::utils::fs_watch::{FS_WATCH_RETRY_DELAY_MS, WatchEventKind, watch_with_error_handler};
use crate::utils::git::parse_git_url;
use crate::utils::paths::{
    PathInputOptions, get_cwd_relative_path, node_basename, node_dirname, node_extname, node_is_absolute, node_join,
    node_resolve, relative_path, resolve_path,
};
use crate::utils::shell::kill_tracked_detached_children;
use crate::utils::uncaught::{ERROR, format_uncaught, set_process_crash_handler, swallow_panics, throw_uncaught};
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue, encode_node_utf8};
#[cfg(test)]
use serde_json::Value;

pub(crate) const PARTIAL_UPDATE_RENDER_THROTTLE_MS: u64 = 50;

const ANTHROPIC_SUBSCRIPTION_AUTH_WARNING: &str = "Anthropic subscription auth is active. Third-party harness usage draws from extra usage and is billed per token, not your Claude plan limits. Manage extra usage at https://claude.ai/settings/usage. Disable this warning in /settings.";
pub(crate) const FOLLOW_UP_BODY_PREVIEW_LENGTH: usize = 50;

pub(crate) fn is_unknown_model(model: Option<&pi_ai::types::Model>) -> bool {
    model.is_some_and(|model| model.provider == "unknown" && model.id == "unknown" && model.api == "unknown")
}

/// `DEAD_TERMINAL_ERROR_CODES`: `EIO`, `EPIPE`, `ENOTCONN`.
const DEAD_TERMINAL_ERROR_CODES: [i32; 3] = [
    rustix::io::Errno::IO.raw_os_error(),
    rustix::io::Errno::PIPE.raw_os_error(),
    rustix::io::Errno::NOTCONN.raw_os_error(),
];

/// Node's stream error message: `<syscall> <CODE>` (`write EPIPE`).
pub fn terminal_error_message(failure: &pi_tui::terminal::TerminalIoError) -> JsString {
    js_format!(
        "{} {}",
        failure.syscall,
        crate::utils::child_process::node_error_code(&failure.error)
    )
}

/// `isDeadTerminalError`: the terminal is gone (closed pty, dead SSH pipe).
pub fn is_dead_terminal_error(error: &std::io::Error) -> bool {
    error
        .raw_os_error()
        .is_some_and(|code| DEAD_TERMINAL_ERROR_CODES.contains(&code))
}

/// Decision made by `uncaughtCrash` before it touches the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrashAction {
    /// The terminal is gone: exit 129 without running restore writes.
    EmergencyExit,
    /// A crash is already being handled: exit 1 immediately.
    ExitOne,
    /// Restore the terminal, report the error, exit 1.
    RestoreAndReport,
}

fn crash_action(dead_terminal: bool, is_shutting_down: bool) -> CrashAction {
    if dead_terminal {
        return CrashAction::EmergencyExit;
    }
    if is_shutting_down {
        return CrashAction::ExitOne;
    }
    CrashAction::RestoreAndReport
}

/// What `terminalErrorHandler` does with a stdin/stdout/stderr error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalErrorAction {
    /// `emergencyTerminalExit()`.
    EmergencyExit,
    /// `return`: a raw-mode failure is reported elsewhere.
    Ignore,
    /// `throw error`: it becomes an uncaught exception.
    Rethrow,
}

fn terminal_error_action(error: &std::io::Error) -> TerminalErrorAction {
    if is_dead_terminal_error(error) {
        return TerminalErrorAction::EmergencyExit;
    }
    if pi_tui::is_terminal_raw_mode_failure(error) {
        return TerminalErrorAction::Ignore;
    }
    TerminalErrorAction::Rethrow
}

/// `isDeadTerminalError(error)` for a thrown value: only an `io::Error`
/// payload carries an errno code.
fn payload_is_dead_terminal_error(payload: &(dyn Any + Send)) -> bool {
    payload
        .downcast_ref::<std::io::Error>()
        .is_some_and(is_dead_terminal_error)
}

/// `getDirectoryCompletions`: directories under `prefix` resolved against `cwd`.
pub(crate) fn get_directory_completions(prefix: &JsString, cwd: &JsString) -> Vec<AutocompleteItem> {
    let quoted = prefix.starts_with('"');
    let raw_prefix = if quoted {
        prefix.slice(1, prefix.len())
    } else {
        prefix.clone()
    };
    let normalized_prefix = raw_prefix.replace(std::path::MAIN_SEPARATOR, "/");
    if let Some(item) = directory_root_completion(&normalized_prefix, quoted) {
        return vec![item];
    }
    let ends_with_slash = normalized_prefix.ends_with('/');
    let typed_dir = if ends_with_slash {
        normalized_prefix.clone()
    } else {
        pi_tui::autocomplete::posix_dirname(&normalized_prefix)
    };
    let typed_name = if ends_with_slash || normalized_prefix.is_empty() {
        JsString::default()
    } else {
        pi_tui::autocomplete::posix_basename(&normalized_prefix)
    };
    let display_dir = if typed_dir == "." {
        JsString::default()
    } else {
        typed_dir
    };
    let search_dir = if display_dir.is_empty() {
        &JsString::from(".")
    } else {
        &display_dir
    };
    let Ok(search_dir) = resolve_path(search_dir, cwd, &PathInputOptions::default()) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(encode_node_utf8(&search_dir)) else {
        return Vec::new();
    };
    let typed_name_lower = typed_name.to_lowercase();
    let mut names: Vec<JsString> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()))
        .map(|entry| JsString::from(entry.file_name().to_string_lossy().as_ref()))
        .filter(|name| name.to_lowercase().starts_with(&typed_name_lower))
        .collect();
    names.sort_by(|a, b| pi_js_compat::collator::locale_compare_utf16(a.units(), b.units()));
    names
        .iter()
        .map(|name| directory_completion_item(name, &display_dir, quoted))
        .collect()
}

fn directory_root_completion(normalized_prefix: &JsString, quoted: bool) -> Option<AutocompleteItem> {
    match normalized_prefix {
        value if value == "~" => Some(directory_completion_value(&"~/".into(), quoted, &"~/".into())),
        value if value == ".." => Some(directory_completion_value(&"../".into(), quoted, &"../".into())),
        value if value == "." => Some(directory_completion_value(&"./".into(), quoted, &"./".into())),
        _ => None,
    }
}

fn directory_completion_item(entry_name: &JsString, display_dir: &JsString, quoted: bool) -> AutocompleteItem {
    let display_path = join_display_directory(display_dir, entry_name);
    directory_completion_value(&display_path, quoted, &js_format!("{}/", entry_name))
}

fn directory_completion_value(display_path: &JsString, quoted: bool, label: &JsString) -> AutocompleteItem {
    let needs_quote = quoted
        || display_path
            .units()
            .iter()
            .any(|unit| pi_js_compat::string::is_js_whitespace_unit(*unit));
    let escaped_path = display_path.replace('"', "\\\"");
    let value = if needs_quote {
        js_format!("\"{}\"", escaped_path)
    } else {
        display_path.to_owned()
    };
    AutocompleteItem {
        value,
        label: label.to_owned(),
        description: None,
    }
}

fn join_display_directory(display_dir: &JsString, entry_name: &JsString) -> JsString {
    if display_dir == "/" {
        return js_format!("/{}/", entry_name);
    }
    if display_dir.ends_with('/') {
        return js_format!("{}{}/", display_dir, entry_name);
    }
    if display_dir.is_empty() {
        js_format!("{}/", entry_name)
    } else {
        js_format!("{}/{}/", display_dir, entry_name)
    }
}

fn model_argument_completions(models: &[Model], prefix: &JsString) -> Option<Vec<AutocompleteItem>> {
    if models.is_empty() {
        return None;
    }
    let items: Vec<ModelSearchItem> = models
        .iter()
        .map(|model| ModelSearchItem {
            id: model.id.clone(),
            provider: model.provider.clone(),
            name: Some(model.name.clone()),
        })
        .collect();
    let filtered = fuzzy_filter(&items, prefix, get_model_search_candidates);
    if filtered.is_empty() {
        return None;
    }
    Some(
        filtered
            .iter()
            .map(|item| AutocompleteItem {
                value: js_format!("{}/{}", item.provider, item.id),
                label: (item.id.clone()).into(),
                description: Some((item.provider.clone()).into()),
            })
            .collect(),
    )
}

fn login_argument_completions(
    runtime: &crate::core::model_runtime::ModelRuntime,
    prefix: &JsString,
) -> Option<Vec<AutocompleteItem>> {
    let providers = get_login_provider_options(runtime, None);
    let options = get_login_provider_completion_options(&providers);
    let filtered = fuzzy_filter(&options, prefix, get_login_provider_search_text);
    if filtered.is_empty() {
        return None;
    }
    Some(
        filtered
            .iter()
            .map(|provider| AutocompleteItem {
                value: (provider.id.clone()).into(),
                label: (provider.id.clone()).into(),
                description: Some((format_login_provider_completion_description(provider)).into()),
            })
            .collect(),
    )
}

fn autocomplete_item_from_value(value: &pi_js_compat::js_json::JsValue) -> Option<AutocompleteItem> {
    let object = value.as_object()?;
    let item_value = object.get("value")?.as_string()?;
    let label = object
        .get("label")
        .and_then(pi_js_compat::js_json::JsValue::as_string)
        .unwrap_or(item_value);
    Some(AutocompleteItem {
        value: item_value.to_owned(),
        label: label.to_owned(),
        description: object
            .get("description")
            .and_then(pi_js_compat::js_json::JsValue::as_string)
            .cloned(),
    })
}

/// Shell-quote a value the way the source resume hint does.
pub(crate) fn quote_if_needed(value: &JsString) -> JsString {
    let safe = !value.is_empty()
        && !value.units().iter().any(|unit| {
            !((*unit <= 0x7f && (*unit as u8).is_ascii_alphanumeric())
                || JsString::from("_-./~:@").units().contains(unit))
        });
    if safe {
        return value.to_owned();
    }
    js_format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn is_skill_invocation(
    text: &JsString,
    prompt_templates: &[crate::core::prompt_templates::PromptTemplate],
) -> bool {
    if crate::parse_skill_block(text).is_some() || text.starts_with("/skill:") {
        return true;
    }
    crate::parse_skill_block(&crate::core::prompt_templates::expand_prompt_template(
        &JsString::from(text),
        prompt_templates,
    ))
    .is_some()
}

/// `formatResumeCommand`: printable `pi --session ...` hint.
pub fn format_resume_command(session_manager: &crate::core::session_manager::SessionManager) -> Option<JsString> {
    use std::io::IsTerminal;

    if !std::io::stdout().is_terminal() {
        return None;
    }
    if !session_manager.is_persisted() {
        return None;
    }
    let session_file = session_manager.get_session_file()?;
    if !Path::new(&encode_node_utf8(session_file)).exists() {
        return None;
    }

    let mut args = vec![JsString::from(crate::config::package_config().app_name.clone())];
    if !session_manager.uses_default_session_dir() {
        args.push(JsString::from("--session-dir"));
        args.push(quote_if_needed(session_manager.get_session_dir()));
    }
    args.push(JsString::from("--session"));
    args.push(session_manager.get_session_id().to_owned());
    Some(JsString::join(args, " "))
}

pub(crate) fn format_follow_up_preview(message: impl Into<JsString>) -> JsString {
    let message = message.into();
    let sender = agent_sender(&message).unwrap_or_else(|| {
        if message.trim() == "Continue working toward the active goal." {
            "goal".into()
        } else {
            "unknown".into()
        }
    });
    let marker = "\nMessage:\n";
    let body = message
        .find(marker)
        .map(|index| {
            let remainder = message.slice(index + marker.len(), message.len());
            remainder
                .find(marker)
                .map_or_else(|| remainder.clone(), |end| remainder.slice(0, end))
        })
        .unwrap_or_else(|| message.clone());
    let body = body.split('\n').next().unwrap_or_default().trim();
    let preview = if body.len() > FOLLOW_UP_BODY_PREVIEW_LENGTH {
        body.slice(0, FOLLOW_UP_BODY_PREVIEW_LENGTH) + "…"
    } else {
        body
    };
    (JsString::from("Follow-up from ") + sender + ": " + preview).into()
}

fn agent_sender(message: &JsString) -> Option<JsString> {
    let units = message.units();
    let is_line_end = |unit: u16| matches!(unit, 0x000a | 0x000d | 0x2028 | 0x2029);
    for start in 0..units.len() {
        if start > 0 && !is_line_end(units[start - 1]) {
            continue;
        }
        const PREFIX: &[u16] = &[45, 32, 97, 103, 101, 110, 116, 58];
        if !units[start..].starts_with(PREFIX) {
            continue;
        }
        let line = message.slice(start + PREFIX.len(), message.len());
        let trimmed = line.trim_start();
        let end = trimmed
            .units()
            .iter()
            .position(|unit| is_line_end(*unit))
            .unwrap_or(trimmed.len());
        if end > 0 {
            return Some(trimmed.slice(0, end).trim());
        }
        if line.units().iter().any(|unit| !is_line_end(*unit)) {
            return Some(JsString::default());
        }
    }
    None
}

/// JS `Number.prototype.toLocaleString()` for token counts under the default
/// locale used by the source (`en-US` grouping).
pub(crate) fn to_locale_string(value: f64) -> JsString {
    if !value.is_finite() {
        return (pi_js_compat::number::js_number_to_string(value)).into();
    }
    let rounded = value.trunc();
    let negative = rounded < 0.0;
    let digits = format!("{:.0}", rounded.abs());
    let mut grouped = JsString::default();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push_str(',');
        }
        grouped.push_str(character);
    }
    if negative { js_format!("-{}", grouped) } else { grouped }
}

pub(crate) fn format_session_token_info(tokens: &crate::core::agent_session::SessionTokenStats) -> JsString {
    let prompt_tokens = tokens.input + tokens.cache_read + tokens.cache_write;
    with_theme(|theme| {
        let mut info = js_format!("{}\n", theme.bold("Tokens"));
        info.push_str(&js_format!(
            "{} {}\n",
            theme.fg(ThemeColor::Dim, "Input:"),
            to_locale_string(prompt_tokens)
        ));
        if prompt_tokens > 0.0 && (tokens.cache_read > 0.0 || tokens.cache_write > 0.0) {
            let hit_rate = theme.fg(
                ThemeColor::Dim,
                &js_format!(
                    "({}%)",
                    pi_js_compat::number::to_fixed(tokens.cache_read / prompt_tokens * 100.0, 1)
                ),
            );
            info.push_str(&js_format!(
                "  {} {} {}\n",
                theme.fg(ThemeColor::Dim, "Cached:"),
                to_locale_string(tokens.cache_read),
                hit_rate
            ));
            let written = if tokens.cache_write > 0.0 {
                js_format!(
                    " {}",
                    theme.fg(
                        ThemeColor::Dim,
                        &js_format!("({} written to cache)", to_locale_string(tokens.cache_write))
                    )
                )
            } else {
                JsString::default()
            };
            info.push_str(&js_format!(
                "  {} {}{}\n",
                theme.fg(ThemeColor::Dim, "Uncached:"),
                to_locale_string(tokens.input + tokens.cache_write),
                written
            ));
        }
        info.push_str(&js_format!(
            "{} {}\n",
            theme.fg(ThemeColor::Dim, "Output:"),
            to_locale_string(tokens.output)
        ));
        info.push_str(&js_format!(
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
) -> JsString {
    if cost <= 0.0 && cache_waste.missed_tokens <= 0.0 {
        return JsString::default();
    }
    with_theme(|theme| {
        let mut info = js_format!("\n{}\n", theme.bold("Cost"));
        info.push_str(&js_format!(
            "{} ${}",
            theme.fg(ThemeColor::Dim, "Total:"),
            pi_js_compat::number::to_fixed(cost, 3)
        ));
        if usage_breakdown.len() > 1 {
            for entry in usage_breakdown {
                info.push_str(&js_format!(
                    "\n  {} ${} {}",
                    theme.fg(ThemeColor::Dim, &js_format!("{}:", entry.key)),
                    pi_js_compat::number::to_fixed(entry.cost, 3),
                    theme.fg(
                        ThemeColor::Dim,
                        &js_format!(
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
            JsString::from("1 miss")
        } else {
            js_format!("{} misses", cache_waste.miss_count)
        };
        let detail = js_format!("{} tokens, {}", to_locale_string(cache_waste.missed_tokens), miss_label);
        if cache_waste.missed_cost >= 0.0001 {
            info.push_str(&js_format!(
                "\n{} ${} {}",
                theme.fg(ThemeColor::Dim, "Cache Re-billed:"),
                pi_js_compat::number::to_fixed(cache_waste.missed_cost, 3),
                theme.fg(ThemeColor::Dim, &js_format!("({})", detail))
            ));
        } else {
            info.push_str(&js_format!(
                "\n{} {}",
                theme.fg(ThemeColor::Dim, "Cache Re-billed:"),
                detail
            ));
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

fn home_directory() -> JsString {
    #[cfg(windows)]
    {
        pi_js_compat::process_env::get("USERPROFILE")
            .or_else(|| pi_js_compat::process_env::get("HOME"))
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        pi_js_compat::process_env::get("HOME").unwrap_or_default().into()
    }
}

pub(crate) fn format_display_path(path: &JsString) -> JsString {
    let home = home_directory();
    if !home.is_empty() && path.starts_with(&home) {
        return js_format!("~{}", path.slice(home.len(), path.len()));
    }
    path.to_owned()
}

pub(crate) fn format_extension_display_path(path: &JsString) -> JsString {
    let display = format_display_path(path);
    display
        .strip_suffix("/index.ts")
        .or_else(|| display.strip_suffix("/index.js"))
        .unwrap_or((&display).clone())
        .to_owned()
}

pub(crate) fn format_context_path(path: &JsString, cwd: &JsString) -> JsString {
    let cwd = resolve_path_string(cwd);
    let absolute = if node_is_absolute(path) {
        resolve_path_string(path)
    } else {
        resolve_path_string(&js_format!("{}/{}", cwd, path))
    };
    if let Ok(Some(relative)) = get_cwd_relative_path(&absolute, &cwd) {
        return relative;
    }
    format_display_path(&absolute)
}

fn resolve_path_string(path: &JsString) -> JsString {
    node_resolve("", path).unwrap_or_else(|error| throw_uncaught(ERROR, error.message().clone()))
}

fn normalize_slashes(path: &JsString) -> JsString {
    path.replace('\\', "/")
}

fn posix_relative(from: &JsString, to: &JsString) -> JsString {
    relative_path(from, to).unwrap_or_else(|error| throw_uncaught(ERROR, error.message().clone()))
}

pub(crate) fn get_short_path(full_path: &JsString, source_info: Option<&SourceInfo>) -> JsString {
    let normalized_full_path = normalize_slashes(full_path);
    if let Some(source_info) = source_info
        && let Some(base_dir) = source_info.base_dir.as_ref()
        && is_package_source(Some(source_info))
    {
        let normalized_base_dir = normalize_slashes(base_dir);
        if let Some(captures) = package_path_regex().captures(&normalized_base_dir) {
            let root = captures.get(1).map(|capture| capture.text()).unwrap_or_default();
            if !root.is_empty() && normalized_full_path.starts_with(&js_format!("{}/", root)) {
                return posix_relative(&normalized_base_dir, &normalized_full_path);
            }
        }

        let relative = relative_path(&resolve_path_string(base_dir), &resolve_path_string(full_path))
            .unwrap_or_else(|error| throw_uncaught(ERROR, error.message().clone()));
        if !relative.is_empty()
            && relative != "."
            && !relative.starts_with("..")
            && !relative.starts_with(&js_format!("..{}", std::path::MAIN_SEPARATOR))
            && !node_is_absolute(&relative)
        {
            return normalize_slashes(&relative);
        }
    }

    let source = source_info.map(|info| info.source.clone()).unwrap_or_default();
    if let Some(captures) = node_modules_path_regex().captures(&normalized_full_path)
        && source.starts_with("npm:")
        && let Some(path) = captures.get(2)
    {
        return (path.text()).into();
    }
    if let Some(captures) = git_path_regex().captures(&normalized_full_path)
        && source.starts_with("git:")
        && let Some(path) = captures.get(1)
    {
        return path.text();
    }
    format_display_path(full_path)
}

pub(crate) fn get_compact_path_label(resource_path: &JsString, source_info: Option<&SourceInfo>) -> JsString {
    let short_path = get_short_path(resource_path, source_info);
    let normalized_path = normalize_slashes(&short_path);
    let segments: Vec<JsString> = normalized_path
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "~")
        .collect();
    if let Some(last) = segments.last() {
        return (*last).to_owned();
    }
    short_path
}

pub(crate) fn get_compact_package_source_label(source_info: Option<&SourceInfo>) -> JsString {
    let source = source_info.map(|info| info.source.clone()).unwrap_or_default();
    if let Some(stripped) = source.strip_prefix("npm:") {
        return if stripped.is_empty() {
            source.to_owned()
        } else {
            stripped.to_owned()
        };
    }
    if let Some(git_source) = parse_git_url(&source) {
        return if git_source.path.is_empty() {
            source.to_owned()
        } else {
            git_source.path
        };
    }
    source.to_owned()
}

pub(crate) fn get_compact_extension_label(resource_path: &JsString, source_info: Option<&SourceInfo>) -> JsString {
    if !is_package_source(source_info) {
        return get_compact_path_label(resource_path, source_info);
    }
    let source_label = get_compact_package_source_label(source_info);
    if source_label.is_empty() {
        return get_compact_path_label(resource_path, source_info);
    }
    let short_path = normalize_slashes(&get_short_path(resource_path, source_info));
    let package_path = short_path.strip_prefix("extensions/").unwrap_or((&short_path).clone());
    let parsed = parse_posix_path(&package_path);
    if parsed.name == "index" {
        if parsed.dir.is_empty() || parsed.dir == "." {
            return source_label;
        }
        return js_format!("{}:{}", source_label, parsed.dir);
    }
    js_format!("{}:{}", source_label, package_path)
}

struct ParsedPath {
    dir: JsString,
    name: JsString,
}

fn parse_posix_path(path: &JsString) -> ParsedPath {
    let base = node_basename(path);
    let extension = node_extname(&base);
    let units = path.units();
    let root_length = usize::from(units.first() == Some(&u16::from(b'/')));
    let last_component_end = units.iter().rposition(|unit| *unit != u16::from(b'/'));
    let directory_end = last_component_end
        .and_then(|end| units[..=end].iter().rposition(|unit| *unit == u16::from(b'/')))
        .unwrap_or(root_length)
        .max(root_length);
    ParsedPath {
        dir: path.slice(0, directory_end),
        name: base.slice(0, base.len() - extension.len()),
    }
}

pub(crate) fn get_compact_display_path_segments(resource_path: &JsString) -> Vec<JsString> {
    normalize_slashes(&format_display_path(resource_path))
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "~")
        .collect()
}

fn compact_non_package_extension_label(
    resource_path: &JsString,
    index: usize,
    all_paths: &[Vec<JsString>],
) -> JsString {
    let Some(segments) = all_paths.get(index) else {
        return get_compact_path_label(resource_path, None);
    };
    if segments.is_empty() {
        return get_compact_path_label(resource_path, None);
    }
    for segment_count in 1..=segments.len() {
        let start = segments.len() - segment_count;
        let candidate = JsString::join(&segments[start..], "/");
        let unique = all_paths.iter().enumerate().all(|(item_index, item)| {
            if item_index == index {
                return true;
            }
            let start = item.len().saturating_sub(segment_count);
            JsString::join(&item[start..], "/") != candidate
        });
        if unique {
            return candidate;
        }
    }
    JsString::join(segments, "/")
}

pub(crate) fn get_compact_extension_labels(extensions: &[(JsString, Option<SourceInfo>)]) -> Vec<JsString> {
    let non_package: Vec<&(JsString, Option<SourceInfo>)> = extensions
        .iter()
        .filter(|(_, source_info)| !is_package_source(source_info.as_ref()))
        .collect();
    let non_package_segments: Vec<Vec<JsString>> = non_package
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
    pub label: JsString,
    pub scope_label: Option<JsString>,
    pub color: ThemeColor,
}

pub(crate) fn get_display_source_info(source_info: Option<&SourceInfo>) -> DisplaySourceInfo {
    let source = source_info
        .map(|info| info.source.clone())
        .unwrap_or_else(|| "local".into());
    let scope = source_info.map(|info| info.scope).unwrap_or(SourceScope::Project);
    if source == "local" {
        return match scope {
            SourceScope::User => DisplaySourceInfo {
                label: JsString::from("user"),
                scope_label: None,
                color: ThemeColor::Muted,
            },
            SourceScope::Project => DisplaySourceInfo {
                label: JsString::from("project"),
                scope_label: None,
                color: ThemeColor::Muted,
            },
            SourceScope::Temporary => DisplaySourceInfo {
                label: JsString::from("path"),
                scope_label: Some(JsString::from("temp")),
                color: ThemeColor::Muted,
            },
        };
    }
    if source == "cli" {
        return DisplaySourceInfo {
            label: JsString::from("path"),
            scope_label: (scope == SourceScope::Temporary).then(|| JsString::from("temp")),
            color: ThemeColor::Muted,
        };
    }
    let scope_label = match scope {
        SourceScope::User => Some(JsString::from("user")),
        SourceScope::Project => Some(JsString::from("project")),
        SourceScope::Temporary => Some(JsString::from("temp")),
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
    let source = source_info
        .map(|info| info.source.clone())
        .unwrap_or_else(|| "local".into());
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
        .map(|info| &info.source)
        .unwrap_or(&JsString::from(""))
        .starts_with("npm:")
        || source_info
            .map(|info| &info.source)
            .unwrap_or(&JsString::from(""))
            .starts_with("git:")
}

#[derive(Debug, Clone)]
pub(crate) struct ScopeGroupItem {
    pub path: JsString,
    pub source_info: Option<SourceInfo>,
}

#[derive(Debug, Clone)]
pub(crate) struct ScopeGroup {
    pub scope: ScopeGroupKind,
    pub paths: Vec<ScopeGroupItem>,
    pub packages: IndexMap<JsString, Vec<ScopeGroupItem>>,
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
            .unwrap_or_else(|| JsString::from("local"));
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
    format_path: impl Fn(&ScopeGroupItem) -> JsString,
    format_package_path: impl Fn(&ScopeGroupItem, &JsString) -> JsString,
) -> JsString {
    with_theme(|theme| {
        let mut lines: Vec<JsString> = Vec::new();
        for group in groups {
            lines.push(js_format!("  {}", theme.fg(ThemeColor::Accent, group.scope.as_str())));

            let mut sorted_paths: Vec<&ScopeGroupItem> = group.paths.iter().collect();
            sorted_paths.sort_by(|left, right| {
                pi_js_compat::collator::locale_compare_utf16(left.path.units(), right.path.units())
            });
            for item in sorted_paths {
                lines.push(theme.fg(ThemeColor::Dim, &js_format!("    {}", format_path(item))));
            }

            let mut sorted_packages: Vec<(&JsString, &Vec<ScopeGroupItem>)> = group.packages.iter().collect();
            sorted_packages.sort_by(|(left, _), (right, _)| {
                pi_js_compat::collator::locale_compare_utf16(left.units(), right.units())
            });
            for (source, items) in sorted_packages {
                lines.push(js_format!("    {}", theme.fg(ThemeColor::MdLink, source)));
                let mut sorted_items: Vec<&ScopeGroupItem> = items.iter().collect();
                sorted_items.sort_by(|left, right| {
                    pi_js_compat::collator::locale_compare_utf16(left.path.units(), right.path.units())
                });
                for item in sorted_items {
                    lines.push(theme.fg(
                        ThemeColor::Dim,
                        &js_format!("      {}", format_package_path(item, source)),
                    ));
                }
            }
        }
        JsString::join(lines, "\n")
    })
}

pub(crate) fn find_source_info_for_path<'a>(
    path: &JsString,
    source_infos: &'a HashMap<JsString, SourceInfo>,
) -> Option<&'a SourceInfo> {
    if let Some(exact) = source_infos.get(path) {
        return Some(exact);
    }
    let mut current = path.clone();
    while let Some(index) = current.units().iter().rposition(|unit| *unit == 0x2f) {
        current = current.slice(0, index);
        if let Some(parent) = source_infos.get(&current) {
            return Some(parent);
        }
    }
    None
}

pub(crate) fn format_path_with_source(path: &JsString, source_info: Option<&SourceInfo>) -> JsString {
    if let Some(source_info) = source_info {
        let short_path = get_short_path(path, Some(source_info));
        let display = get_display_source_info(Some(source_info));
        let label_text = match display.scope_label {
            Some(scope_label) => js_format!("{} ({})", display.label, scope_label),
            None => display.label,
        };
        return js_format!("{} {}", label_text, short_path);
    }
    format_display_path(path)
}

pub(crate) fn format_diagnostics(
    diagnostics: &[ResourceDiagnostic],
    source_infos: &HashMap<JsString, SourceInfo>,
) -> JsString {
    with_theme(|theme| {
        let mut lines: Vec<JsString> = Vec::new();
        let mut collisions: IndexMap<JsString, Vec<&ResourceDiagnostic>> = IndexMap::new();
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

        push_collision_lines(&mut lines, theme, &collisions, source_infos);
        push_other_diagnostic_lines(&mut lines, theme, &other, source_infos);
        JsString::join(lines, "\n")
    })
}

fn push_collision_lines(
    lines: &mut Vec<JsString>,
    theme: &Theme,
    collisions: &IndexMap<JsString, Vec<&ResourceDiagnostic>>,
    source_infos: &HashMap<JsString, SourceInfo>,
) {
    for (name, collision_list) in collisions {
        let Some(first) = collision_list
            .first()
            .and_then(|diagnostic| diagnostic.collision.as_ref())
        else {
            continue;
        };
        lines.push(theme.fg(ThemeColor::Warning, &js_format!("  \"{}\" collision:", name)));
        lines.push(theme.fg(
            ThemeColor::Dim,
            &js_format!(
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
                    &js_format!(
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
}

fn push_other_diagnostic_lines(
    lines: &mut Vec<JsString>,
    theme: &Theme,
    other: &[&ResourceDiagnostic],
    source_infos: &HashMap<JsString, SourceInfo>,
) {
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
                    &js_format!(
                        "  {}",
                        format_path_with_source(path, find_source_info_for_path(path, source_infos))
                    ),
                ));
                lines.push(theme.fg(color, &js_format!("    {}", diagnostic.message)));
            }
            None => lines.push(theme.fg(color, &js_format!("  {}", diagnostic.message))),
        }
    }
}

pub(crate) fn format_compact_list(items: &[JsString], sort: bool) -> JsString {
    let mut labels: Vec<JsString> = items
        .iter()
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect();
    if sort {
        labels.sort_by(|left, right| pi_js_compat::collator::locale_compare_utf16(left.units(), right.units()));
    }
    with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("  {}", JsString::join(labels, ", "))))
}

pub(crate) fn section_header(name: &JsString, color: ThemeColor) -> JsString {
    with_theme(|theme| theme.fg(color, &js_format!("[{}]", name)))
}

/// `ExpandableText`: text whose content depends on the expansion state.
pub struct ExpandableText {
    collapsed_text: JsString,
    expanded_text: JsString,
    expanded: bool,
    padding_x: usize,
    padding_y: usize,
    text: pi_tui::components::text::Text,
}

impl ExpandableText {
    pub fn new(
        collapsed_text: impl Fn() -> JsString + 'static,
        expanded_text: impl Fn() -> JsString + 'static,
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
    fn render(&mut self, width: usize) -> Vec<JsString> {
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
    Main(Rc<TUI>),
}

impl Renderer {
    /// Clones the renderer handle so callers never hold the `RefCell` borrow
    /// across a call that can re-enter the mode (for example input handlers
    /// that switch TUI mode).
    fn clone_ref(&self) -> Self {
        match self {
            Self::Main(tui) => Self::Main(Rc::clone(tui)),
        }
    }

    pub fn mode(&self) -> TuiMode {
        match self {
            Self::Main(_) => TuiMode::Regular,
        }
    }

    pub fn terminal(&self) -> Rc<RefCell<dyn Terminal>> {
        match self {
            Self::Main(tui) => TuiExt::terminal(tui.as_ref()),
        }
    }

    pub fn start(&self) -> Result<(), TerminalRawModeError> {
        match self {
            Self::Main(tui) => TuiExt::start(tui.as_ref()),
        }
    }

    pub fn stop(&self) {
        match self {
            Self::Main(tui) => TuiExt::stop(tui.as_ref()),
        }
    }
}

/// `InteractiveTuiOptions`.
pub struct InteractiveTuiOptions {
    pub show_hardware_cursor: bool,
    pub log_directory: JsString,
    pub terminal: Option<Rc<RefCell<dyn Terminal>>>,
    pub on_right_click_paste: Option<Rc<dyn Fn()>>,
}

/// Composition root for selecting the interactive terminal renderer.
pub fn create_interactive_tui(options: InteractiveTuiOptions) -> Renderer {
    let terminal = options
        .terminal
        .unwrap_or_else(|| Rc::new(RefCell::new(ProcessTerminal::new())));
    Renderer::Main(TUI::with_options(
        terminal,
        Some(options.show_hardware_cursor),
        Some(PathBuf::from(encode_node_utf8(&options.log_directory))),
    ))
}

/// Stable TUI handle used by components while the mode replaces the renderer.
pub struct InteractiveTuiHandle {
    renderer: RefCell<Renderer>,
}

impl InteractiveTuiHandle {
    /// Clone the active renderer out of the cell so dispatching input or
    /// rendering cannot re-enter the `RefCell` borrow.
    fn renderer(&self) -> Renderer {
        match &*self.renderer.borrow() {
            Renderer::Main(tui) => Renderer::Main(Rc::clone(tui)),
        }
    }

    pub fn new(renderer: Renderer) -> Rc<Self> {
        Rc::new(Self {
            renderer: RefCell::new(renderer),
        })
    }

    pub fn replace_renderer(&self, renderer: Renderer) {
        *self.renderer.borrow_mut() = renderer;
    }

    pub fn mode(&self) -> TuiMode {
        self.renderer.borrow().clone_ref().mode()
    }

    pub fn terminal(&self) -> Rc<RefCell<dyn Terminal>> {
        self.renderer.borrow().clone_ref().terminal()
    }

    pub fn add_child(&self, child: ComponentRef) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::add_child(tui.as_ref(), child),
        }
    }

    pub fn clear(&self) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::clear(tui.as_ref()),
        }
    }

    pub fn set_focus(&self, component: Option<ComponentRef>) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::set_focus(tui.as_ref(), component),
        }
    }

    pub fn get_focused_component(&self) -> Option<ComponentRef> {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::get_focused_component(tui.as_ref()),
        }
    }

    pub fn request_render(&self) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::request_render(tui.as_ref(), false),
        }
    }

    pub fn render_now(&self) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::render_now(tui.as_ref(), true),
        }
    }

    pub fn invalidate(&self) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::invalidate(tui.as_ref()),
        }
    }

    pub fn start(&self) -> Result<(), TerminalRawModeError> {
        self.renderer.borrow().clone_ref().start()
    }

    pub fn stop(&self) {
        self.renderer.borrow().clone_ref().stop();
    }

    pub fn stop_with(&self, options: pi_tui::tui::TuiStopOptions) {
        let renderer = self.renderer.borrow().clone_ref();
        match &renderer {
            Renderer::Main(tui) => TuiExt::stop_with(tui.as_ref(), options),
        }
    }

    pub fn flush_pending(&self) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::flush_pending(tui.as_ref()),
        }
    }

    pub fn process_events(&self) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::process_events(tui.as_ref()),
        }
    }

    pub fn get_show_hardware_cursor(&self) -> bool {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::get_show_hardware_cursor(tui.as_ref()),
        }
    }

    pub fn get_clear_on_shrink(&self) -> bool {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::get_clear_on_shrink(tui.as_ref()),
        }
    }

    pub fn set_show_hardware_cursor(&self, enabled: bool) {
        let renderer = self.renderer.borrow().clone_ref();
        match &renderer {
            Renderer::Main(tui) => TuiExt::set_show_hardware_cursor(tui.as_ref(), enabled),
        }
    }

    pub fn set_clear_on_shrink(&self, enabled: bool) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::set_clear_on_shrink(tui.as_ref(), enabled),
        }
    }

    pub fn request_render_force(&self) {
        let renderer = self.renderer.borrow().clone_ref();
        match &renderer {
            Renderer::Main(tui) => TuiExt::request_render(tui.as_ref(), true),
        }
    }

    pub fn set_terminal_color_scheme_notifications(&self, enabled: bool) {
        let renderer = self.renderer.borrow().clone_ref();
        match &renderer {
            Renderer::Main(tui) => TuiExt::set_terminal_color_scheme_notifications(tui.as_ref(), enabled),
        }
    }

    pub fn on_terminal_color_scheme_change(
        &self,
        listener: Rc<dyn Fn(pi_tui::terminal_colors::TerminalColorScheme)>,
    ) -> usize {
        let renderer = self.renderer.borrow().clone_ref();
        match &renderer {
            Renderer::Main(tui) => TuiExt::on_terminal_color_scheme_change(tui.as_ref(), listener),
        }
    }

    pub fn remove_terminal_color_scheme_listener(&self, id: usize) {
        let renderer = self.renderer.borrow().clone_ref();
        match &renderer {
            Renderer::Main(tui) => tui.base().remove_terminal_color_scheme_listener(id),
        }
    }

    pub fn query_terminal_background_color(
        &self,
        timeout: Duration,
    ) -> Option<Option<pi_tui::terminal_colors::RgbColor>> {
        let renderer = self.renderer.borrow().clone_ref();
        let query = match &renderer {
            Renderer::Main(tui) => TuiExt::query_terminal_background_color(tui.as_ref(), timeout),
        };
        self.wait_for_terminal_query(|| query.result())
    }

    pub fn query_terminal_color_scheme(
        &self,
        timeout: Duration,
    ) -> Option<Option<pi_tui::terminal_colors::TerminalColorScheme>> {
        let renderer = self.renderer.borrow().clone_ref();
        let query = match &renderer {
            Renderer::Main(tui) => TuiExt::query_terminal_color_scheme(tui.as_ref(), timeout),
        };
        self.wait_for_terminal_query(|| query.result())
    }

    fn wait_for_terminal_query<T>(&self, result: impl Fn() -> Option<T>) -> Option<T> {
        loop {
            self.process_events();
            if let Some(value) = result() {
                return Some(value);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    pub fn show_overlay(&self, component: ComponentRef, options: Option<OverlayOptions>) -> OverlayHandle {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::show_overlay(tui.as_ref(), component, options),
        }
    }

    pub fn hide_overlay(&self) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::hide_overlay(tui.as_ref()),
        }
    }

    pub fn has_overlay_entries(&self) -> bool {
        match self.renderer() {
            Renderer::Main(tui) => tui.base().has_overlay_entries(),
        }
    }

    pub fn add_input_listener(&self, listener: Rc<dyn Fn(&JsString) -> TuiInputListenerResult>) -> usize {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::add_input_listener(tui.as_ref(), listener),
        }
    }

    pub fn remove_input_listener(&self, id: usize) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::remove_input_listener(tui.as_ref(), id),
        }
    }

    pub fn set_on_debug(&self, handler: Rc<dyn Fn()>) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::set_on_debug(tui.as_ref(), handler),
        }
    }

    pub fn create_flow_render_region(&self, component: ComponentRef) -> Option<FlowRenderRegion> {
        match self.renderer() {
            Renderer::Main(tui) => Some(TUI::create_flow_render_region(&tui, component)),
        }
    }

    pub fn create_render_region(&self, component: ComponentRef) -> Option<RenderRegion> {
        match self.renderer() {
            Renderer::Main(tui) => Some(TUI::create_render_region(&tui, component)),
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
            Renderer::Main(tui) => Rc::new(TUI::create_fixed_cell(tui)),
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
        }
    }
}

/// `TUI` surface consumed by the interactive theme controller.
struct InteractiveThemeUi(Rc<InteractiveTuiHandle>);

impl crate::modes::interactive::theme::ThemeUi for InteractiveThemeUi {
    fn invalidate(&self) {
        self.0.invalidate();
    }

    fn request_render(&self) {
        self.0.request_render();
    }

    fn set_terminal_color_scheme_notifications(&self, enabled: bool) {
        self.0.set_terminal_color_scheme_notifications(enabled);
    }

    fn on_terminal_color_scheme_change(
        &self,
        listener: Rc<dyn Fn(crate::modes::interactive::theme::TerminalTheme)>,
    ) -> usize {
        self.0.on_terminal_color_scheme_change(Rc::new(move |scheme| {
            let theme = match scheme {
                pi_tui::terminal_colors::TerminalColorScheme::Dark => {
                    crate::modes::interactive::theme::TerminalTheme::Dark
                }
                pi_tui::terminal_colors::TerminalColorScheme::Light => {
                    crate::modes::interactive::theme::TerminalTheme::Light
                }
            };
            listener(theme);
        }))
    }

    fn remove_terminal_color_scheme_listener(&self, id: usize) {
        self.0.remove_terminal_color_scheme_listener(id);
    }

    fn query_terminal_background_color(
        &self,
        timeout: Duration,
    ) -> Result<Option<pi_tui::terminal_colors::RgbColor>, crate::modes::interactive::theme::ThemeQueryError> {
        Ok(self.0.query_terminal_background_color(timeout).flatten())
    }

    fn query_terminal_color_scheme(
        &self,
        timeout: Duration,
    ) -> Option<
        Result<
            Option<crate::modes::interactive::theme::TerminalTheme>,
            crate::modes::interactive::theme::ThemeQueryError,
        >,
    > {
        Some(Ok(self.0.query_terminal_color_scheme(timeout).flatten().map(
            |scheme| match scheme {
                pi_tui::terminal_colors::TerminalColorScheme::Dark => {
                    crate::modes::interactive::theme::TerminalTheme::Dark
                }
                pi_tui::terminal_colors::TerminalColorScheme::Light => {
                    crate::modes::interactive::theme::TerminalTheme::Light
                }
            },
        )))
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
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0.borrow_mut().render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.0.borrow_mut().invalidate();
    }
}

/// Adapter mounting the login dialog while the auth bridge mutates it.
pub struct SharedLoginDialog(pub Rc<RefCell<LoginDialogComponent>>);

impl Component for SharedLoginDialog {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0
            .try_borrow_mut()
            .map(|mut dialog| dialog.render(width))
            .unwrap_or_default()
    }

    fn handle_input(&mut self, data: &JsString) {
        if let Ok(mut dialog) = self.0.try_borrow_mut() {
            dialog.handle_input(data);
        }
    }

    fn invalidate(&mut self) {
        if let Ok(mut dialog) = self.0.try_borrow_mut() {
            dialog.invalidate();
        }
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}

impl Focusable for SharedLoginDialog {
    fn focused(&self) -> bool {
        self.0.try_borrow().map(|dialog| dialog.focused()).unwrap_or(false)
    }

    fn set_focused(&mut self, focused: bool) {
        if let Ok(mut dialog) = self.0.try_borrow_mut() {
            dialog.set_focused(focused);
        }
    }
}

/// Adapter mounting the share loader while the command mutates it.
pub struct SharedBorderedLoader(pub Rc<RefCell<BorderedLoader>>);

impl Component for SharedBorderedLoader {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0
            .try_borrow_mut()
            .map(|mut loader| loader.render(width))
            .unwrap_or_default()
    }

    fn handle_input(&mut self, data: &JsString) {
        if let Ok(mut loader) = self.0.try_borrow_mut() {
            loader.handle_input(data);
        }
    }

    fn invalidate(&mut self) {
        if let Ok(mut loader) = self.0.try_borrow_mut() {
            loader.invalidate();
        }
    }
}

/// Adapter mounting the session selector while its async loads mutate it.
pub struct SharedSessionSelector(pub Rc<tokio::sync::Mutex<SessionSelectorComponent>>);

impl Component for SharedSessionSelector {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0
            .try_lock()
            .map(|mut selector| selector.render(width))
            .unwrap_or_default()
    }

    fn handle_input(&mut self, data: &JsString) {
        if let Ok(mut selector) = self.0.try_lock() {
            selector.handle_input(data);
        }
    }

    fn wants_input(&self) -> bool {
        self.0.try_lock().map(|selector| selector.wants_input()).unwrap_or(true)
    }

    fn invalidate(&mut self) {
        if let Ok(mut selector) = self.0.try_lock() {
            selector.invalidate();
        }
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}

impl Focusable for SharedSessionSelector {
    fn focused(&self) -> bool {
        self.0.try_lock().map(|selector| selector.focused()).unwrap_or(false)
    }

    fn set_focused(&mut self, focused: bool) {
        if let Ok(mut selector) = self.0.try_lock() {
            selector.set_focused(focused);
        }
    }
}

/// Adapter mounting the current editor through the component contract.
pub struct AnyEditorHandle(pub Rc<RefCell<dyn EditorComponent>>);

impl Component for AnyEditorHandle {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0.borrow_mut().render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        let callbacks = self.0.borrow_mut().handle_input_deferred(data);
        for callback in callbacks {
            callback();
        }
    }

    fn wants_input(&self) -> bool {
        true
    }

    fn invalidate(&mut self) {
        self.0.borrow_mut().invalidate();
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        self.0.borrow_mut().as_focusable_mut()?;
        Some(self)
    }
}

impl Focusable for AnyEditorHandle {
    fn focused(&self) -> bool {
        self.0
            .borrow_mut()
            .as_focusable_mut()
            .is_some_and(|editor| editor.focused())
    }

    fn set_focused(&mut self, focused: bool) {
        if let Some(editor) = self.0.borrow_mut().as_focusable_mut() {
            editor.set_focused(focused);
        }
    }
}

/// Callback receiving submitted main-loop input.
pub type InputCallback = Rc<dyn Fn(JsString)>;

/// `InteractiveModeOptions`.
pub type SteerMultiAgent =
    Arc<dyn Fn(JsString, JsString, JsString) -> BoxFuture<'static, Result<(), JsString>> + Send + Sync>;
pub type CancelMultiAgent = Arc<dyn Fn(JsString) -> BoxFuture<'static, Result<(), JsString>> + Send + Sync>;
pub type AgentViewSelectorSlot =
    Arc<std::sync::Mutex<Option<crate::bundled_extensions::agents_core::runtime::SelectAgentView>>>;

#[derive(Default)]
pub struct InteractiveModeOptions {
    pub migrated_providers: Vec<JsString>,
    pub model_fallback_message: Option<JsString>,
    pub auto_trust_on_reload_cwd: Option<JsString>,
    pub initial_message: Option<JsString>,
    pub initial_images: Vec<ImageContent>,
    pub initial_messages: Vec<JsString>,
    pub control_message: Option<crate::core::session_control_db::IncomingControlMessage>,
    pub verbose: bool,
    pub control_db_path: Option<JsString>,
    /// Shared first-party multi-agent store for agent slot selection.
    pub multi_agent_store: Option<Arc<MultiAgentStore>>,
    /// Wake an active multi-agent wait after main-session steering is queued.
    pub wake_wait_agents_after_steering: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Submit editor text to the selected agent through the lifecycle coordinator.
    pub steer_multi_agent: Option<SteerMultiAgent>,
    /// Request cancellation of the selected agent's runtime.
    pub cancel_multi_agent: Option<CancelMultiAgent>,
    /// Slot filled with the bridge selector used by `agent_viewer`.
    pub agent_view_selector_slot: Option<AgentViewSelectorSlot>,
}

/// `PromptActivitySource`: non-streaming work that keeps the prompt animated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PromptActivitySource {
    Bash,
    BranchSummary,
    Compaction,
    Retry,
    SummaryMaterialization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompactionQueuedMessage {
    text: JsString,
    mode: &'static str,
}

/// Async operations queued by synchronous TUI callbacks.
enum ModeAction {
    Submit(JsString),
    Shutdown,
    ExternalEditor,
    FollowUp,
    Dequeue,
    CycleModel(CycleDirection),
    NewSession,
    Copy,
}

/// Shared, thread-safe counters used by the extension UI bridge.
pub struct UiShared {
    rows: std::sync::atomic::AtomicUsize,
    columns: std::sync::atomic::AtomicUsize,
    editor_component_factory: std::sync::Mutex<Option<ExtensionEditorFactory>>,
}

impl UiShared {
    fn new() -> Self {
        Self {
            rows: std::sync::atomic::AtomicUsize::new(24),
            columns: std::sync::atomic::AtomicUsize::new(80),
            editor_component_factory: std::sync::Mutex::new(None),
        }
    }
}

/// Requests crossing from extension threads into the TUI thread.
enum UiRequest {
    SelectAgentView {
        agent_id: JsString,
        respond: tokio::sync::oneshot::Sender<bool>,
    },
    Select {
        title: JsString,
        options: Vec<JsString>,
        timeout: Option<f64>,
        respond: tokio::sync::oneshot::Sender<Option<JsString>>,
    },
    Confirm {
        title: JsString,
        message: JsString,
        timeout: Option<f64>,
        respond: tokio::sync::oneshot::Sender<bool>,
    },
    Input {
        title: JsString,
        placeholder: Option<JsString>,
        timeout: Option<f64>,
        secret: bool,
        respond: tokio::sync::oneshot::Sender<Option<JsString>>,
    },
    Editor {
        title: JsString,
        prefill: Option<JsString>,
        respond: tokio::sync::oneshot::Sender<Option<JsString>>,
    },
    Notify {
        message: JsString,
        notify_type: Option<NotifyType>,
    },
    RequestRender,
    RequestRenderForce,
    SetStatus {
        key: JsString,
        text: Option<JsString>,
    },
    SetWorkingMessage(Option<JsString>),
    SetWorkingVisible(bool),
    SetWorkingIndicator(Option<WorkingIndicatorOptions>),
    SetHiddenThinkingLabel(Option<JsString>),
    SetWidget {
        key: JsString,
        content: Option<Vec<JsString>>,
        placement: WidgetPlacement,
    },
    SetWidgetComponent {
        key: JsString,
        content: Option<ExtensionUiComponentFactory>,
        placement: WidgetPlacement,
    },
    SetTitle(JsString),
    PasteToEditor(JsString),
    SetEditorText(JsString),
    SetToolsExpanded(bool),
    SetFooter(Option<ExtensionUiFooterFactory>),
    SetDefaultFooter(Option<ExtensionUiFooterFactory>),
    SetHeader(Option<ExtensionUiComponentFactory>),
    Custom {
        factory: ExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
        respond: tokio::sync::oneshot::Sender<Result<Option<JsValue>, JsString>>,
    },
    AddTerminalInput {
        handler: TerminalInputHandler,
        respond: std::sync::mpsc::Sender<usize>,
    },
    RemoveTerminalInput(usize),
    AddAutocompleteProvider(ExtensionAutocompleteProviderFactory),
    SetEditorComponent(Option<ExtensionEditorFactory>),
    ShowApprovalSelector,
    ShowSandboxSelector,
    RuntimeNewSession {
        options: Option<crate::core::extensions::types::NewSessionOptions>,
        respond: tokio::sync::oneshot::Sender<Result<SessionTransitionResult, JsString>>,
    },
    RuntimeFork {
        entry_id: JsString,
        options: Option<crate::core::extensions::types::ForkOptions>,
        respond: tokio::sync::oneshot::Sender<Result<SessionTransitionResult, JsString>>,
    },
    RuntimeNavigateTree {
        entry_id: JsString,
        options: Option<crate::core::extensions::types::NavigateTreeOptions>,
        respond: tokio::sync::oneshot::Sender<Result<SessionTransitionResult, JsString>>,
    },
    RuntimeSwitchSession {
        path: JsString,
        respond: tokio::sync::oneshot::Sender<Result<SessionTransitionResult, JsString>>,
    },
    RuntimeReload {
        respond: tokio::sync::oneshot::Sender<()>,
    },
    RuntimeRestart {
        options: Option<crate::core::extensions::types::RestartOptions>,
        respond: tokio::sync::oneshot::Sender<Result<(), crate::core::extensions::types::HandlerError>>,
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

/// `resolveEnabledModelsPersistTarget`.
fn resolve_enabled_models_persist_target(
    project_enabled_models: Option<&JsValue>,
    patterns: Option<Vec<JsString>>,
) -> (Option<Vec<JsString>>, crate::core::settings_manager::SettingsScope) {
    use crate::core::settings_manager::SettingsScope;
    if project_enabled_models.is_none() {
        return (patterns, SettingsScope::Global);
    }
    (Some(patterns.unwrap_or_default()), SettingsScope::Project)
}

/// `sortNamedSessionsFirst`: order named sessions first, then by modification time.
fn sort_named_sessions_first(
    sessions: Vec<crate::core::session_manager::SessionInfo>,
) -> Vec<crate::core::session_manager::SessionInfo> {
    let mut sessions = sessions;
    sessions.sort_by(|a, b| {
        let a_named = session_has_name(a);
        let b_named = session_has_name(b);
        if a_named != b_named {
            return if a_named {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        b.modified.cmp(&a.modified)
    });
    sessions
}

fn session_has_name(session: &crate::core::session_manager::SessionInfo) -> bool {
    session.name.as_ref().is_some_and(|name| !name.trim().is_empty())
}

fn tree_filter_mode(mode: TreeFilterMode) -> FilterMode {
    match mode {
        TreeFilterMode::Default => FilterMode::Default,
        TreeFilterMode::NoTools => FilterMode::NoTools,
        TreeFilterMode::UserOnly => FilterMode::UserOnly,
        TreeFilterMode::LabeledOnly => FilterMode::LabeledOnly,
        TreeFilterMode::All => FilterMode::All,
    }
}

fn with_mode<R>(id: usize, f: impl FnOnce(Rc<InteractiveMode>) -> R) -> Option<R> {
    MODE_REGISTRY.with(|registry| {
        let mode = registry.borrow().get(&id).and_then(std::rc::Weak::upgrade)?;
        Some(f(mode))
    })
}

/// Completes an extension UI request with the value the mode sends back.
async fn await_ui_response<T, E: Into<JsString>>(
    receiver: tokio::sync::oneshot::Receiver<Result<T, E>>,
) -> Result<T, HandlerError> {
    receiver
        .await
        .map_err(|error| HandlerError::new(error.to_string()))?
        .map_err(HandlerError::new)
}

/// Sends a pending UI response once, then drops the shared slot.
fn respond_once<T>(responder: &RefCell<Option<tokio::sync::oneshot::Sender<T>>>, value: T) {
    if let Some(respond) = responder.borrow_mut().take() {
        let _ = respond.send(value);
    }
}

/// Cancels a selector, restoring the editor and repainting.
fn cancel_restores_editor(mode_id: usize) -> Rc<dyn Fn()> {
    Rc::new(move || {
        with_mode(mode_id, |mode| {
            mode.restore_editor();
            mode.ui.request_render();
        });
    })
}

/// TUI-thread implementation of `ExtensionUiContext`.
struct InteractiveUiContext {
    id: usize,
    sender: std::sync::mpsc::Sender<UiRequest>,
    shared: Arc<UiShared>,
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
        title: &JsString,
        options: &[JsString],
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, JsString>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Select {
            title: title.to_owned(),
            options: options.to_vec(),
            timeout: opts.and_then(|opts| opts.timeout),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| JsString::from(error.to_string())) })
    }

    fn confirm<'a>(
        &'a self,
        title: &JsString,
        message: &JsString,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, JsString>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Confirm {
            title: title.to_owned(),
            message: message.to_owned(),
            timeout: opts.and_then(|opts| opts.timeout),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| JsString::from(error.to_string())) })
    }

    fn input<'a>(
        &'a self,
        title: &JsString,
        placeholder: Option<&JsString>,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, JsString>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Input {
            title: title.to_owned(),
            placeholder: placeholder.cloned(),
            timeout: opts.and_then(|opts| opts.timeout),
            secret: opts.and_then(|opts| opts.secret).unwrap_or(false),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| JsString::from(error.to_string())) })
    }

    fn notify(&self, message: &JsString, notify_type: Option<NotifyType>) {
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

    fn set_status(&self, key: &JsString, text: Option<&JsString>) {
        self.send(UiRequest::SetStatus {
            key: key.to_owned(),
            text: text.cloned(),
        });
    }

    fn set_working_message(&self, message: Option<&JsString>) {
        self.send(UiRequest::SetWorkingMessage(message.cloned()));
    }

    fn set_working_visible(&self, visible: bool) {
        self.send(UiRequest::SetWorkingVisible(visible));
    }

    fn set_working_indicator(&self, options: Option<&WorkingIndicatorOptions>) {
        self.send(UiRequest::SetWorkingIndicator(options.cloned()));
    }

    fn set_hidden_thinking_label(&self, label: Option<&JsString>) {
        self.send(UiRequest::SetHiddenThinkingLabel(label.cloned()));
    }

    fn set_widget(&self, key: &JsString, content: Option<&[JsString]>, options: Option<&ExtensionWidgetOptions>) {
        self.send(UiRequest::SetWidget {
            key: key.to_owned(),
            content: content.map(<[JsString]>::to_vec),
            placement: options
                .and_then(|options| options.placement)
                .unwrap_or(WidgetPlacement::AboveEditor),
        });
    }

    fn set_widget_component(
        &self,
        key: &JsString,
        content: Option<ExtensionUiComponentFactory>,
        options: Option<&ExtensionWidgetOptions>,
    ) {
        self.send(UiRequest::SetWidgetComponent {
            key: key.to_owned(),
            content,
            placement: options
                .and_then(|options| options.placement)
                .unwrap_or(WidgetPlacement::AboveEditor),
        });
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
    ) -> BoxFuture<'a, Result<Option<JsValue>, JsString>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Custom {
            factory,
            options,
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| JsString::from(error.to_string()))? })
    }

    fn set_title(&self, title: &JsString) {
        self.send(UiRequest::SetTitle(title.to_owned()));
    }

    fn paste_to_editor(&self, text: &JsString) {
        self.send(UiRequest::PasteToEditor(text.to_owned()));
    }

    fn set_editor_text(&self, text: &JsString) {
        self.send(UiRequest::SetEditorText(text.to_owned()));
    }

    fn get_editor_text(&self) -> JsString {
        with_mode(self.id, |mode| mode.editor().borrow().get_expanded_text()).unwrap_or_default()
    }

    fn add_autocomplete_provider(&self, factory: ExtensionAutocompleteProviderFactory) {
        self.send(UiRequest::AddAutocompleteProvider(factory));
    }

    fn set_editor_component(&self, factory: Option<ExtensionEditorFactory>) {
        self.send(UiRequest::SetEditorComponent(factory));
    }

    fn get_editor_component(&self) -> Option<ExtensionEditorFactory> {
        self.shared
            .editor_component_factory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn editor<'a>(
        &'a self,
        title: &JsString,
        prefill: Option<&JsString>,
    ) -> BoxFuture<'a, Result<Option<JsString>, JsString>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Editor {
            title: title.to_owned(),
            prefill: prefill.cloned(),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| JsString::from(error.to_string())) })
    }

    fn get_all_themes(&self) -> Vec<ThemeInfo> {
        crate::modes::interactive::theme::get_available_themes_with_paths()
            .unwrap_or_else(|error| {
                crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error.message())
            })
            .into_iter()
            .map(|info| crate::core::extensions::types::ThemeInfo {
                name: info.name,
                path: info.path,
            })
            .collect()
    }

    fn get_theme(&self, name: &JsString) -> Option<Arc<Theme>> {
        crate::modes::interactive::theme::get_theme_by_name(name)
    }

    fn set_theme(&self, name: &JsString) -> ThemeSetResult {
        with_mode(self.id, |mode| mode.set_theme_name(name)).unwrap_or(ThemeSetResult {
            success: false,
            error: Some("UI not available".into()),
        })
    }

    fn set_theme_object(&self, theme: Arc<Theme>) -> ThemeSetResult {
        with_mode(self.id, |mode| mode.set_theme_instance(theme)).unwrap_or(ThemeSetResult {
            success: false,
            error: Some("UI not available".into()),
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

// Factories delivered through UiRequest stay Send + Sync; direct UI-thread factories need not.
type LocalExtensionUiCustomFactory = Rc<
    dyn Fn(
        crate::core::extensions::ExtensionUiFactoryContext,
        ExtensionUiDone,
    ) -> crate::core::extensions::ExtensionUiCustomResult,
>;

/// Thread-safe TUI handle passed to extension component factories.
struct InteractiveUiTuiBackend {
    mode_id: usize,
    shared: Arc<UiShared>,
    sender: std::sync::mpsc::Sender<UiRequest>,
}

impl ExtensionUiTuiBackend for InteractiveUiTuiBackend {
    fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        with_mode(self.mode_id, |mode| {
            pi_tui::components::editor::EditorHost::create_fixed_cell(&InteractiveTuiHost(Rc::clone(&mode.ui)))
        })
        .expect("extension fixed cells must be created on the owning live UI thread")
    }

    fn show_overlay(
        &self,
        inner: Box<dyn ExtensionUiComponent>,
        options: Option<crate::core::extensions::ExtensionUiOverlayOptions>,
    ) -> Rc<dyn crate::core::extensions::ExtensionUiOverlayHandle> {
        with_mode(self.mode_id, |mode| {
            let component = component(ExtensionComponentAdapter { inner: inner.into() });
            Rc::new(InteractiveOverlayHandle(
                mode.ui.show_overlay(component, options.map(Into::into)),
            )) as Rc<dyn crate::core::extensions::ExtensionUiOverlayHandle>
        })
        .expect("extension overlays must be created on the owning live UI thread")
    }

    fn request_render(&self) {
        let _ = self.sender.send(UiRequest::RequestRender);
    }

    fn request_render_force(&self) {
        let _ = self.sender.send(UiRequest::RequestRenderForce);
    }

    fn stop(&self) {
        with_mode(self.mode_id, |mode| mode.ui.stop()).expect("extension TUI stop requires the owning live UI thread");
    }

    fn start(&self) -> Result<(), JsString> {
        with_mode(self.mode_id, |mode| {
            mode.ui.start().map_err(|error| JsString::from(error.to_string()))
        })
        .expect("extension TUI start requires the owning live UI thread")
    }

    fn terminal_rows(&self) -> usize {
        self.shared.rows.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn terminal_columns(&self) -> usize {
        self.shared.columns.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn mode(&self) -> TuiMode {
        TuiMode::Regular
    }
}

/// Extension widget content: string lines or a factory-built component.
enum ExtensionWidget {
    Lines(Vec<JsString>),
    Component(ComponentRef),
}

/// Maximum widget lines before the truncation marker (source `MAX_WIDGET_LINES`).
const MAX_WIDGET_LINES: usize = 10;

/// Adapts a UI-local extension component to the `pi-tui` component contract.
struct ExtensionComponentAdapter {
    inner: Rc<dyn ExtensionUiComponent>,
}

/// Source `tui.ts` overlay handle, without moving its `Rc` state off-thread.
struct InteractiveOverlayHandle(OverlayHandle);

impl crate::core::extensions::ExtensionUiOverlayHandle for InteractiveOverlayHandle {
    fn hide(&self) {
        self.0.hide();
    }
    fn set_hidden(&self, hidden: bool) {
        self.0.set_hidden(hidden);
    }
    fn is_hidden(&self) -> bool {
        self.0.is_hidden()
    }
    fn focus(&self) {
        self.0.focus();
    }
    fn unfocus(&self) {
        self.0.unfocus();
    }
    fn unfocus_to(&self, target: Option<ComponentRef>) {
        self.0.unfocus_to(target);
    }
    fn is_focused(&self) -> bool {
        self.0.is_focused()
    }
}

impl Focusable for ExtensionComponentAdapter {
    fn focused(&self) -> bool {
        self.inner.focused() == Some(true)
    }

    fn set_focused(&mut self, focused: bool) {
        self.inner.set_focused(focused);
    }
}

impl Component for ExtensionComponentAdapter {
    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        self.inner.focused()?;
        Some(self)
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.inner.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.inner.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.inner.invalidate();
    }

    fn wants_input(&self) -> bool {
        self.inner.wants_input()
    }

    fn wants_key_release(&self) -> bool {
        self.inner.wants_key_release()
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

thread_local! {
    /// The mode currently driving this thread's UI. `onUserMessagePrepared`
    /// must be `Send + Sync` because `PromptOptions` is shared with the core
    /// session, but the callback runs on the UI thread; this lets the
    /// no-capture callback reach the active mode without changing that API.
    static ACTIVE_INTERACTIVE_MODE: RefCell<Option<std::rc::Weak<InteractiveMode>>> = const { RefCell::new(None) };
}

/// `onUserMessagePrepared` callback: renders the submitted user bubble before
/// the turn starts. The callback captures nothing, so it satisfies the
/// `Send + Sync` callback type while reaching the thread-local mode.
fn prepared_user_message_callback() -> crate::core::agent_session::UserMessagePreparedCallback {
    Arc::new(|message: &AgentMessage| {
        ACTIVE_INTERACTIVE_MODE.with(|slot| {
            if let Some(mode) = slot.borrow().as_ref().and_then(std::rc::Weak::upgrade) {
                mode.render_prepared_user_message(message);
            }
        });
    })
}

// ============================================================================
// InteractiveMode
// ============================================================================

/// UI work produced by a provider auth flow running off the TUI thread.
enum AuthUiEvent {
    Prompt(AuthPrompt, tokio::sync::oneshot::Sender<Result<JsString, JsString>>),
    Notify(AuthEvent),
}

/// `AuthInteraction` backed by the mode's UI loop.
struct InteractiveAuthInteraction {
    sender: std::sync::Mutex<std::sync::mpsc::Sender<AuthUiEvent>>,
    signal: tokio_util::sync::CancellationToken,
}

impl InteractiveAuthInteraction {
    fn send(&self, event: AuthUiEvent) -> Result<(), CallbackError> {
        self.sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .send(event)
            .map_err(|_| CallbackError::new("Login cancelled"))
    }
}

impl AuthInteraction for InteractiveAuthInteraction {
    fn signal(&self) -> tokio_util::sync::CancellationToken {
        self.signal.clone()
    }

    fn prompt(&self, prompt: AuthPrompt) -> pi_ai::request::BoxFuture<'_, Result<JsString, CallbackError>> {
        Box::pin(async move {
            let prompt_signal = prompt.signal.clone();
            let (sender, receiver) = tokio::sync::oneshot::channel();
            self.send(AuthUiEvent::Prompt(prompt, sender))?;
            let result = match prompt_signal {
                Some(signal) => tokio::select! {
                    result = receiver => result,
                    _ = signal.cancelled() => return Err(CallbackError::new("Login cancelled")),
                },
                None => receiver.await,
            };
            match result {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(message)) => Err(CallbackError::new(message)),
                Err(_) => Err(CallbackError::new("Login cancelled")),
            }
        })
    }

    fn notify(&self, event: AuthEvent) {
        let _ = self.send(AuthUiEvent::Notify(event));
    }
}

/// Active auth flow UI state polled by the mode loop.
struct AuthUiState {
    receiver: std::sync::mpsc::Receiver<AuthUiEvent>,
    dialog: Rc<RefCell<LoginDialogComponent>>,
}

/// Which session the status loader reflects (`workingLoaderView`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkingLoaderView {
    Main,
    Child,
}

/// Sets the text of a mounted `Loader` component.
fn set_loader_message(loader: &ComponentRef, message: impl Into<JsString>) {
    if let Some(loader) = loader
        .borrow_mut()
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<Loader>())
    {
        loader.set_message(message);
    }
}

/// `InteractiveMode`: TUI rendering and user interaction for the coding agent.
pub struct InteractiveMode {
    runtime: Arc<tokio::sync::Mutex<AgentSessionRuntime>>,
    session: RefCell<AgentSession>,
    options: InteractiveModeOptions,
    multi_agent_store: Option<Arc<MultiAgentStore>>,
    child_view: RefCell<Option<ChildAgentView>>,
    child_view_watch_events: RefCell<Option<std::sync::mpsc::Receiver<ChildViewWatchEvent>>>,
    child_view_watch_sender: std::sync::mpsc::Sender<ChildViewWatchEvent>,
    multi_agent_updates: RefCell<Option<tokio::sync::mpsc::UnboundedReceiver<AgentSnapshot>>>,
    multi_agent_subscription: RefCell<Option<crate::core::multi_agent_store::AgentSubscription>>,
    agent_slot_input_listener: Cell<Option<usize>>,
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
    transcript_tail_region: RefCell<Option<Rc<FlowRenderRegion>>>,
    status_render_region: RefCell<Option<Rc<RenderRegion>>>,
    editor: RefCell<Rc<RefCell<dyn EditorComponent>>>,
    default_editor: Rc<RefCell<CustomEditor>>,
    editor_handle: RefCell<ComponentRef>,
    footer: Rc<RefCell<FooterComponent>>,
    footer_data_provider: Arc<FooterDataProvider>,
    keybindings: KeybindingsManager,
    version: JsString,
    is_initialized: Cell<bool>,
    actions: RefCell<std::collections::VecDeque<ModeAction>>,
    events: RefCell<Option<tokio::sync::mpsc::UnboundedSender<AgentSessionEvent>>>,
    ui_requests: RefCell<Option<std::sync::mpsc::Receiver<UiRequest>>>,
    ui_sender: std::sync::mpsc::Sender<UiRequest>,
    ui_shared: Arc<UiShared>,
    ui_id: Cell<usize>,
    unsubscribe: Cell<Option<u64>>,
    shutdown_requested: Cell<bool>,
    is_shutting_down: Arc<AtomicBool>,
    signal_handlers: RefCell<Vec<tokio::task::JoinHandle<()>>>,
    tool_output_expanded: Cell<bool>,
    hide_thinking_block: Cell<bool>,
    hidden_thinking_label: RefCell<JsString>,
    is_bash_mode: Cell<bool>,
    streaming_component: RefCell<Option<ComponentRef>>,
    streaming_message: RefCell<Option<AssistantMessage>>,
    pending_tools: RefCell<IndexMap<JsString, ComponentRef>>,
    executing_tool_names: RefCell<IndexMap<JsString, String>>,
    executing_tool_started_at: RefCell<IndexMap<JsString, f64>>,
    completed_tool_timings: RefCell<IndexMap<JsString, (f64, f64)>>,
    extension_statuses: RefCell<IndexMap<JsString, JsString>>,
    extension_widgets_above: RefCell<IndexMap<JsString, ExtensionWidget>>,
    extension_widgets_below: RefCell<IndexMap<JsString, ExtensionWidget>>,
    on_input_callback: RefCell<Option<InputCallback>>,
    last_status_spacer: RefCell<Option<ComponentRef>>,
    last_status_text: RefCell<Option<ComponentRef>>,
    last_sigint_time: Cell<f64>,
    last_escape_time: Cell<f64>,
    working_message: RefCell<Option<JsString>>,
    current_working_default_message: RefCell<JsString>,
    loading_animation: RefCell<Option<ComponentRef>>,
    working_loader_view: RefCell<Option<WorkingLoaderView>>,
    retry_loader: RefCell<Option<ComponentRef>>,
    retry_countdown: RefCell<Option<CountdownTimer>>,
    retry_escape_handler: RefCell<Option<Rc<dyn Fn()>>>,
    auto_compaction_escape_handler: RefCell<Option<Rc<dyn Fn()>>>,
    auto_compaction_loader: RefCell<Option<ComponentRef>>,
    auto_compaction_source_hint: RefCell<Option<crate::core::compaction::CompactionSourceInfo>>,
    thinking_started_at: Cell<Option<f64>>,
    thinking_follows_tool: Cell<bool>,
    thinking_timer_next: Cell<Option<f64>>,
    tool_waiting_timer_next: Cell<Option<f64>>,
    child_activity_timer_next: Cell<Option<f64>>,
    now_millis: RefCell<Rc<dyn Fn() -> f64>>,
    working_visible: Cell<bool>,
    working_indicator_options: RefCell<Option<WorkingIndicatorOptions>>,
    prompt_activity_sources: RefCell<std::collections::BTreeSet<PromptActivitySource>>,
    compaction_queued_messages: RefCell<Vec<CompactionQueuedMessage>>,
    response_complete_notification:
        RefCell<Option<Box<dyn crate::core::desktop_notification::DesktopNotificationHandle>>>,
    custom_footer: RefCell<Option<ComponentRef>>,
    default_extension_footer: RefCell<Option<ComponentRef>>,
    custom_header: RefCell<Option<ComponentRef>>,
    changelog_markdown: RefCell<Option<JsString>>,
    startup_notices_shown: Cell<bool>,
    http_fetch: RefCell<Option<Arc<dyn Fetch>>>,
    anthropic_subscription_warning_shown: Cell<bool>,
    terminal_current_directory: RefCell<Option<JsString>>,
    theme_controller: RefCell<crate::modes::interactive::theme::InteractiveThemeController>,
    debug_handler: RefCell<Option<Rc<dyn Fn()>>>,
    terminal_input_listeners: RefCell<Vec<(usize, TerminalInputHandler)>>,
    autocomplete_provider: RefCell<Option<Rc<dyn pi_tui::autocomplete::AutocompleteProvider>>>,
    autocomplete_provider_wrappers: RefCell<Vec<ExtensionAutocompleteProviderFactory>>,
    editor_component_factory: RefCell<Option<ExtensionEditorFactory>>,
    fd_path: RefCell<Option<JsString>>,
    clipboard_temp_files: RefCell<crate::utils::clipboard_temp_files::ClipboardTempFileTracker>,
    pending_compaction_summary_edit_id: RefCell<Option<JsString>>,
    active_session_selector: RefCell<Option<Rc<tokio::sync::Mutex<SessionSelectorComponent>>>>,
    active_selector_dispose: RefCell<Option<Rc<dyn Fn()>>>,
    active_bash_components: RefCell<Vec<ComponentRef>>,
    provisional_user_messages: RefCell<Vec<(AgentMessage, Vec<ComponentRef>)>>,
    auth_ui: RefCell<Option<AuthUiState>>,
    session_selector_loaded: Cell<bool>,
}

/// A selected child transcript watch event delivered from the notify thread.
enum ChildViewWatchEvent {
    Changed,
    Error,
}

/// Selected child agent transcript shown in place of the main conversation.
struct ChildAgentView {
    transcript_path: Option<JsString>,
    agent_id: JsString,
    session_manager: Option<Arc<std::sync::Mutex<SessionManager>>>,
    watcher: Option<notify::RecommendedWatcher>,
    watch_path: Option<JsString>,
    reload_deadline: Option<Instant>,
    watch_retry_deadline: Option<Instant>,
    reload_retries: u32,
}

/// Agent slot selection keybindings (`AGENT_SLOT_KEYBINDINGS`).
const AGENT_SLOT_KEYBINDINGS: [(&str, i64); 9] = [
    ("app.agent.slot1", 0),
    ("app.agent.slot2", 1),
    ("app.agent.slot3", 2),
    ("app.agent.slot4", 3),
    ("app.agent.slot5", 4),
    ("app.agent.slot6", 5),
    ("app.agent.slot7", 6),
    ("app.agent.slot8", 7),
    ("app.agent.slot9", 8),
];

/// Maximum number of child transcript reload retries after a partial write.
const MAX_CHILD_TRANSCRIPT_RELOAD_RETRIES: u32 = 3;
/// Maximum agent log bytes rendered in the live placeholder.
const MAX_AGENT_LOG_VIEW_BYTES: usize = 64 * 1024;

/// `findReadableAgentFilePath`.
fn find_readable_agent_file_path(agent: &AgentSnapshot, label: &JsString) -> Option<JsString> {
    agent
        .result
        .as_ref()
        .and_then(|result| result.file_refs.as_ref())
        .and_then(|file_refs| {
            file_refs
                .iter()
                .find(|file_ref| {
                    file_ref.label.as_ref() == Some(label) && Path::new(&encode_node_utf8(&file_ref.path)).exists()
                })
                .map(|file_ref| file_ref.path.clone())
        })
}

/// `isCompleteJsonlFile`: an empty file or one ending in a newline.
fn is_complete_jsonl_file(path: &JsString) -> std::io::Result<bool> {
    let metadata = std::fs::metadata(encode_node_utf8(path))?;
    let size = metadata.len();
    if size == 0 {
        return Ok(true);
    }
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(encode_node_utf8(path))?;
    file.seek(SeekFrom::Start(size - 1))?;
    let mut last = [0u8; 1];
    file.read_exact(&mut last)?;
    Ok(last[0] == b'\n')
}

/// Reads a child agent transcript for the selected-agent view.
pub fn read_child_transcript(path: &JsString, session_dir: &JsString) -> Option<Vec<AgentMessage>> {
    if !std::path::Path::new(&encode_node_utf8(path)).exists() {
        return None;
    }
    let manager = SessionManager::open(path, Some(session_dir), None).ok()?;
    Some(manager.build_session_context().messages)
}

fn is_steerable_agent(agent: &AgentSnapshot) -> bool {
    let has_parent = agent.parent_id.as_ref() == Some(&JsString::from("main"));
    let has_transcript = agent
        .transcript
        .as_ref()
        .is_some_and(|transcript| !transcript.session_id.is_empty());
    has_parent
        && has_transcript
        && matches!(
            agent.lifecycle,
            AgentLifecycle::Running | AgentLifecycle::WaitingForInput | AgentLifecycle::SteeringPending
        )
}

fn is_interruptible_lifecycle(lifecycle: AgentLifecycle) -> bool {
    matches!(
        lifecycle,
        AgentLifecycle::Running | AgentLifecycle::SteeringPending | AgentLifecycle::Cancelling
    )
}

impl InteractiveMode {
    /// Builds the interactive mode. Must run inside a tokio runtime context
    /// because [`FooterDataProvider`] captures the current runtime handle.
    pub async fn new(
        runtime: Arc<tokio::sync::Mutex<AgentSessionRuntime>>,
        options: InteractiveModeOptions,
        terminal: Option<Rc<RefCell<dyn Terminal>>>,
    ) -> Result<Rc<Self>, JsString> {
        let session = runtime.lock().await.session().clone();
        let settings_handle = session.settings_manager_handle();
        let settings = session.settings_manager();
        let show_hardware_cursor = settings.get_show_hardware_cursor();
        let clear_on_shrink = settings.get_clear_on_shrink();
        let editor_padding_x = settings.get_editor_padding_x() as usize;
        let autocomplete_max_visible = settings.get_autocomplete_max_visible() as usize;
        let hide_thinking_block = settings.get_hide_thinking_block();
        let _output_pad = settings.get_output_pad();
        let theme_name = settings.get_theme().cloned();
        let agent_dir = session.agent_dir().to_owned();
        drop(settings);

        let mode_slot: Rc<RefCell<Option<std::rc::Weak<InteractiveMode>>>> = Rc::new(RefCell::new(None));
        let right_click_slot = Rc::clone(&mode_slot);
        let renderer = create_interactive_tui(InteractiveTuiOptions {
            show_hardware_cursor,
            log_directory: agent_dir.clone(),
            terminal,
            on_right_click_paste: Some(Rc::new(move || {
                if let Some(mode) = right_click_slot.borrow().as_ref().and_then(std::rc::Weak::upgrade) {
                    tokio::task::spawn_local(async move {
                        mode.handle_right_click_paste().await;
                    });
                }
            })),
        });
        let ui = InteractiveTuiHandle::new(renderer);
        ui.set_clear_on_shrink(clear_on_shrink);

        let loader_themes = session.resource_loader().get_themes();
        let registered: Vec<Arc<Theme>> = loader_themes.themes.into_iter().map(Arc::new).collect();
        if !registered.is_empty() {
            let _ = crate::modes::interactive::theme::set_registered_themes(registered);
        }
        let error_slot = Rc::clone(&mode_slot);
        let theme_controller = crate::modes::interactive::theme::InteractiveThemeController::new(
            Rc::new(InteractiveThemeUi(Rc::clone(&ui))),
            Arc::clone(&settings_handle),
            Rc::new(move |message: &JsString| {
                if let Some(mode) = error_slot.borrow().as_ref().and_then(std::rc::Weak::upgrade) {
                    mode.show_error(message);
                }
            }),
            Rc::new({
                let ui = Rc::clone(&ui);
                move || ui.request_render()
            }),
        )
        .map_err(|error| JsString::from(error.to_string()))?;

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

        let keybindings = KeybindingsManager::create(Some(&agent_dir));
        keybindings.clone().install();

        let default_editor = Rc::new(RefCell::new(
            CustomEditor::new(
                Rc::new(InteractiveTuiHost(Rc::clone(&ui))),
                crate::modes::interactive::theme::get_editor_theme(),
                keybindings.clone(),
                CustomEditorOptions {
                    editor: EditorOptions {
                        padding_x: Some(editor_padding_x),
                        autocomplete_max_visible: Some(autocomplete_max_visible),
                    },
                    legacy_prompt_history_path: Some(node_join(&agent_dir, "prompt-history.json")),
                    prompt_history_control_db_path: None,
                    prompt_history_filter: Some(Rc::new(|text: &JsString| !is_skill_invocation(text, &[]))),
                },
            )
            .map_err(|error| JsString::from(error.to_string()))?,
        ));
        let editor: Rc<RefCell<dyn EditorComponent>> = default_editor.clone();
        let editor_handle: ComponentRef = component(AnyEditorHandle(Rc::clone(&editor)));

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
        let multi_agent_store = options.multi_agent_store.clone();
        let (child_view_watch_sender, child_view_watch_events) = std::sync::mpsc::channel();

        let mode = Rc::new(Self {
            runtime,
            session: RefCell::new(session),
            options,
            multi_agent_store,
            child_view: RefCell::new(None),
            child_view_watch_events: RefCell::new(Some(child_view_watch_events)),
            child_view_watch_sender,
            multi_agent_updates: RefCell::new(None),
            multi_agent_subscription: RefCell::new(None),
            agent_slot_input_listener: Cell::new(None),
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
            transcript_tail_region: RefCell::new(None),
            status_render_region: RefCell::new(None),
            editor: RefCell::new(editor),
            default_editor,
            editor_handle: RefCell::new(editor_handle),
            footer,
            footer_data_provider,
            keybindings,
            version: (crate::config::package_config().version.clone()).into(),
            is_initialized: Cell::new(false),
            actions: RefCell::new(std::collections::VecDeque::new()),
            events: RefCell::new(None),
            ui_requests: RefCell::new(Some(ui_receiver)),
            ui_sender,
            ui_shared: shared,
            ui_id: Cell::new(0),
            unsubscribe: Cell::new(None),
            shutdown_requested: Cell::new(false),
            is_shutting_down: Arc::new(AtomicBool::new(false)),
            signal_handlers: RefCell::new(Vec::new()),
            tool_output_expanded: Cell::new(false),
            hide_thinking_block: Cell::new(hide_thinking_block),
            hidden_thinking_label: RefCell::new("Thinking...".into()),
            is_bash_mode: Cell::new(false),
            streaming_component: RefCell::new(None),
            streaming_message: RefCell::new(None),
            pending_tools: RefCell::new(IndexMap::new()),
            executing_tool_names: RefCell::new(IndexMap::new()),
            executing_tool_started_at: RefCell::new(IndexMap::new()),
            completed_tool_timings: RefCell::new(IndexMap::new()),
            extension_statuses: RefCell::new(IndexMap::new()),
            extension_widgets_above: RefCell::new(IndexMap::new()),
            extension_widgets_below: RefCell::new(IndexMap::new()),
            on_input_callback: RefCell::new(None),
            last_status_spacer: RefCell::new(None),
            last_status_text: RefCell::new(None),
            last_sigint_time: Cell::new(0.0),
            last_escape_time: Cell::new(0.0),
            working_message: RefCell::new(None),
            current_working_default_message: RefCell::new("Thinking...".into()),
            loading_animation: RefCell::new(None),
            working_loader_view: RefCell::new(None),
            retry_loader: RefCell::new(None),
            retry_countdown: RefCell::new(None),
            retry_escape_handler: RefCell::new(None),
            auto_compaction_escape_handler: RefCell::new(None),
            auto_compaction_loader: RefCell::new(None),
            auto_compaction_source_hint: RefCell::new(None),
            thinking_started_at: Cell::new(None),
            thinking_follows_tool: Cell::new(false),
            thinking_timer_next: Cell::new(None),
            tool_waiting_timer_next: Cell::new(None),
            child_activity_timer_next: Cell::new(None),
            now_millis: RefCell::new(Rc::new(js_now_millis)),
            working_visible: Cell::new(true),
            working_indicator_options: RefCell::new(None),
            prompt_activity_sources: RefCell::new(std::collections::BTreeSet::new()),
            compaction_queued_messages: RefCell::new(Vec::new()),
            response_complete_notification: RefCell::new(None),
            custom_footer: RefCell::new(None),
            default_extension_footer: RefCell::new(None),
            custom_header: RefCell::new(None),
            changelog_markdown: RefCell::new(None),
            startup_notices_shown: Cell::new(false),
            http_fetch: RefCell::new(None),
            anthropic_subscription_warning_shown: Cell::new(false),
            terminal_current_directory: RefCell::new(None),
            theme_controller: RefCell::new(theme_controller),
            debug_handler: RefCell::new(None),
            terminal_input_listeners: RefCell::new(Vec::new()),
            autocomplete_provider: RefCell::new(None),
            autocomplete_provider_wrappers: RefCell::new(Vec::new()),
            editor_component_factory: RefCell::new(None),
            fd_path: RefCell::new(None),
            clipboard_temp_files: RefCell::new(crate::utils::clipboard_temp_files::ClipboardTempFileTracker::new()),
            pending_compaction_summary_edit_id: RefCell::new(None),
            active_session_selector: RefCell::new(None),
            active_selector_dispose: RefCell::new(None),
            active_bash_components: RefCell::new(Vec::new()),
            provisional_user_messages: RefCell::new(Vec::new()),
            auth_ui: RefCell::new(None),
            session_selector_loaded: Cell::new(false),
        });

        *mode_slot.borrow_mut() = Some(Rc::downgrade(&mode));

        let id = register_mode(&mode);
        mode.ui_id.set(id);

        if let Some(slot) = mode.options.agent_view_selector_slot.as_ref() {
            let sender = mode.ui_sender.clone();
            *slot.lock().expect("agent view selector") = Some(Arc::new(move |agent_id: &JsString| {
                let sender = sender.clone();
                let agent_id = agent_id.to_owned();
                Box::pin(async move {
                    let (respond, receiver) = tokio::sync::oneshot::channel();
                    if sender.send(UiRequest::SelectAgentView { agent_id, respond }).is_err() {
                        return Some(false);
                    }
                    receiver.await.ok()
                })
            }));
        }

        mode.editor_container
            .borrow_mut()
            .add_child(Rc::clone(&mode.editor_handle.borrow()));
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
            shared: Arc::clone(&self.ui_shared),
        })
    }

    fn session(&self) -> AgentSession {
        self.session.borrow().clone()
    }

    /// The editor currently mounted in the editor container.
    fn editor(&self) -> Rc<RefCell<dyn EditorComponent>> {
        Rc::clone(&self.editor.borrow())
    }

    /// `setPromptActivity`.
    fn set_prompt_activity(&self, source: PromptActivitySource, active: bool) {
        if active {
            self.prompt_activity_sources.borrow_mut().insert(source);
        } else {
            self.prompt_activity_sources.borrow_mut().remove(&source);
        }
        self.sync_working_editor_state();
    }

    /// `syncWorkingEditorState`.
    fn sync_working_editor_state(&self) {
        let indicator_options = self.working_indicator_options.borrow().clone();
        let indicator_options = indicator_options.map(|options| pi_tui::LoaderIndicatorOptions {
            frames: options.frames,
            interval_ms: options.interval_ms.map(|interval| interval.max(0.0) as u64),
        });
        let editor = self.editor();
        let is_mounted = self
            .editor_container
            .borrow()
            .children()
            .iter()
            .any(|child| Rc::ptr_eq(child, &self.editor_handle.borrow()));
        let mut editor_ref = editor.borrow_mut();
        let can_animate = crate::modes::interactive::working_editor::supports_working_prompt_animation(
            &*editor_ref,
            indicator_options.as_ref(),
        );
        let selected_session_is_working = if self.is_viewing_agent_session() {
            self.is_selected_child_working()
        } else {
            self.session().is_streaming()
        };
        let has_prompt_activity = selected_session_is_working || !self.prompt_activity_sources.borrow().is_empty();
        let should_show = self.working_visible.get() && has_prompt_activity;
        crate::modes::interactive::working_editor::sync_working_editor(
            &mut *editor_ref,
            should_show && can_animate,
            indicator_options,
        );
        if !is_mounted {
            crate::modes::interactive::working_editor::clear_working_editor_position(&mut *editor_ref);
        }
    }

    /// `isViewingAgentSession`.
    fn is_viewing_agent_session(&self) -> bool {
        let Some(store) = self.multi_agent_store.as_ref() else {
            return false;
        };
        store
            .get_selected_agent_id()
            .is_some_and(|agent_id| store.get_agent(&agent_id).is_some())
    }

    /// `isSelectedChildWorking`.
    fn is_selected_child_working(&self) -> bool {
        let Some(store) = self.multi_agent_store.as_ref() else {
            return false;
        };
        let Some(agent_id) = store.get_selected_agent_id() else {
            return false;
        };
        store.get_agent(&agent_id).is_some_and(|agent| {
            matches!(
                agent.lifecycle,
                AgentLifecycle::Running | AgentLifecycle::SteeringPending | AgentLifecycle::Cancelling
            )
        })
    }

    fn now_millis(&self) -> f64 {
        (self.now_millis.borrow())()
    }

    /// `getWorkingLoaderMessage`.
    fn get_working_loader_message(&self) -> JsString {
        let Some(store) = self.multi_agent_store.as_ref() else {
            return self
                .working_message
                .borrow()
                .clone()
                .unwrap_or_else(|| self.current_working_default_message.borrow().clone());
        };
        let Some(selected_agent_id) = store.get_selected_agent_id() else {
            return self
                .working_message
                .borrow()
                .clone()
                .unwrap_or_else(|| self.current_working_default_message.borrow().clone());
        };
        let Some(activity) = store
            .get_agent(&selected_agent_id)
            .and_then(|agent| agent.current_activity)
        else {
            return "Working...".into();
        };
        match activity {
            crate::core::multi_agent_store::AgentCurrentActivity::Thinking { started_at } => {
                let started_at = pi_js_compat::date::date_parse_utf16(started_at.units());
                if started_at.is_nan() {
                    return "Working...".into();
                }
                js_format!(
                    "Thinking... {}",
                    format_elapsed_duration(self.now_millis() - started_at)
                )
                .into()
            }
            crate::core::multi_agent_store::AgentCurrentActivity::Tool {
                started_at,
                tool_call_id,
                tool_name,
                ..
            } => {
                let started_at = pi_js_compat::date::date_parse_utf16(started_at.units());
                if started_at.is_nan() {
                    return "Working...".into();
                }
                self.get_tool_waiting_message(
                    &tool_name,
                    Some(started_at),
                    !self.tool_component_owns_elapsed(&tool_call_id),
                )
            }
        }
    }

    /// `getThinkingWorkingMessage`.
    fn get_thinking_working_message(&self) -> JsString {
        let Some(started_at) = self.thinking_started_at.get() else {
            return JsString::from("Thinking...");
        };
        let elapsed_ms = self.now_millis() - started_at;
        let duration = if elapsed_ms < MIN_VISIBLE_ELAPSED_MS {
            JsString::from("0s")
        } else {
            (format_elapsed_duration(elapsed_ms)).into()
        };
        js_format!("Thinking... {}", duration)
    }

    /// `getToolWaitingMessage`.
    fn get_tool_waiting_message(
        &self,
        tool_name: impl Into<JsString>,
        started_at: Option<f64>,
        show_elapsed: bool,
    ) -> JsString {
        let tool_name = tool_name.into();
        let base_message = if tool_name == "bash" {
            "Waiting for command...".into()
        } else {
            pi_js_compat::js_format!("Waiting for tool: {}...", tool_name)
        };
        if !show_elapsed {
            return base_message;
        }
        let Some(started_at) = started_at else {
            return base_message;
        };
        let elapsed_ms = self.now_millis() - started_at;
        if elapsed_ms < MIN_VISIBLE_ELAPSED_MS {
            return base_message;
        }
        pi_js_compat::js_format!("{} Elapsed: {}", base_message, format_elapsed_duration(elapsed_ms))
    }

    /// `setDefaultWorkingMessage`.
    fn set_default_working_message(&self, message: impl Into<JsString>) {
        let message = message.into();
        *self.current_working_default_message.borrow_mut() = message.clone();
        let has_loader = self.loading_animation.borrow().is_some();
        let has_override = self.working_message.borrow().is_some();
        let is_child_view = *self.working_loader_view.borrow() == Some(WorkingLoaderView::Child);
        if has_loader
            && !has_override
            && !is_child_view
            && let Some(loader) = self.loading_animation.borrow().as_ref()
        {
            set_loader_message(loader, message.clone());
        }
    }

    /// `toolComponentOwnsElapsed`.
    fn tool_component_owns_elapsed(&self, tool_call_id: impl Into<JsString>) -> bool {
        let tool_call_id = tool_call_id.into();
        let component = self
            .pending_tools
            .borrow()
            .iter()
            .find(|(id, _)| &tool_call_id == *id)
            .map(|(_, component)| component.clone());
        component.is_some_and(|component| {
            component
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                .is_some_and(|component| component.has_elapsed_timing())
        })
    }

    /// `setWorkingMessageForActiveTools`.
    fn set_working_message_for_active_tools(&self) {
        let first_tool = self
            .executing_tool_names
            .borrow()
            .iter()
            .next()
            .map(|(tool_call_id, tool_name)| (tool_call_id.clone(), tool_name.clone()));
        let Some((tool_call_id, tool_name)) = first_tool else {
            self.set_default_working_message(&self.get_thinking_working_message());
            return;
        };
        let started_at = self.executing_tool_started_at.borrow().get(&tool_call_id).copied();
        self.set_default_working_message(&self.get_tool_waiting_message(
            &tool_name,
            started_at,
            !self.tool_component_owns_elapsed(&tool_call_id),
        ));
    }

    /// `updateThinkingWorkingMessage`.
    fn update_thinking_working_message(&self) {
        if !self.executing_tool_names.borrow().is_empty() {
            return;
        }
        self.set_default_working_message(&self.get_thinking_working_message());
    }

    /// `startThinkingTimer`.
    fn start_thinking_timer(&self) {
        self.stop_thinking_timer();
        self.thinking_started_at.set(Some(self.now_millis()));
        self.update_thinking_working_message();
        self.thinking_timer_next
            .set(Some(self.now_millis() + MIN_VISIBLE_ELAPSED_MS));
    }

    /// `stopThinkingTimer`.
    fn stop_thinking_timer(&self) {
        self.thinking_timer_next.set(None);
        self.thinking_started_at.set(None);
    }

    /// `startToolWaitingTimer`.
    fn start_tool_waiting_timer(&self) {
        if self.tool_waiting_timer_next.get().is_some() {
            return;
        }
        self.tool_waiting_timer_next
            .set(Some(self.now_millis() + MIN_VISIBLE_ELAPSED_MS));
    }

    /// `stopToolWaitingTimerIfIdle`.
    fn stop_tool_waiting_timer_if_idle(&self) {
        if !self.executing_tool_names.borrow().is_empty() || self.tool_waiting_timer_next.get().is_none() {
            return;
        }
        self.tool_waiting_timer_next.set(None);
    }

    /// `startChildActivityTimer`.
    fn start_child_activity_timer(&self) {
        if self.child_activity_timer_next.get().is_some() {
            return;
        }
        self.child_activity_timer_next
            .set(Some(self.now_millis() + MIN_VISIBLE_ELAPSED_MS));
    }

    /// `stopChildActivityTimer`.
    fn stop_child_activity_timer(&self) {
        self.child_activity_timer_next.set(None);
    }

    /// Drives the ported `setInterval` callbacks and the retry countdown.
    fn poll_timers(&self) {
        let now = self.now_millis();
        if let Some(next) = self.thinking_timer_next.get()
            && now >= next
        {
            self.thinking_timer_next.set(Some(now + MIN_VISIBLE_ELAPSED_MS));
            self.update_thinking_working_message();
        }
        if let Some(next) = self.tool_waiting_timer_next.get()
            && now >= next
        {
            self.tool_waiting_timer_next.set(Some(now + MIN_VISIBLE_ELAPSED_MS));
            self.set_working_message_for_active_tools();
        }
        if let Some(next) = self.child_activity_timer_next.get()
            && now >= next
        {
            self.child_activity_timer_next.set(Some(now + MIN_VISIBLE_ELAPSED_MS));
            if *self.working_loader_view.borrow() == Some(WorkingLoaderView::Child)
                && let Some(loader) = self.loading_animation.borrow().as_ref()
            {
                set_loader_message(loader, self.get_working_loader_message());
            }
        }
        let mut countdown = self.retry_countdown.borrow_mut().take();
        if let Some(timer) = countdown.as_mut()
            && !timer.poll(Instant::now())
        {
            *self.retry_countdown.borrow_mut() = countdown;
        }
        for loader in [
            self.loading_animation.borrow().clone(),
            self.retry_loader.borrow().clone(),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(loader) = loader
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<Loader>())
            {
                loader.poll(Instant::now());
            }
        }
    }

    /// `getWorkingLoaderIndicator`.
    fn get_working_loader_indicator(&self, _view: WorkingLoaderView) -> pi_tui::LoaderIndicatorOptions {
        pi_tui::LoaderIndicatorOptions {
            frames: Some(Vec::new()),
            interval_ms: None,
        }
    }

    /// `updateWorkingLoaderIndicator`.
    fn update_working_loader_indicator(&self) {
        if self.loading_animation.borrow().is_none() || self.working_loader_view.borrow().is_none() {
            return;
        }
        let indicator =
            self.get_working_loader_indicator(self.working_loader_view.borrow().unwrap_or(WorkingLoaderView::Main));
        if let Some(loader) = self.loading_animation.borrow().as_ref()
            && let Some(loader) = loader
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<Loader>())
        {
            loader.set_indicator(Some(indicator));
        }
    }

    /// `createWorkingLoader`.
    fn create_working_loader(&self, indicator: pi_tui::LoaderIndicatorOptions) -> ComponentRef {
        let ui = Rc::clone(&self.ui);
        let request_render: Rc<dyn Fn()> = Rc::new(move || {
            ui.request_render();
        });
        component(Loader::new(
            request_render,
            Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Accent, text))),
            Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
            self.get_working_loader_message(),
            Some(indicator),
        ))
    }

    /// `stopWorkingLoader`.
    fn stop_working_loader(&self) {
        self.stop_child_activity_timer();
        let Some(loader) = self.loading_animation.borrow_mut().take() else {
            return;
        };
        if let Some(loader) = loader
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Loader>())
        {
            loader.stop();
        }
        *self.working_loader_view.borrow_mut() = None;
        self.status_container.borrow_mut().remove_child(&loader);
    }

    /// `stopRetryLoader`.
    fn stop_retry_loader(&self) {
        let Some(loader) = self.retry_loader.borrow_mut().take() else {
            return;
        };
        if let Some(loader) = loader
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Loader>())
        {
            loader.stop();
        }
        self.status_container.borrow_mut().remove_child(&loader);
    }

    /// Stops the compaction loader and clears its status row.
    fn stop_auto_compaction_loader(&self) {
        let Some(loader) = self.auto_compaction_loader.borrow_mut().take() else {
            return;
        };
        if let Some(loader) = loader
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Loader>())
        {
            loader.stop();
        }
        self.status_container.borrow_mut().remove_child(&loader);
    }

    /// `syncWorkingLoaderVisibility`.
    fn sync_working_loader_visibility(&self) {
        let viewing_child = self.is_viewing_agent_session();
        let should_show = self.working_visible.get()
            && if viewing_child {
                self.is_selected_child_working()
            } else {
                self.session().is_streaming()
            };
        self.sync_working_editor_state();
        if !should_show {
            self.stop_working_loader();
            return;
        }

        let desired_view = if viewing_child {
            WorkingLoaderView::Child
        } else {
            WorkingLoaderView::Main
        };
        if self.loading_animation.borrow().is_some() && *self.working_loader_view.borrow() == Some(desired_view) {
            if let Some(loader) = self.loading_animation.borrow().as_ref() {
                set_loader_message(loader, self.get_working_loader_message());
            }
            return;
        }
        self.stop_working_loader();
        let loader = self.create_working_loader(self.get_working_loader_indicator(desired_view));
        *self.loading_animation.borrow_mut() = Some(Rc::clone(&loader));
        *self.working_loader_view.borrow_mut() = Some(desired_view);
        self.status_container.borrow_mut().add_child(loader);
        if viewing_child {
            self.start_child_activity_timer();
        }
    }

    /// `setWorkingVisible`.
    fn set_working_visible(&self, visible: bool) {
        self.working_visible.set(visible);
        self.sync_working_loader_visibility();
        self.ui.request_render();
    }

    /// `setWorkingIndicator`.
    fn set_working_indicator(&self, options: Option<WorkingIndicatorOptions>) {
        *self.working_indicator_options.borrow_mut() = options;
        self.sync_working_editor_state();
        self.update_working_loader_indicator();
        self.ui.request_render();
    }

    /// `addCompletedThinkingDurationBeforeToolCall`.
    fn add_completed_thinking_duration_before_tool_call(&self, message: &AssistantMessage) {
        let has_tool_call = message
            .content
            .iter()
            .any(|content| matches!(content, pi_ai::types::AssistantContent::ToolCall(_)));
        if !has_tool_call || !self.thinking_follows_tool.get() {
            return;
        }
        let Some(started_at) = self.thinking_started_at.get() else {
            return;
        };
        let elapsed_ms = self.now_millis() - started_at;
        self.thinking_follows_tool.set(false);
        if elapsed_ms < MIN_VISIBLE_ELAPSED_MS {
            return;
        }
        let duration = format_elapsed_duration(elapsed_ms);
        self.chat_container.borrow_mut().add_child(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Muted, &js_format!("Thought for {}", duration))),
            1,
            0,
        )));
    }

    fn settings<R>(&self, f: impl FnOnce(&crate::core::settings_manager::SettingsManager) -> R) -> R {
        f(&self.session.borrow().settings_manager())
    }

    fn cwd(&self) -> JsString {
        self.session.borrow().cwd().to_owned()
    }

    /// `selectAgentViewFromBridge`.
    pub fn select_agent_view_from_bridge(&self, agent_id: &JsString) -> bool {
        self.select_agent_view(agent_id)
    }

    /// `selectAgentView`.
    fn select_agent_view(&self, agent_id: &JsString) -> bool {
        let Some(store) = self.multi_agent_store.as_ref() else {
            return false;
        };
        if agent_id == "main" {
            store.clear_selected_agent_view();
            self.clear_child_agent_view();
            self.loaded_resources_container.borrow_mut().clear();
            self.show_loaded_resources(false, true);
            self.chat_container.borrow_mut().clear();
            self.render_initial_messages();
            self.sync_working_loader_visibility();
            self.update_selected_agent_selection_widgets();
            return true;
        }
        let Some(selected) = store.get_agent(agent_id) else {
            return false;
        };
        let previous = store.get_selected_agent_id();
        if !matches!(
            store.select_agent_view_with_status(agent_id),
            crate::core::multi_agent_store::AgentViewSelectionResult::Ok(_)
        ) {
            return false;
        }
        if !self.open_child_agent_view(&selected) {
            self.restore_previous_agent_selection(previous.as_ref());
            return false;
        }
        self.loaded_resources_container.borrow_mut().clear();
        self.sync_working_loader_visibility();
        self.update_selected_agent_selection_widgets();
        true
    }

    /// `selectAgentSlot`.
    fn select_agent_slot(&self, slot_index: i64) -> bool {
        if slot_index == 0 {
            return self.select_agent_view(&"main".into());
        }
        let Some(store) = self.multi_agent_store.as_ref() else {
            return false;
        };
        let previous_selected_agent_id = store.get_selected_agent_id();
        match store.select_active_agent_slot_target_with_status(slot_index) {
            crate::core::multi_agent_store::ActiveAgentTargetSelectionResult::NotFound { .. } => false,
            crate::core::multi_agent_store::ActiveAgentTargetSelectionResult::Inactive { agent } => {
                self.show_inactive_agent_selection_status(&agent);
                true
            }
            crate::core::multi_agent_store::ActiveAgentTargetSelectionResult::Ok(agent) => {
                if !self.open_child_agent_view(&agent) {
                    self.restore_previous_agent_selection(previous_selected_agent_id.as_ref());
                    return false;
                }
                self.loaded_resources_container.borrow_mut().clear();
                self.sync_working_loader_visibility();
                self.update_selected_agent_selection_widgets();
                true
            }
        }
    }

    /// `showInactiveAgentSelectionStatus`.
    fn show_inactive_agent_selection_status(&self, agent: &AgentSnapshot) {
        self.show_status(&crate::core::multi_agent_store::format_inactive_agent_selection_message(agent));
    }

    /// `registerAgentSlotKeyHandlers`.
    fn register_agent_slot_key_handlers(self: &Rc<Self>) {
        for (keybinding, slot_index) in AGENT_SLOT_KEYBINDINGS {
            let mode = Rc::clone(self);
            self.default_editor.borrow_mut().on_action(
                keybinding,
                Rc::new(move || {
                    mode.select_agent_slot(slot_index);
                    true
                }),
            );
        }
    }

    /// `registerGlobalAgentSlotInputHandler`.
    fn register_global_agent_slot_input_handler(&self) {
        if let Some(id) = self.agent_slot_input_listener.take() {
            self.ui.remove_input_listener(id);
        }
        let id = self.ui_id.get();
        let listener_id = self.ui.add_input_listener(Rc::new(move |data: &JsString| {
            let consumed = with_mode(id, |mode| {
                AGENT_SLOT_KEYBINDINGS.into_iter().any(|(keybinding, slot_index)| {
                    mode.keybindings.matches(data, keybinding) && mode.select_agent_slot(slot_index)
                })
            })
            .unwrap_or(false);
            if consumed {
                TuiInputListenerResult {
                    consume: true,
                    data: None,
                }
            } else {
                TuiInputListenerResult::default()
            }
        }));
        self.agent_slot_input_listener.set(Some(listener_id));
    }

    fn restore_previous_agent_selection(&self, agent_id: Option<&JsString>) {
        let Some(store) = self.multi_agent_store.as_ref() else {
            return;
        };
        match agent_id {
            Some(agent_id) => {
                if !matches!(
                    store.select_agent_view_with_status(agent_id),
                    crate::core::multi_agent_store::AgentViewSelectionResult::Ok(_)
                ) {
                    store.clear_selected_agent_view();
                }
            }
            None => store.clear_selected_agent_view(),
        }
    }

    /// `updateSelectedAgentSelectionWidgets`.
    fn update_selected_agent_selection_widgets(&self) {
        self.render_widgets();
        self.footer.borrow_mut().invalidate();
        self.ui.request_render();
    }

    /// `openChildAgentView`.
    fn open_child_agent_view(&self, agent: &AgentSnapshot) -> bool {
        let transcript_path = agent.transcript.as_ref().and_then(|transcript| transcript.path.clone());
        self.clear_child_agent_view();
        *self.child_view.borrow_mut() = Some(ChildAgentView {
            transcript_path: transcript_path.clone(),
            agent_id: agent.id.clone(),
            session_manager: None,
            watcher: None,
            watch_path: None,
            reload_deadline: None,
            watch_retry_deadline: None,
            reload_retries: 0,
        });
        let Some(path) = transcript_path.as_ref() else {
            self.render_live_agent_placeholder(agent, None);
            return true;
        };
        if !Path::new(&encode_node_utf8(path)).exists() {
            self.render_live_agent_placeholder(agent, Some(path));
            return true;
        }
        let session_dir = self.session.borrow().session_manager().get_session_dir().to_owned();
        match SessionManager::open(path, Some(&session_dir), None) {
            Ok(manager) => {
                if let Some(view) = self.child_view.borrow_mut().as_mut() {
                    view.session_manager = Some(Arc::new(std::sync::Mutex::new(manager)));
                }
            }
            Err(_) => {
                self.render_live_agent_placeholder(agent, Some(path));
                self.schedule_selected_agent_transcript_reload(false);
                return true;
            }
        }
        self.watch_selected_agent_transcript(path);
        self.render_selected_agent_view()
    }

    /// `watchSelectedAgentTranscript`.
    fn watch_selected_agent_transcript(&self, transcript_path: &JsString) {
        let watch_path = if Path::new(&encode_node_utf8(transcript_path)).exists() {
            transcript_path.to_owned()
        } else {
            node_dirname(transcript_path)
        };
        let target_name = std::ffi::OsString::from(encode_node_utf8(&node_basename(transcript_path)));
        let is_file_watch = &watch_path == transcript_path;
        let sender = self.child_view_watch_sender.clone();
        let listener_sender = sender.clone();
        let watcher = watch_with_error_handler(
            Path::new(&encode_node_utf8(&watch_path)),
            move |_kind: WatchEventKind, path: Option<PathBuf>| {
                let matches_target = path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .is_none_or(|name| name == target_name);
                if is_file_watch || matches_target {
                    let _ = listener_sender.send(ChildViewWatchEvent::Changed);
                }
            },
            move || {
                let _ = sender.send(ChildViewWatchEvent::Error);
            },
        );
        if let Some(view) = self.child_view.borrow_mut().as_mut() {
            view.watcher = watcher;
            view.watch_path = Some(watch_path);
        }
    }

    /// `scheduleSelectedAgentTranscriptReload`.
    fn schedule_selected_agent_transcript_reload(&self, reset_retries: bool) {
        if let Some(view) = self.child_view.borrow_mut().as_mut() {
            if reset_retries {
                view.reload_retries = 0;
            }
            view.reload_deadline = Some(Instant::now() + Duration::from_millis(50));
        }
    }

    /// Drives `fs.watch` events, the reload debounce, and the watch retry.
    fn poll_child_view_events(&self) {
        let events: Vec<ChildViewWatchEvent> = self
            .child_view_watch_events
            .borrow()
            .as_ref()
            .map(|receiver| receiver.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                ChildViewWatchEvent::Changed => self.schedule_selected_agent_transcript_reload(true),
                ChildViewWatchEvent::Error => {
                    if let Some(view) = self.child_view.borrow_mut().as_mut() {
                        view.watcher = None;
                        view.watch_path = None;
                        view.watch_retry_deadline =
                            Some(Instant::now() + Duration::from_millis(FS_WATCH_RETRY_DELAY_MS));
                    }
                }
            }
        }

        let retry_due = self
            .child_view
            .borrow()
            .as_ref()
            .and_then(|view| view.watch_retry_deadline)
            .is_some_and(|deadline| Instant::now() >= deadline);
        if retry_due {
            let retry = self.child_view.borrow_mut().as_mut().and_then(|view| {
                view.watch_retry_deadline = None;
                view.transcript_path.clone()
            });
            if let Some(path) = retry
                && self.selected_child_view_matches()
            {
                self.watch_selected_agent_transcript(&path);
                self.schedule_selected_agent_transcript_reload(true);
            }
        }

        let reload_due = self
            .child_view
            .borrow()
            .as_ref()
            .and_then(|view| view.reload_deadline)
            .is_some_and(|deadline| Instant::now() >= deadline);
        if reload_due {
            if let Some(view) = self.child_view.borrow_mut().as_mut() {
                view.reload_deadline = None;
            }
            self.reload_selected_agent_transcript();
        }
    }

    fn selected_child_view_matches(&self) -> bool {
        let Some(store) = self.multi_agent_store.as_ref() else {
            return false;
        };
        self.child_view
            .borrow()
            .as_ref()
            .is_some_and(|view| store.get_selected_agent_id().as_ref() == Some(&view.agent_id))
    }

    /// `reloadSelectedAgentTranscript`.
    fn reload_selected_agent_transcript(&self) {
        if self.child_view.borrow().is_none() || !self.selected_child_view_matches() {
            return;
        }
        let Some(path) = self
            .child_view
            .borrow()
            .as_ref()
            .and_then(|view| view.transcript_path.clone())
        else {
            return;
        };
        if !Path::new(&encode_node_utf8(&path)).exists() {
            let rewatch = self
                .child_view
                .borrow()
                .as_ref()
                .is_some_and(|view| view.watch_path.as_ref() == Some(&path));
            if rewatch {
                self.watch_selected_agent_transcript(&path);
            }
            return;
        }
        if !is_complete_jsonl_file(&path).unwrap_or(false) {
            self.retry_selected_agent_transcript_reload();
            return;
        }
        let session_dir = self.session.borrow().session_manager().get_session_dir().to_owned();
        let Ok(manager) = SessionManager::open(&path, Some(&session_dir), None) else {
            self.retry_selected_agent_transcript_reload();
            return;
        };
        let watch_path = {
            let mut view = self.child_view.borrow_mut();
            let Some(view) = view.as_mut() else {
                return;
            };
            view.session_manager = Some(Arc::new(std::sync::Mutex::new(manager)));
            view.reload_retries = 0;
            view.watch_path.clone()
        };
        if watch_path.as_ref() != Some(&path) {
            self.watch_selected_agent_transcript(&path);
        }
        if self.render_selected_agent_view() {
            self.ui.request_render();
        }
    }

    /// `retrySelectedAgentTranscriptReload`.
    fn retry_selected_agent_transcript_reload(&self) {
        let should_retry = {
            let mut view = self.child_view.borrow_mut();
            let Some(view) = view.as_mut() else {
                return;
            };
            if view.reload_retries >= MAX_CHILD_TRANSCRIPT_RELOAD_RETRIES {
                false
            } else {
                view.reload_retries += 1;
                true
            }
        };
        if should_retry {
            self.schedule_selected_agent_transcript_reload(false);
        }
    }

    /// `renderSelectedAgentView`.
    fn render_selected_agent_view(&self) -> bool {
        let Some(session_manager) = self
            .child_view
            .borrow()
            .as_ref()
            .and_then(|view| view.session_manager.clone())
        else {
            return false;
        };
        let agent_id = self
            .child_view
            .borrow()
            .as_ref()
            .map(|view| view.agent_id.clone())
            .unwrap_or_default();
        let manager = session_manager
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let session_context = manager.build_session_context();
        let persisted_settings = manager.read_persisted_session_settings().ok().flatten();
        let selected_agent = self
            .multi_agent_store
            .as_ref()
            .and_then(|store| store.get_agent(&agent_id));
        let thinking_level = persisted_settings
            .and_then(|settings| settings.thinking_level)
            .or_else(|| {
                selected_agent
                    .as_ref()
                    .and_then(|agent| agent.model.as_ref())
                    .and_then(|model| model.thinking_level.clone().map(JsString::from))
            })
            .unwrap_or_else(|| session_context.thinking_level.clone());
        let model = session_context.model.as_ref().and_then(|model| {
            self.session
                .borrow()
                .model_runtime()
                .get_model(&model.provider, &model.model_id)
        });
        let context_window = model.as_ref().map_or(0.0, |model| model.context_window);
        let estimate = crate::core::compaction::estimate_context_tokens(&session_context.messages);
        let context_usage = (context_window > 0.0).then(|| ContextUsage {
            tokens: Some(estimate.tokens),
            context_window,
            percent: Some(estimate.tokens / context_window * 100.0),
        });
        let footer_override = FooterSessionOverride {
            cwd: manager.get_cwd().to_owned(),
            session_manager: Some(Arc::clone(&session_manager)),
            model,
            thinking_level: thinking_level.into(),
            context_usage,
        };
        let entries = manager.build_context_entries();
        let source_cwd = manager.get_cwd().to_owned();
        drop(manager);
        self.footer.borrow_mut().set_session_override(footer_override.clone());
        self.footer_data_provider.set_session_override(footer_override);
        self.chat_container.borrow_mut().clear();
        self.render_session_entries_with_cwd(&entries, Some(&source_cwd), false);
        true
    }

    /// `findReadableAgentLogPath`.
    fn find_readable_agent_log_path(&self, agent: &AgentSnapshot) -> Option<JsString> {
        let known_output = find_readable_agent_file_path(agent, &JsString::from("Pyrun output"))
            .or_else(|| find_readable_agent_file_path(agent, &JsString::from("Bash output")));
        if known_output.is_some() {
            return known_output;
        }
        agent
            .result
            .as_ref()
            .and_then(|result| result.file_refs.as_ref())
            .and_then(|file_refs| {
                file_refs
                    .iter()
                    .find(|file_ref| {
                        file_ref.label.as_ref() != Some(&JsString::from("Pyrun script"))
                            && Path::new(&encode_node_utf8(&file_ref.path)).exists()
                    })
                    .map(|file_ref| file_ref.path.clone())
            })
    }

    /// `readAgentLogPreview`.
    fn read_agent_log_preview(&self, log_path: &JsString) -> JsString {
        self.read_agent_log_preview_unchecked(log_path).unwrap_or_else(|error| {
            js_format!(
                "Unable to read log: {}",
                crate::core::tools::node_fs::node_read_error(&error, log_path)
            )
        })
    }

    /// `readAgentLogPreviewUnchecked`.
    fn read_agent_log_preview_unchecked(&self, log_path: &JsString) -> std::io::Result<JsString> {
        let log_size = std::fs::metadata(encode_node_utf8(log_path))?.len() as usize;
        if log_size <= MAX_AGENT_LOG_VIEW_BYTES {
            return std::fs::read_to_string(encode_node_utf8(log_path)).map(JsString::from);
        }
        let start_offset = log_size - MAX_AGENT_LOG_VIEW_BYTES;
        let bytes = std::fs::read(encode_node_utf8(log_path))?;
        let tail = String::from_utf8_lossy(&bytes[start_offset..]);
        Ok(js_format!(
            "[Showing last {} bytes]\n{}",
            MAX_AGENT_LOG_VIEW_BYTES,
            JsString::from(tail.as_ref())
        ))
    }

    /// `renderLiveAgentPlaceholder`.
    fn render_live_agent_placeholder(&self, agent: &AgentSnapshot, transcript_path: Option<&JsString>) {
        let footer_override = FooterSessionOverride {
            cwd: agent.cwd.clone(),
            session_manager: None,
            model: None,
            thinking_level: "off".into(),
            context_usage: None,
        };
        self.footer.borrow_mut().set_session_override(footer_override.clone());
        self.footer_data_provider.set_session_override(footer_override);
        if let Some(path) = transcript_path {
            self.watch_selected_agent_transcript(path);
        }
        let transcript_status = transcript_path.map_or_else(
            || JsString::from("Transcript file has not been assigned yet."),
            |path| js_format!("Transcript file has not been written yet: {}", path),
        );
        let script_path = find_readable_agent_file_path(agent, &JsString::from("Pyrun script"));
        let log_path = self.find_readable_agent_log_path(agent);
        let chat = Rc::clone(&self.chat_container);
        chat.borrow_mut().clear();
        chat.borrow_mut().add_child(component(Text::new(
            with_theme(|theme| {
                theme.bold(&theme.fg(
                    ThemeColor::Accent,
                    &js_format!("Viewing live agent: {}", agent.display_name),
                ))
            }),
            1,
            0,
        )));
        chat.borrow_mut().add_child(component(Spacer::new(1)));
        chat.borrow_mut().add_child(component(Text::new(
            with_theme(|theme| {
                theme.fg(
                    ThemeColor::Dim,
                    &js_format!("{} {}", agent.id, lifecycle_name(agent.lifecycle)),
                )
            }),
            1,
            0,
        )));
        chat.borrow_mut().add_child(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Dim, &transcript_status)),
            1,
            0,
        )));
        if let Some(script_path) = script_path {
            chat.borrow_mut().add_child(component(Spacer::new(1)));
            chat.borrow_mut().add_child(component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("Script: {}", script_path))),
                1,
                0,
            )));
            chat.borrow_mut()
                .add_child(component(Text::new(self.read_agent_log_preview(&script_path), 1, 0)));
        }
        if let Some(log_path) = log_path {
            chat.borrow_mut().add_child(component(Spacer::new(1)));
            chat.borrow_mut().add_child(component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("Log: {}", log_path))),
                1,
                0,
            )));
            chat.borrow_mut()
                .add_child(component(Text::new(self.read_agent_log_preview(&log_path), 1, 0)));
        }
    }

    /// `clearChildAgentView`.
    fn clear_child_agent_view(&self) {
        self.footer.borrow_mut().clear_session_override();
        self.footer_data_provider.clear_session_override();
        *self.child_view.borrow_mut() = None;
    }

    /// `subscribeToMultiAgentStore`.
    fn subscribe_to_multi_agent_store(&self) {
        if let Some(subscription) = self.multi_agent_subscription.borrow_mut().take() {
            subscription.unsubscribe();
        }
        *self.multi_agent_updates.borrow_mut() = None;
        let Some(store) = self.multi_agent_store.as_ref() else {
            return;
        };
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let subscription = store.subscribe_agent_updates(move |_previous, current| {
            let _ = sender.send(current.clone());
        });
        *self.multi_agent_updates.borrow_mut() = Some(receiver);
        *self.multi_agent_subscription.borrow_mut() = Some(subscription);
    }

    /// Handles `subscribeAgentUpdates` callbacks on the UI thread.
    fn poll_multi_agent_updates(&self) {
        let mut updates: Vec<AgentSnapshot> = Vec::new();
        if let Some(receiver) = self.multi_agent_updates.borrow_mut().as_mut() {
            while let Ok(update) = receiver.try_recv() {
                updates.push(update);
            }
        }
        for current in updates {
            let Some(store) = self.multi_agent_store.as_ref() else {
                continue;
            };
            if store.get_selected_agent_id().as_ref() != Some(&current.id) {
                continue;
            }
            if matches!(
                current.lifecycle,
                AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted
            ) {
                self.select_agent_view(&"main".into());
                continue;
            }
            let transcript_path = current
                .transcript
                .as_ref()
                .and_then(|transcript| transcript.path.clone());
            let child_path = self
                .child_view
                .borrow()
                .as_ref()
                .and_then(|view| view.transcript_path.clone());
            if transcript_path.is_some() && transcript_path != child_path {
                self.open_child_agent_view(&current);
            }
            self.sync_working_loader_visibility();
            self.update_selected_agent_selection_widgets();
            self.ui.request_render();
        }
    }

    async fn submit_selected_agent_steering(&self, message: &JsString, submitted_text: &JsString) -> bool {
        let Some(store) = self.multi_agent_store.as_ref() else {
            return false;
        };
        let Some(agent_id) = store.get_selected_agent_id() else {
            return false;
        };
        let Some(agent) = store.get_agent(&agent_id) else {
            return false;
        };
        if !is_steerable_agent(&agent) {
            self.show_error(JsString::from("Could not steer ") + &agent.display_name + ": agent is not steerable");
            self.editor().borrow_mut().set_text(submitted_text);
            return true;
        }
        let Some(steer) = self.options.steer_multi_agent.clone() else {
            self.show_error(
                JsString::from("Could not steer ") + &agent.display_name + ": lifecycle coordinator unavailable",
            );
            self.editor().borrow_mut().set_text(submitted_text);
            return true;
        };
        let session_id = self.session.borrow().session_manager().get_session_id().to_owned();
        match steer(agent_id, message.into(), session_id).await {
            Ok(()) => {
                self.add_submitted_text_to_history(message);
                self.clear_editor();
                self.ui.request_render();
            }
            Err(error) => {
                self.show_error(JsString::from("Could not steer ") + &agent.display_name + ": " + error);
                self.editor().borrow_mut().set_text(submitted_text);
            }
        }
        true
    }

    fn cancel_selected_agent_turn(&self) -> bool {
        let Some(store) = self.multi_agent_store.as_ref() else {
            return false;
        };
        let Some(agent_id) = store.get_selected_agent_id() else {
            return false;
        };
        let Some(agent) = store.get_agent(&agent_id) else {
            return false;
        };
        if !is_interruptible_lifecycle(agent.lifecycle) {
            return false;
        }
        let Some(cancel) = self.options.cancel_multi_agent.clone() else {
            self.show_status(
                JsString::from("Could not cancel ") + &agent.display_name + ": lifecycle coordinator unavailable",
            );
            self.ui.request_render();
            return true;
        };
        self.show_status(JsString::from("Requesting cancellation for ") + &agent.display_name + "...");
        let display_name = agent.display_name.clone();
        let id = self.ui_id.get();
        tokio::task::spawn_local(async move {
            let cancelled = cancel(agent_id).await;
            with_mode(id, |mode| {
                match cancelled {
                    Ok(()) => mode.show_status(JsString::from("Cancellation requested for ") + &display_name + "."),
                    Err(error) => mode.show_status(JsString::from("Could not cancel ") + &display_name + ": " + error),
                }
                mode.ui.request_render();
            });
        });
        true
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
        let tail_region = tail_region.map(Rc::new);
        let status_region = status_region.map(Rc::new);
        *self.transcript_tail_region.borrow_mut() = tail_region.clone();
        *self.status_render_region.borrow_mut() = status_region.clone();

        let on_transcript_tail_layout: Rc<dyn Fn(pi_tui::tui::FlowRenderRegionLayout)> = match tail_region {
            Some(region) => Rc::new(move |layout| region.place(layout)),
            None => Rc::new(|_| {}),
        };
        let on_status_layout: Rc<dyn Fn(pi_tui::tui::RenderRegionRect)> = match status_region {
            Some(region) => Rc::new(move |rect| region.place(rect)),
            None => Rc::new(|_| {}),
        };
        let height_ui = Rc::clone(&self.ui);
        let editor_container = Rc::clone(&self.editor_container);
        let editor_handle = Rc::clone(&self.editor_handle.borrow());
        let editor = Rc::clone(&self.editor.borrow());
        let root = create_interactive_root_compositor(InteractiveRootCompositorOptions {
            get_height: Rc::new(move || height_ui.terminal().borrow().rows()),
            header: component(SharedContainer(Rc::clone(&self.header_container))),
            loaded_resources: component(SharedContainer(Rc::clone(&self.loaded_resources_container))),
            chat: component(SharedContainer(Rc::clone(&self.chat_container))),
            transcript_tail: transcript_tail_ref,
            pending_messages: component(SharedContainer(Rc::clone(&self.pending_messages_container))),
            on_transcript_tail_layout,
            status: status_ref,
            widget_above: component(SharedContainer(Rc::clone(&self.widget_container_above))),
            editor: component(SharedContainer(Rc::clone(&self.editor_container))),
            widget_below: component(SharedContainer(Rc::clone(&self.widget_container_below))),
            footer: component(SharedContainer(Rc::clone(&self.footer_container))),
            on_status_layout,
            on_editor_layout: Rc::new(move |rect| {
                let mounted = editor_container
                    .borrow()
                    .children()
                    .iter()
                    .any(|child| Rc::ptr_eq(child, &editor_handle));
                if !mounted {
                    crate::modes::interactive::working_editor::clear_working_editor_position(&mut *editor.borrow_mut());
                    return;
                }
                crate::modes::interactive::working_editor::position_working_editor(
                    &mut *editor.borrow_mut(),
                    rect.row,
                    rect.col,
                );
            }),
        });
        *self.root_compositor.borrow_mut() = Some(component(root));
    }

    fn mount_interactive_tui(&self) {
        self.rebuild_root_compositor();
        if let Some(root) = self.root_compositor.borrow().clone() {
            self.ui.add_child(root);
        }
    }

    fn update_terminal_current_directory(&self) {
        let cwd = self.cwd();
        if self.terminal_current_directory.borrow().as_ref() == Some(&cwd) {
            return;
        }
        *self.terminal_current_directory.borrow_mut() = Some(cwd.clone());
        let sequence = crate::utils::terminal_current_directory::format_terminal_current_directory_sequence(&cwd)
            .unwrap_or_else(|error| throw_uncaught(ERROR, error.message().clone()));
        let terminal = self.ui.terminal();
        terminal.borrow_mut().write(&JsString::from(&sequence));
    }

    fn update_terminal_title(&self) {
        let cwd = self.cwd();
        let basename = node_basename(&cwd);
        let title = match self.session.borrow().session_name() {
            Some(name) => js_format!(
                "{} - {} - {}",
                crate::config::package_config().app_title,
                name,
                basename
            ),
            None => js_format!("{} - {}", crate::config::package_config().app_title, basename),
        };
        self.ui.terminal().borrow_mut().set_title(&title);
    }

    /// `getStartupExpansionState`.
    fn startup_expansion_state(&self) -> bool {
        self.options.verbose || self.tool_output_expanded.get()
    }

    fn show_status(&self, message: impl Into<JsString>) {
        let message = message.into();
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
                text.set_text(with_theme(|theme| theme.fg(ThemeColor::Dim, &message)));
            }
            self.ui.request_render();
            return;
        }

        let spacer = component(Spacer::new(1));
        let text = component(Text::new(with_theme(|theme| theme.fg(ThemeColor::Dim, &message)), 1, 0));
        {
            let mut chat = self.chat_container.borrow_mut();
            chat.add_child(Rc::clone(&spacer));
            chat.add_child(Rc::clone(&text));
        }
        *self.last_status_spacer.borrow_mut() = Some(spacer);
        *self.last_status_text.borrow_mut() = Some(text);
        self.ui.request_render();
    }

    fn show_error(&self, message: impl Into<JsString>) {
        let message = message.into();
        let text = component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Error, &message)),
            1,
            0,
        ));
        self.chat_container.borrow_mut().add_child(text);
        self.ui.request_render();
    }

    fn show_warning(&self, message: impl Into<JsString>) {
        let message = message.into();
        let text = component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Warning, &message)),
            1,
            0,
        ));
        self.chat_container.borrow_mut().add_child(text);
        self.ui.request_render();
    }

    fn get_user_message_text(message: &AgentMessage) -> JsString {
        let AgentMessage::User(user) = message else {
            return JsString::default();
        };
        match &user.content {
            pi_ai::types::UserContent::Text(text) => text.clone(),
            pi_ai::types::UserContent::Blocks(blocks) => JsString::join(
                blocks.iter().filter_map(|block| match block {
                    pi_ai::types::UserContentBlock::Text(text) => Some(text.text.clone()),
                    _ => None,
                }),
                "",
            ),
        }
    }

    fn add_message_to_chat(&self, message: &AgentMessage, populate_history: bool) {
        match message {
            AgentMessage::User(user) => {
                let text = Self::get_user_message_text(message);
                if text.is_empty() {
                    return;
                }
                if !self.chat_container.borrow().children().is_empty() {
                    self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
                }
                let skill_block = crate::parse_skill_block(&text);
                let is_skill_block = skill_block.is_some();
                if let Some(skill_block) = skill_block {
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
                            text.clone(),
                            Some(self.markdown_theme()),
                            self.output_pad(),
                            self.markdown_transformers(),
                        )));
                }
                if populate_history
                    && user.input_source != Some(pi_ai::types::InputSource::Extension)
                    && !is_skill_block
                {
                    self.add_rendered_message_to_editor_history(&JsString::from(&(&text).clone()));
                }
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

    /// `renderPreparedUserMessage`: render the user bubble at submit time and
    /// remember its components so `message_start` can reuse them.
    fn render_prepared_user_message(&self, message: &AgentMessage) {
        if message.role() != "user" || self.is_viewing_agent_session() {
            return;
        }
        let previous_child_count = self.chat_container.borrow().children().len();
        self.add_message_to_chat(message, false);
        let components: Vec<ComponentRef> = self
            .chat_container
            .borrow()
            .children()
            .iter()
            .skip(previous_child_count)
            .cloned()
            .collect();
        if components.is_empty() {
            return;
        }
        self.provisional_user_messages
            .borrow_mut()
            .push((message.clone(), components));
        self.ui.request_render();
    }

    /// `consumePreparedUserMessage`: reuse the provisional components when they
    /// are still mounted in the chat.
    fn consume_prepared_user_message(&self, message: &AgentMessage) -> bool {
        let index = self
            .provisional_user_messages
            .borrow()
            .iter()
            .position(|(prepared, _)| prepared == message);
        let Some(index) = index else {
            return false;
        };
        let (_, components) = self.provisional_user_messages.borrow_mut().remove(index);
        let children = self.chat_container.borrow().children().to_vec();
        let remain = !components.is_empty()
            && components
                .iter()
                .all(|component| children.iter().any(|child| Rc::ptr_eq(child, component)));
        if remain {
            return true;
        }
        self.remove_components_from_chat(&components);
        false
    }

    /// `removeProvisionalUserMessages`.
    fn remove_provisional_user_messages(&self) -> bool {
        let entries = std::mem::take(&mut *self.provisional_user_messages.borrow_mut());
        let mut removed = false;
        for (_, components) in entries {
            removed = self.remove_components_from_chat(&components) || removed;
        }
        removed
    }

    /// `removeComponentsFromChat`.
    fn remove_components_from_chat(&self, components: &[ComponentRef]) -> bool {
        let mut chat = self.chat_container.borrow_mut();
        let mut removed = false;
        for component in components {
            while chat.children().iter().any(|child| Rc::ptr_eq(child, component)) {
                chat.remove_child(component);
                removed = true;
            }
        }
        removed
    }

    fn add_custom_message_to_chat(&self, object: &JsObject) {
        let role = object
            .get("role")
            .and_then(JsValue::as_string)
            .cloned()
            .unwrap_or_default();
        match role {
            role if role == "bashExecution" => {
                let command = json_string(object, &JsString::from("command"));
                let output = json_string(object, &JsString::from("output"));
                let exit_code = object.get("exitCode").and_then(JsValue::as_f64);
                let cancelled = object.get("cancelled").and_then(JsValue::as_bool).unwrap_or(false);
                let truncated = object.get("truncated").and_then(JsValue::as_bool).unwrap_or(false);
                let full_output_path = object.get("fullOutputPath").and_then(JsValue::as_string).cloned();
                let exclude_from_context = object
                    .get("excludeFromContext")
                    .and_then(JsValue::as_bool)
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
                        content: JsString::default(),
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
            role if role == "compactionSummary" => {
                let message = js_json::from_value::<crate::core::messages::CompactionSummaryMessage>(&JsValue::Object(
                    object.clone(),
                ))
                .unwrap_or_else(|error| throw_uncaught(ERROR, error.to_string()));
                self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
                let mut summary = CompactionSummaryMessageComponent::new(message, Some(self.markdown_theme()));
                summary.set_expanded(self.tool_output_expanded.get());
                self.chat_container.borrow_mut().add_child(component(summary));
            }
            role if role == "branchSummary" => {
                let message = crate::core::messages::BranchSummaryMessage {
                    role: pi_agent_core::harness::messages::BranchSummaryRole::BranchSummary,
                    summary: json_string(object, &JsString::from("summary")),
                    from_id: ((&json_string(object, &JsString::from("fromId"))).clone()).into(),
                    timestamp: object.get("timestamp").and_then(JsValue::as_f64).unwrap_or_default(),
                };
                self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
                let mut summary = BranchSummaryMessageComponent::new(message, Some(self.markdown_theme()));
                summary.set_expanded(self.tool_output_expanded.get());
                self.chat_container.borrow_mut().add_child(component(summary));
            }
            _ => {
                if object.get("display").and_then(JsValue::as_bool) == Some(true) {
                    let renderer = self.session.borrow().extension_runner().get_message_renderer(&role);
                    let content = match object.get("content") {
                        Some(JsValue::String(text)) => {
                            crate::core::extensions::types::CustomMessageContent::Text(text.clone())
                        }
                        Some(JsValue::Array(_)) => crate::core::extensions::types::CustomMessageContent::Blocks(
                            js_json::from_value(&object.get("content").cloned().unwrap_or_default())
                                .unwrap_or_else(|error| throw_uncaught(ERROR, error.to_string())),
                        ),
                        _ => crate::core::extensions::types::CustomMessageContent::Text(JsString::default()),
                    };
                    let message = crate::core::extensions::types::CustomMessageValue {
                        custom_type: role.clone(),
                        content,
                        display: true,
                        details: object.get("details").cloned(),
                        timestamp: object.get("timestamp").and_then(JsValue::as_f64).unwrap_or_default(),
                    };
                    let mut custom = CustomMessageComponent::new(message, renderer, Some(self.markdown_theme()));
                    custom.set_expanded(self.tool_output_expanded.get());
                    self.chat_container.borrow_mut().add_child(component(custom));
                }
            }
        }
    }

    fn render_initial_messages(&self) {
        self.footer.borrow_mut().invalidate();
        self.update_editor_border_color();
        let entries = self.session.borrow().session_manager().build_context_entries();
        self.render_session_entries_with_cwd(&entries, None, true);
        self.render_project_trust_warning_if_needed();
        let all_entries = self.session.borrow().session_manager().get_entries();
        let compaction_count = all_entries
            .iter()
            .filter(|entry| matches!(entry.known().as_deref(), Some(KnownSessionEntry::Compaction(_))))
            .count();
        if compaction_count > 0 {
            let times = if compaction_count == 1 {
                JsString::from("1 time")
            } else {
                js_format!("{} times", compaction_count)
            };
            self.show_status(&js_format!("Session compacted {}", times));
        }
    }

    /// `renderProjectTrustWarningIfNeeded`.
    fn render_project_trust_warning_if_needed(&self) {
        if self.settings(|settings| settings.is_project_trusted())
            || !crate::core::trust_manager::has_trust_requiring_project_resources(&self.cwd())
                .unwrap_or_else(|error| throw_uncaught(ERROR, error.message().clone()))
        {
            return;
        }
        if !self.chat_container.borrow().children().is_empty() {
            self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
        }
        let message = js_format!(
            "This project is not trusted. Project {} resources and packages are ignored. Use /trust to save a trust decision, then restart pi.",
            crate::config::config_dir_name()
        );
        self.chat_container.borrow_mut().add_child(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Warning, &message)),
            1,
            0,
        )));
    }

    /// `renderSessionEntries`: custom entries render through their registered
    /// renderer; every other entry yields its context messages.
    pub fn render_session_entries(&self, entries: &[SessionEntry]) {
        self.render_session_entries_with_cwd(entries, None, false);
    }

    /// `renderSessionEntries` with `sourceCwd` and `populateHistory` (user
    /// messages are added to the editor history; only the initial render sets it).
    fn render_session_entries_with_cwd(
        &self,
        entries: &[SessionEntry],
        source_cwd: Option<&JsString>,
        populate_history: bool,
    ) {
        self.clear_pending_tools();
        let tool_cwd = source_cwd.cloned().unwrap_or_else(|| self.cwd());
        let mut rendered_pending_tools: IndexMap<JsString, ComponentRef> = IndexMap::new();
        for entry in entries {
            if let Some(KnownSessionEntry::Custom(custom)) = entry.known().as_deref() {
                self.add_custom_entry_to_chat(custom);
                continue;
            }
            for message in crate::core::session_manager::session_entry_to_context_messages(entry) {
                self.render_session_message(message, &mut rendered_pending_tools, &tool_cwd, populate_history);
            }
        }
        for (tool_call_id, component_ref) in rendered_pending_tools {
            if let Some(started_at) = self.get_pending_tool_started_at(&tool_call_id)
                && let Some(component_value) = component_ref
                    .borrow_mut()
                    .as_any_mut()
                    .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
            {
                component_value.mark_execution_started(started_at);
            }
            self.pending_tools.borrow_mut().insert(tool_call_id, component_ref);
        }
        self.ui.request_render();
    }

    /// `renderSessionItems` message branch.
    fn render_session_message(
        &self,
        message: AgentMessage,
        rendered_pending_tools: &mut IndexMap<JsString, ComponentRef>,
        tool_cwd: &JsString,
        populate_history: bool,
    ) {
        match &message {
            AgentMessage::Assistant(assistant) => {
                self.add_message_to_chat(&message, false);
                for content in &assistant.content {
                    let pi_ai::types::AssistantContent::ToolCall(call) = content else {
                        continue;
                    };
                    let component_ref = self.create_tool_component(
                        &call.name,
                        &call.id,
                        JsValue::Object(call.arguments.clone()),
                        tool_cwd,
                    );
                    if let Some(component_value) = component_ref
                        .borrow_mut()
                        .as_any_mut()
                        .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                    {
                        component_value.set_expanded(self.tool_output_expanded.get());
                    }
                    self.chat_container.borrow_mut().add_child(Rc::clone(&component_ref));
                    if matches!(
                        assistant.stop_reason,
                        pi_ai::types::StopReason::Aborted | pi_ai::types::StopReason::Error
                    ) {
                        let error_message = if assistant.stop_reason == pi_ai::types::StopReason::Aborted {
                            let retry_attempt = self.session().retry_attempt();
                            if retry_attempt > 0 {
                                js_format!(
                                    "Aborted after {} retry attempt{}",
                                    retry_attempt,
                                    if retry_attempt > 1 { "s" } else { "" }
                                )
                            } else {
                                JsString::from("Operation aborted")
                            }
                        } else {
                            assistant
                                .error_message
                                .clone()
                                .unwrap_or_else(|| JsString::from("Error"))
                        };
                        if let Some(component_value) = component_ref
                            .borrow_mut()
                            .as_any_mut()
                            .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                        {
                            component_value.update_result(
                                ToolExecutionResult {
                                    content: vec![pi_ai::types::TextOrImageContent::Text(
                                        pi_ai::types::TextContent::new(error_message),
                                    )],
                                    details: None,
                                    is_error: true,
                                },
                                false,
                                None,
                            );
                        }
                    } else {
                        rendered_pending_tools.insert((call.id.clone()).into(), component_ref);
                    }
                }
                if !matches!(
                    assistant.stop_reason,
                    pi_ai::types::StopReason::Aborted | pi_ai::types::StopReason::Error
                ) {
                    self.maybe_show_cache_miss_notice(assistant);
                }
            }
            AgentMessage::ToolResult(result) => {
                let Some(component_ref) = rendered_pending_tools.shift_remove(&result.tool_call_id) else {
                    return;
                };
                let timing = self.completed_tool_timings.borrow().get(&result.tool_call_id).copied();
                if let Some(component_value) = component_ref
                    .borrow_mut()
                    .as_any_mut()
                    .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                {
                    if let Some((started_at, _)) = timing {
                        component_value.mark_execution_started(started_at);
                    }
                    component_value.update_result(
                        ToolExecutionResult {
                            content: result.content.clone(),
                            details: result.details.clone(),
                            is_error: result.is_error,
                        },
                        false,
                        timing.map(|(_, finished_at)| finished_at),
                    );
                }
            }
            _ => self.add_message_to_chat(&message, populate_history),
        }
    }

    /// `getPendingToolStartedAt`.
    fn get_pending_tool_started_at(&self, tool_call_id: impl Into<JsString>) -> Option<f64> {
        let tool_call_id = tool_call_id.into();
        let Some(store) = self.multi_agent_store.as_ref() else {
            return self.executing_tool_started_at.borrow().get(&tool_call_id).copied();
        };
        let Some(selected_agent_id) = store.get_selected_agent_id() else {
            return self.executing_tool_started_at.borrow().get(&tool_call_id).copied();
        };
        let activity = store.get_agent(&selected_agent_id)?.current_activity?;
        match activity {
            crate::core::multi_agent_store::AgentCurrentActivity::Tool {
                started_at,
                tool_call_id: activity_tool_call_id,
                ..
            } if activity_tool_call_id == tool_call_id => {
                let started_at = pi_js_compat::date::date_parse_utf16(started_at.units());
                (!started_at.is_nan()).then_some(started_at)
            }
            _ => None,
        }
    }

    /// `addCustomEntryToChat`.
    fn add_custom_entry_to_chat(&self, entry: &crate::core::session_manager::CustomEntry) -> bool {
        let Some(renderer) = self.session().extension_runner().get_entry_renderer(&entry.custom_type) else {
            return false;
        };
        let mut component_value = CustomEntryComponent::new(entry.clone(), renderer);
        component_value.set_expanded(self.tool_output_expanded.get());
        if !component_value.has_content() {
            return false;
        }
        let component_ref = component(component_value);
        if let Some(streaming) = self.streaming_component.borrow().as_ref() {
            let mut chat = self.chat_container.borrow_mut();
            if let Some(index) = chat.children().iter().position(|child| Rc::ptr_eq(child, streaming)) {
                chat.insert_child(index, component_ref);
                return true;
            }
        }
        self.chat_container.borrow_mut().add_child(component_ref);
        true
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
        let active_tools: Vec<JsString> = session
            .get_active_tool_names()
            .into_iter()
            .map(JsString::from)
            .collect();
        let prompt_templates = session.prompt_templates();
        let extension_runner = session.extension_runner();
        drop(session);

        let mut source_infos: HashMap<JsString, SourceInfo> = HashMap::new();
        for extension in &extensions.extensions {
            let registration = extension.read();
            source_infos.insert(registration.path.clone(), registration.source_info.clone());
        }
        for skill in &skills.skills {
            source_infos.insert(skill.file_path.clone(), skill.source_info.clone());
        }
        for prompt in &prompts.prompts {
            source_infos.insert(prompt.file_path.clone(), prompt.source_info.clone());
        }
        for theme in &themes.themes {
            if let (Some(source_path), Some(source_info)) = (&theme.source_path, &theme.source_info) {
                source_infos.insert(source_path.clone(), source_info.clone());
            }
        }

        let expanded = self.startup_expansion_state();
        let cwd = self.cwd();

        if show_listing {
            let mut context_files: Vec<JsString> = Vec::new();
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
                let expanded_body = JsString::join(
                    context_files.iter().map(|path| {
                        with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("  {}", format_display_path(path))))
                    }),
                    "\n",
                );
                let compact_items = context_files
                    .iter()
                    .map(|path| format_context_path(path, &cwd))
                    .collect::<Vec<_>>();
                self.add_loaded_section(
                    &JsString::from("Context"),
                    &format_compact_list(&compact_items, false),
                    &expanded_body,
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
            if !rules_files.rules_files.is_empty() {
                let expanded_body = JsString::join(
                    rules_files.rules_files.iter().map(|file| {
                        with_theme(|theme| {
                            theme.fg(ThemeColor::Dim, &js_format!("  {}", format_display_path(&file.path)))
                        })
                    }),
                    "\n",
                );
                let compact_items = rules_files
                    .rules_files
                    .iter()
                    .map(|file| format_display_path(&file.path))
                    .collect::<Vec<_>>();
                self.add_loaded_section(
                    &JsString::from("User Rules"),
                    &format_compact_list(&compact_items, false),
                    &expanded_body,
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
            if !active_tools.is_empty() {
                self.add_loaded_section(
                    &JsString::from("Tools"),
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
                    &JsString::from("Skills"),
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
                let by_path: HashMap<&JsString, &JsString> = prompt_templates
                    .iter()
                    .map(|template| (&template.file_path, &template.name))
                    .collect();
                let expanded_body = format_scope_groups(
                    &groups,
                    |item| match by_path.get(&item.path) {
                        Some(name) => js_format!("/{}", name),
                        None => format_display_path(&item.path),
                    },
                    |item, _source| match by_path.get(&item.path) {
                        Some(name) => js_format!("/{}", name),
                        None => format_display_path(&item.path),
                    },
                );
                let compact_items = prompt_templates
                    .iter()
                    .map(|template| js_format!("/{}", template.name))
                    .collect::<Vec<_>>();
                self.add_loaded_section(
                    &JsString::from("Prompts"),
                    &format_compact_list(&compact_items, true),
                    &expanded_body,
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
            let listed_extensions: Vec<(JsString, Option<SourceInfo>)> = extensions
                .extensions
                .iter()
                .filter(|extension| extension.read().hidden != Some(true))
                .map(|extension| {
                    let registration = extension.read();
                    (registration.path.clone(), Some(registration.source_info.clone()))
                })
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
                    &JsString::from("Extensions"),
                    &format_compact_list(&compact_labels, true),
                    &expanded_body,
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
            let custom_themes: Vec<(JsString, Option<SourceInfo>, JsString)> = themes
                .themes
                .iter()
                .filter_map(|theme| {
                    let source_path = theme.source_path.as_ref()?;
                    Some((
                        source_path.clone(),
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
                    &JsString::from("Themes"),
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
                        js_format!(
                            "{}\n{}",
                            with_theme(|theme| theme.fg(ThemeColor::Warning, &js_format!("[{}]", label))),
                            body
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
                    message: ((&error.error).clone()).into(),
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
                        js_format!(
                            "{}\n{}",
                            with_theme(|theme| theme.fg(ThemeColor::Warning, "[Extension issues]")),
                            body
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
        name: &JsString,
        collapsed_body: &JsString,
        expanded_body: &JsString,
        color: ThemeColor,
        expanded: bool,
    ) {
        let collapsed = js_format!("{}\n{}", section_header(name, color), collapsed_body);
        let expanded_text = js_format!("{}\n{}", section_header(name, color), expanded_body);
        let section = ExpandableText::new(move || collapsed.clone(), move || expanded_text.clone(), expanded, 0, 0);
        self.loaded_resources_container
            .borrow_mut()
            .add_child(component(section));
        self.loaded_resources_container
            .borrow_mut()
            .add_child(component(Spacer::new(1)));
    }

    fn set_theme_instance(&self, theme: Arc<Theme>) -> ThemeSetResult {
        crate::modes::interactive::theme::set_theme_instance(theme);
        self.ui.invalidate();
        self.update_editor_border_color();
        self.ui.request_render();
        ThemeSetResult {
            success: true,
            error: None,
        }
    }

    fn set_theme_name(&self, name: &JsString) -> ThemeSetResult {
        let result = self.theme_controller.borrow_mut().set_theme_name(name, false);
        if result.success {
            self.settings_mut(|settings| {
                if settings.get_theme() != Some(name) {
                    settings.set_theme(name);
                }
            });
            self.update_editor_border_color();
        }
        ThemeSetResult {
            success: result.success,
            error: result.error,
        }
    }

    fn settings_mut<R>(&self, f: impl FnOnce(&mut crate::core::settings_manager::SettingsManager) -> R) -> R {
        f(&mut self.session.borrow().settings_manager())
    }

    /// `getAutocompleteSourceTag`.
    fn get_autocomplete_source_tag(&self, source_info: Option<&SourceInfo>) -> Option<JsString> {
        let source_info = source_info?;
        let scope_prefix = match source_info.scope {
            SourceScope::User => "u",
            SourceScope::Project => "p",
            SourceScope::Temporary => "t",
        };
        let source = source_info.source.trim();
        if ["auto", "local", "cli"].iter().any(|tag| source == *tag) {
            return Some((scope_prefix.to_owned()).into());
        }
        if source.starts_with("npm:") {
            return Some(js_format!("{}:{}", scope_prefix, source));
        }
        if let Some(git_source) = parse_git_url(&source) {
            let reference = git_source
                .reference
                .map(|reference| js_format!("@{}", reference))
                .unwrap_or_default();
            return Some(js_format!(
                "{}:git:{}/{}{}",
                scope_prefix,
                git_source.host,
                git_source.path,
                reference
            ));
        }
        Some((scope_prefix.to_owned()).into())
    }

    /// `prefixAutocompleteDescription`.
    fn prefix_autocomplete_description(
        &self,
        description: Option<&JsString>,
        source_info: Option<&SourceInfo>,
    ) -> Option<JsString> {
        let Some(tag) = self.get_autocomplete_source_tag(source_info) else {
            return description.cloned();
        };
        Some(match description {
            Some(description) => js_format!("[{}] {}", tag, description),
            None => js_format!("[{}]", tag),
        })
    }

    /// `createBaseAutocompleteProvider`.
    fn create_base_autocomplete_provider(&self) -> Rc<dyn AutocompleteProvider> {
        let mut commands: Vec<CommandEntry> = Vec::new();
        for command in crate::core::slash_commands::builtin_slash_commands() {
            let mut slash = SlashCommand::new(command.name);
            slash.description = Some((command.description).into());
            slash.argument_hint = command.argument_hint.map(JsString::from);
            if command.name == "cd" {
                slash.argument_hint = Some(JsString::from("<path>"));
                let cwd = self.cwd();
                slash.get_argument_completions = Some(Box::new(move |prefix: &JsString| {
                    Some(get_directory_completions(prefix, &cwd))
                }));
            }
            if command.name == "model" {
                let session = self.session();
                let scoped = session.scoped_models();
                let models: Vec<Model> = if scoped.is_empty() {
                    session.model_runtime().get_available_snapshot()
                } else {
                    scoped.into_iter().map(|scoped| scoped.model).collect()
                };
                slash.get_argument_completions = Some(Box::new(move |prefix: &JsString| {
                    model_argument_completions(&models, prefix)
                }));
            }
            if command.name == "login" {
                let runtime = self.session().model_runtime().clone();
                slash.get_argument_completions = Some(Box::new(move |prefix: &JsString| {
                    login_argument_completions(&runtime, prefix)
                }));
            }
            commands.push(CommandEntry::Command(slash));
        }

        let templates = self.session().prompt_templates();
        for template in templates {
            let mut slash = SlashCommand::new(&template.name);
            slash.description =
                self.prefix_autocomplete_description(Some(&template.description), Some(&template.source_info));
            slash.argument_hint = template.argument_hint.clone();
            commands.push(CommandEntry::Command(slash));
        }

        let builtin_names: Vec<&'static str> = crate::core::slash_commands::builtin_slash_commands()
            .iter()
            .map(|command| command.name)
            .collect();
        for command in self.session().extension_runner().get_registered_commands() {
            if builtin_names.iter().any(|name| command.name == *name) {
                continue;
            }
            let mut slash = SlashCommand::new(&command.invocation_name);
            slash.description =
                self.prefix_autocomplete_description(command.description.as_ref(), Some(&command.source_info));
            if let Some(completions) = command.get_argument_completions.clone() {
                slash.get_argument_completions = Some(Box::new(move |prefix: &JsString| {
                    completions(prefix)
                        .and_then(|items| {
                            items
                                .map(|items| {
                                    items
                                        .into_iter()
                                        .map(|item| {
                                            autocomplete_item_from_value(&item)
                                                .ok_or_else(|| JsString::from("Invalid extension argument completion"))
                                        })
                                        .collect::<Result<Vec<_>, _>>()
                                })
                                .transpose()
                        })
                        .unwrap_or_else(|error| throw_uncaught(ERROR, error))
                }));
            }
            commands.push(CommandEntry::Command(slash));
        }

        if self.settings(|settings| settings.get_enable_skill_commands()) {
            let skills = self.session().resource_loader().get_skills();
            for skill in skills.skills {
                let mut slash = SlashCommand::new(js_format!("skill:{}", skill.name));
                slash.description =
                    self.prefix_autocomplete_description(Some(&skill.description), Some(&skill.source_info));
                commands.push(CommandEntry::Command(slash));
            }
        }

        Rc::new(CombinedAutocompleteProvider::new(
            commands,
            self.cwd(),
            self.fd_path.borrow().clone(),
        ))
    }

    /// `setupAutocompleteProvider`.
    fn setup_autocomplete_provider(&self) {
        let mut provider = self.create_base_autocomplete_provider();
        for factory in self.autocomplete_provider_wrappers.borrow().iter() {
            provider = factory(Rc::clone(&provider));
        }
        *self.autocomplete_provider.borrow_mut() = Some(Rc::clone(&provider));
        self.default_editor
            .borrow_mut()
            .set_autocomplete_provider(Rc::clone(&provider));
        let current = self.editor();
        let is_default = Rc::ptr_eq(
            &current,
            &(self.default_editor.clone() as Rc<RefCell<dyn EditorComponent>>),
        );
        if !is_default {
            current.borrow_mut().set_autocomplete_provider(provider);
        }
    }

    /// `setCustomEditorComponent`.
    fn set_custom_editor_component(&self, factory: Option<ExtensionEditorFactory>) {
        *self.editor_component_factory.borrow_mut() = factory.clone();
        *self
            .ui_shared
            .editor_component_factory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = factory.clone();

        let current_text = self.editor().borrow().get_text();
        crate::modes::interactive::working_editor::clear_working_editor(&mut *self.editor().borrow_mut());
        self.editor_container.borrow_mut().clear();

        let next_editor: Rc<RefCell<dyn EditorComponent>> = match factory {
            Some(factory) => {
                let new_editor = factory(
                    ExtensionUiTui::new(Arc::new(InteractiveUiTuiBackend {
                        mode_id: self.ui_id.get(),
                        shared: Arc::clone(&self.ui_shared),
                        sender: self.ui_sender.clone(),
                    })),
                    crate::modes::interactive::theme::get_editor_theme(),
                    self.keybindings.clone(),
                );
                {
                    let mut default_editor = self.default_editor.borrow_mut();
                    let on_submit = default_editor.editor_mut().on_submit.clone();
                    let on_change = default_editor.editor_mut().on_change.clone();
                    let border_color = default_editor.get_border_color();
                    let padding_x = default_editor.get_padding_x();
                    let autocomplete_max_visible = default_editor.get_autocomplete_max_visible();
                    let on_escape = default_editor.on_escape.clone();
                    let on_ctrl_d = default_editor.on_ctrl_d.clone();
                    let on_paste_image = default_editor.on_paste_image.clone();
                    let on_extension_shortcut = default_editor.on_extension_shortcut.clone();
                    let action_handlers = default_editor.action_handlers.clone();
                    drop(default_editor);

                    let mut editor = new_editor.borrow_mut();
                    editor.set_text(&current_text);
                    editor.set_border_color(border_color);
                    editor.set_padding_x(padding_x);
                    editor.set_autocomplete_max_visible(autocomplete_max_visible);
                    if let Some(provider) = self.autocomplete_provider.borrow().clone() {
                        editor.set_autocomplete_provider(provider);
                    }
                    if let Some(custom) = editor.as_any_mut().and_then(|any| any.downcast_mut::<CustomEditor>()) {
                        custom.editor_mut().on_submit = on_submit;
                        custom.editor_mut().on_change = on_change;
                        if custom.on_escape.is_none() {
                            custom.on_escape = on_escape;
                        }
                        if custom.on_ctrl_d.is_none() {
                            custom.on_ctrl_d = on_ctrl_d;
                        }
                        if custom.on_paste_image.is_none() {
                            custom.on_paste_image = on_paste_image;
                        }
                        if custom.on_extension_shortcut.is_none() {
                            custom.on_extension_shortcut = on_extension_shortcut;
                        }
                        for (action, handler) in action_handlers {
                            custom.action_handlers.insert(action, handler);
                        }
                    }
                }
                new_editor
            }
            None => {
                self.default_editor.borrow_mut().set_text(&current_text);
                self.default_editor.clone()
            }
        };

        let handle: ComponentRef = component(AnyEditorHandle(Rc::clone(&next_editor)));
        *self.editor.borrow_mut() = next_editor;
        *self.editor_handle.borrow_mut() = Rc::clone(&handle);
        self.editor_container.borrow_mut().add_child(Rc::clone(&handle));
        self.update_editor_border_color();
        self.sync_working_editor_state();
        self.ui.set_focus(Some(handle));
        self.ui.request_render();
    }

    fn update_editor_border_color(&self) {
        let color = if self.is_bash_mode.get() {
            ThemeColor::BashMode
        } else {
            ThemeColor::Border
        };
        let style: pi_tui::components::text::StyleFn =
            Rc::new(move |text: &JsString| with_theme(|theme| theme.fg(color, text)));
        self.editor().borrow_mut().set_border_color(style);
    }

    fn render_widgets(&self) {
        self.render_widget_container(
            &self.widget_container_above,
            &self.extension_widgets_above.borrow(),
            true,
            true,
        );
        if let Some(store) = self.multi_agent_store.as_ref()
            && let Some(theme) = crate::modes::interactive::theme::current_theme_if_initialized()
        {
            self.widget_container_above
                .borrow_mut()
                .add_child(component(AgentSelectionBannerComponent::new(
                    Some(Arc::clone(store)),
                    theme,
                )));
        }
        self.render_widget_container(
            &self.widget_container_below,
            &self.extension_widgets_below.borrow(),
            false,
            false,
        );
    }

    fn render_widget_container(
        &self,
        container: &Rc<RefCell<Container>>,
        widgets: &IndexMap<JsString, ExtensionWidget>,
        spacer_when_empty: bool,
        leading_spacer: bool,
    ) {
        let mut container = container.borrow_mut();
        container.clear();
        if widgets.is_empty() {
            if spacer_when_empty {
                container.add_child(component(Spacer::new(1)));
            }
            return;
        }
        if leading_spacer {
            container.add_child(component(Spacer::new(1)));
        }
        for widget in widgets.values() {
            match widget {
                ExtensionWidget::Lines(lines) => {
                    container.add_child(component(Text::new(JsString::join(lines, "\n"), 1, 0)));
                }
                ExtensionWidget::Component(widget) => container.add_child(Rc::clone(widget)),
            }
        }
    }

    fn add_extension_widget(&self, key: &JsString, content: Option<ExtensionWidget>, placement: WidgetPlacement) {
        self.extension_widgets_above.borrow_mut().shift_remove(key);
        self.extension_widgets_below.borrow_mut().shift_remove(key);
        let Some(content) = content else {
            self.render_widgets();
            self.ui.request_render();
            return;
        };
        let widgets = match placement {
            WidgetPlacement::AboveEditor => &self.extension_widgets_above,
            WidgetPlacement::BelowEditor => &self.extension_widgets_below,
        };
        widgets.borrow_mut().insert(key.to_owned(), content);
        self.render_widgets();
        self.ui.request_render();
    }

    fn string_widget_lines(content: &[JsString]) -> Vec<JsString> {
        let mut lines: Vec<JsString> = content.iter().take(MAX_WIDGET_LINES).cloned().collect();
        if content.len() > MAX_WIDGET_LINES {
            lines.push(with_theme(|theme| {
                theme.fg(ThemeColor::Muted, "... (widget truncated)")
            }));
        }
        lines
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
                        &js_format!(
                            "  {} {}",
                            with_theme(|theme| theme.fg(ThemeColor::Accent, "Steering:")),
                            (&message).clone()
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
                        &js_format!(
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

    /// `addRenderedMessageToEditorHistory`: the default editor skips persistence
    /// (the prompt is already in the control DB); custom editors use `addToHistory`.
    fn add_rendered_message_to_editor_history(&self, text: &JsString) {
        let editor = self.editor();
        let default_editor: Rc<RefCell<dyn EditorComponent>> = self.default_editor.clone();
        if Rc::ptr_eq(&editor, &default_editor) {
            self.default_editor
                .borrow_mut()
                .add_to_history_without_persistence(text);
            return;
        }
        editor.borrow_mut().add_to_history(text);
    }

    fn add_submitted_text_to_history(&self, text: &JsString) {
        if is_skill_invocation(text, &self.session().prompt_templates()) {
            return;
        }
        self.editor().borrow_mut().add_to_history(text);
    }

    fn clear_editor(&self) {
        self.editor().borrow_mut().set_text(&JsString::from(""));
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

    fn detach_running_tool(&self) -> bool {
        let session = self.session();
        if !session.has_detachable_tool() {
            return false;
        }
        if !session.detach_running_tool() {
            return false;
        }
        self.show_status("Detached tool to background job");
        true
    }

    fn toggle_thinking_block_visibility(&self) {
        let hidden = !self.hide_thinking_block.get();
        self.hide_thinking_block.set(hidden);
        self.settings_mut(|settings| settings.set_hide_thinking_block(hidden));
        self.ui.request_render();
    }

    fn cycle_thinking_level(&self) {
        let result = self.session.borrow().cycle_thinking_level();
        match result {
            Ok(None) => self.show_status("Current model does not support thinking"),
            Ok(Some(level)) => {
                self.footer.borrow_mut().invalidate();
                self.update_editor_border_color();
                self.show_status(&js_format!("Thinking level: {}", level.as_str()));
            }
            Err(error) => self.show_error(error.message()),
        }
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
        if self.editor().borrow().get_text().is_empty() {
            self.actions.borrow_mut().push_back(ModeAction::Shutdown);
        }
    }

    /// `handleCtrlZ`: restore the terminal and suspend the process group.
    fn handle_ctrl_z(self: &Rc<Self>) {
        self.handle_ctrl_z_for_platform(&std::env::consts::OS.into());
    }

    fn handle_ctrl_z_for_platform(self: &Rc<Self>, platform: &JsString) {
        if platform == "windows" {
            self.show_status("Suspend to background is not supported on Windows");
            return;
        }
        self.suspend_to_background();
    }

    fn suspend_to_background(self: &Rc<Self>) {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};

            let (Ok(sigint), Ok(mut sigcont)) = (signal(SignalKind::interrupt()), signal(SignalKind::from_raw(18)))
            else {
                self.show_error("Could not install suspend signal handlers");
                return;
            };
            let mode = Rc::clone(self);
            tokio::task::spawn_local(async move {
                let _ = sigcont.recv().await;
                drop(sigint);
                let _ = mode.ui.start();
                mode.ui.request_render_force();
            });

            self.ui.stop();
            let Some(pid) = rustix::process::Pid::from_raw(0) else {
                self.show_error("Could not suspend the process group");
                let _ = self.ui.start();
                return;
            };
            if let Err(error) = rustix::process::kill_process_group(pid, rustix::process::Signal::TSTP) {
                self.show_error(&std::io::Error::from(error).to_string());
                let _ = self.ui.start();
            }
        }
        #[cfg(not(unix))]
        self.show_status("Suspend to background is not supported on Windows");
    }

    /// `handleRightClickPaste`.
    async fn handle_right_click_paste(self: &Rc<Self>) {
        let Some(target) = self.ui.get_focused_component() else {
            return;
        };
        let environment = crate::utils::clipboard_image::ClipboardEnvironment::from_process();
        let native = crate::utils::clipboard_native::clipboard();
        let text = crate::utils::clipboard::read_clipboard_text(&environment, native.as_deref()).await;
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return;
        };
        let still_focused = self
            .ui
            .get_focused_component()
            .is_some_and(|current| Rc::ptr_eq(&current, &target));
        if !still_focused {
            return;
        }
        target
            .borrow_mut()
            .handle_input(&js_format!("\x1b[200~{}\x1b[201~", text));
        self.ui.request_render();
    }

    /// `handleClipboardPaste`: insert clipboard image paths, otherwise text.
    async fn handle_clipboard_paste(&self) {
        let environment = crate::utils::clipboard_image::ClipboardEnvironment::from_process();
        let native = crate::utils::clipboard_native::clipboard();
        let native = native.as_deref();
        if let Ok(Some(image)) = crate::utils::clipboard_image::read_clipboard_image(&environment, native).await {
            let extension =
                crate::utils::clipboard_image::extension_for_image_mime_type(&image.mime_type).unwrap_or("png");
            let file_path = node_join(
                JsString::from(std::env::temp_dir().to_string_lossy().as_ref()),
                js_format!("pi-clipboard-{}.{}", pi_ai::utils::uuid::random_uuid(), extension),
            );
            if std::fs::write(encode_node_utf8(&file_path), &image.bytes).is_ok() {
                self.clipboard_temp_files.borrow_mut().track(file_path.clone());
                self.editor().borrow_mut().insert_text_at_cursor(&file_path);
                self.ui.request_render();
                return;
            }
        }
        if let Some(text) = crate::utils::clipboard::read_clipboard_text(&environment, native).await
            && !text.is_empty()
        {
            self.editor().borrow_mut().insert_text_at_cursor(&JsString::from(&text));
            self.ui.request_render();
        }
    }

    fn handle_editor_escape(&self) {
        if self.is_bash_mode.get() {
            self.is_bash_mode.set(false);
            self.update_editor_border_color();
            return;
        }
        if self.cancel_selected_agent_turn() {
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
            self.editor().borrow_mut().set_text(&JsString::from(""));
            self.ui.request_render();
        }
    }

    /// `InteractiveMode.stop`: releases the session subscription and stops the TUI.
    pub fn stop(&self) {
        self.dispose_active_selector();
        if self.settings(|settings| settings.get_show_terminal_progress()) {
            self.ui.terminal().borrow_mut().set_progress(false);
        }
        if let Some(loader) = self.loading_animation.borrow_mut().take() {
            if let Some(loader) = loader
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<Loader>())
            {
                loader.stop();
            }
            *self.working_loader_view.borrow_mut() = None;
        }
        self.stop_child_activity_timer();
        self.clear_child_agent_view();
        self.theme_controller.borrow_mut().disable_auto_sync();
        self.clear_extension_terminal_input_listeners();
        if let Some(component) = self.custom_footer.borrow_mut().take() {
            dispose_extension_component(&component);
        }
        if let Some(component) = self.default_extension_footer.borrow_mut().take() {
            dispose_extension_component(&component);
        }
        crate::modes::interactive::components::footer::dispose();
        self.footer_data_provider.dispose();
        if let Some(id) = self.unsubscribe.get() {
            self.session.borrow().unsubscribe(id);
            self.unsubscribe.set(None);
        }
        if let Some(subscription) = self.multi_agent_subscription.borrow_mut().take() {
            subscription.unsubscribe();
        }
        *self.multi_agent_updates.borrow_mut() = None;
        self.cancel_partial_update_render();
        self.dispose_active_bash_components();
        self.clear_pending_tool_components();
        self.close_response_complete_notification();
        self.clipboard_temp_files.borrow_mut().cleanup_all();
        if self.is_initialized.get() {
            self.ui.stop();
            self.is_initialized.set(false);
        }
        self.unregister_signal_handlers();
    }

    /// `handleFatalRuntimeError`: report, stop the TUI on the transcript, and
    /// exit 1. The TypeScript signature returns `Promise<never>`.
    fn handle_fatal_runtime_error(&self, prefix: &JsString, error: impl Into<JsString>) -> ! {
        let error = error.into();
        self.show_error(&js_format!("{}: {}", prefix, error));
        crate::modes::interactive::theme::stop_theme_watcher();
        self.stop();
        std::process::exit(1);
    }

    /// `disposeActiveSelector`: runs the active selector's dispose hook.
    fn dispose_active_selector(&self) {
        let dispose = self.active_selector_dispose.borrow_mut().take();
        if let Some(dispose) = dispose {
            dispose();
        }
    }

    /// `clearExtensionTerminalInputListeners`: unsubscribes every listener the
    /// extension API registered on the terminal.
    fn clear_extension_terminal_input_listeners(&self) {
        for (id, _) in self.terminal_input_listeners.borrow().iter() {
            self.ui.remove_input_listener(*id);
        }
        self.terminal_input_listeners.borrow_mut().clear();
    }

    /// `cancelPartialUpdateRender`: the Rust render loop keeps the throttle in
    /// `tokio::select!` instead of a stored timeout, so nothing to cancel.
    fn cancel_partial_update_render(&self) {}

    /// `disposeActiveBashComponents`: drops the active bash execution views.
    fn dispose_active_bash_components(&self) {
        self.active_bash_components.borrow_mut().clear();
    }

    /// `clearPendingToolComponents`: drops components awaiting a tool result.
    fn clear_pending_tool_components(&self) {
        self.pending_tools.borrow_mut().clear();
    }

    fn close_response_complete_notification(&self) {
        let notification = self.response_complete_notification.borrow_mut().take();
        if let Some(notification) = notification {
            notification.close();
        }
    }
}

/// `customFooter?.dispose?.()`: extension components expose an optional
/// `dispose`; the adapter forwards it to `ExtensionUiComponent::dispose`.
fn dispose_extension_component(component_ref: &ComponentRef) {
    let Ok(mut component) = component_ref.try_borrow_mut() else {
        return;
    };
    if let Some(any) = component.as_any_mut()
        && let Some(adapter) = any.downcast_mut::<ExtensionComponentAdapter>()
    {
        adapter.inner.dispose();
    }
}

/// Component adapter for the shared footer component.
pub struct FooterHandle(pub Rc<RefCell<FooterComponent>>);

impl Component for FooterHandle {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0.borrow_mut().render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.0.borrow_mut().invalidate();
    }
}

/// `MarkdownTheme` wrapper applying the configured code-block indent.
struct SettingsMarkdownTheme {
    inner: Rc<dyn pi_tui::components::markdown::MarkdownTheme>,
    code_block_indent: JsString,
}

impl pi_tui::components::markdown::MarkdownTheme for SettingsMarkdownTheme {
    fn heading(&self, text: &JsString) -> JsString {
        self.inner.heading(text)
    }
    fn link(&self, text: &JsString) -> JsString {
        self.inner.link(text)
    }
    fn link_url(&self, text: &JsString) -> JsString {
        self.inner.link_url(text)
    }
    fn code(&self, text: &JsString) -> JsString {
        self.inner.code(text)
    }
    fn code_block(&self, text: &JsString) -> JsString {
        self.inner.code_block(text)
    }
    fn code_block_border(&self, text: &JsString) -> JsString {
        self.inner.code_block_border(text)
    }
    fn quote(&self, text: &JsString) -> JsString {
        self.inner.quote(text)
    }
    fn quote_border(&self, text: &JsString) -> JsString {
        self.inner.quote_border(text)
    }
    fn hr(&self, text: &JsString) -> JsString {
        self.inner.hr(text)
    }
    fn list_bullet(&self, text: &JsString) -> JsString {
        self.inner.list_bullet(text)
    }
    fn bold(&self, text: &JsString) -> JsString {
        self.inner.bold(text)
    }
    fn italic(&self, text: &JsString) -> JsString {
        self.inner.italic(text)
    }
    fn strikethrough(&self, text: &JsString) -> JsString {
        self.inner.strikethrough(text)
    }
    fn underline(&self, text: &JsString) -> JsString {
        self.inner.underline(text)
    }
    fn highlight_code(&self, code: &JsString, lang: Option<&JsString>) -> Option<Vec<JsString>> {
        self.inner.highlight_code(code, lang)
    }
    fn code_block_indent(&self) -> JsString {
        (&self.code_block_indent).clone()
    }
}

#[cfg(unix)]
fn spawn_signal_handlers(mode: &Rc<InteractiveMode>) -> Vec<tokio::task::JoinHandle<()>> {
    use tokio::signal::unix::{SignalKind, signal};

    let terminate = {
        let mode = Rc::clone(mode);
        tokio::task::spawn_local(async move {
            let Ok(mut signals) = signal(SignalKind::terminate()) else {
                return;
            };
            if signals.recv().await.is_some() {
                kill_tracked_detached_children();
                mode.shutdown_from_signal().await;
            }
        })
    };
    let hangup = {
        let mode = Rc::clone(mode);
        tokio::task::spawn_local(async move {
            let Ok(mut signals) = signal(SignalKind::hangup()) else {
                return;
            };
            if signals.recv().await.is_some() {
                kill_tracked_detached_children();
                mode.restart_from_signal().await;
            }
        })
    };
    vec![terminate, hangup]
}

#[cfg(not(unix))]
fn spawn_signal_handlers(_mode: &Rc<InteractiveMode>) -> Vec<tokio::task::JoinHandle<()>> {
    Vec::new()
}

/// Runs interactive mode on a local task set.
pub async fn run_interactive_mode(
    runtime: Arc<tokio::sync::Mutex<AgentSessionRuntime>>,
    options: InteractiveModeOptions,
    terminal: Option<Rc<RefCell<dyn Terminal>>>,
) -> Result<(), JsString> {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let mode = InteractiveMode::new(runtime, options, terminal).await?;
            mode.run().await
        })
        .await
}

fn read_pipe_text(pipe: Option<impl std::io::Read>) -> JsString {
    let Some(mut pipe) = pipe else {
        return JsString::default();
    };
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut pipe, &mut bytes).unwrap_or_else(|error| throw_uncaught(ERROR, error.to_string()));
    String::from_utf8_lossy(&bytes).as_ref().into()
}

struct GhGistRun {
    status: std::process::ExitStatus,
    stdout: JsString,
    stderr: JsString,
}

async fn run_gh_gist(
    gh_program: &JsString,
    tmp_path: &JsString,
    pid: &std::sync::atomic::AtomicU32,
) -> Result<GhGistRun, JsString> {
    let mut child = std::process::Command::new(encode_node_utf8(gh_program))
        .args(["gist", "create", "--public=false"])
        .arg(encode_node_utf8(tmp_path))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            crate::utils::child_process::node_spawn_error_message(
                "spawn",
                &pi_js_compat::js_json::JsString::from("gh"),
                &error,
            )
        })?;
    pid.store(child.id(), std::sync::atomic::Ordering::SeqCst);
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_task = tokio::task::spawn_blocking(move || read_pipe_text(stdout));
    let stderr_task = tokio::task::spawn_blocking(move || read_pipe_text(stderr));
    let wait_task = tokio::task::spawn_blocking(move || child.wait());
    let (stdout, stderr, status) = tokio::join!(stdout_task, stderr_task, wait_task);
    pid.store(0, std::sync::atomic::Ordering::SeqCst);
    let status = match status {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            return Err(crate::utils::child_process::node_spawn_error_message(
                "spawn",
                &JsString::from("gh"),
                &error,
            ));
        }
        Err(error) => return Err((error.to_string()).into()),
    };
    Ok(GhGistRun {
        status,
        stdout: stdout.unwrap_or_default(),
        stderr: stderr.unwrap_or_default(),
    })
}

fn json_string(object: &JsObject, key: &JsString) -> JsString {
    object
        .get(key)
        .and_then(JsValue::as_string)
        .cloned()
        .unwrap_or_default()
}

/// Result of [`changelog_display_for`]: the markdown to show, and whether the
/// seen version must be recorded (fresh install or new entries).
struct ChangelogDisplay {
    markdown: Option<JsString>,
    update_version: bool,
}

fn changelog_display_for(
    last_version: Option<&JsString>,
    changelog_path: &std::path::Path,
    has_messages: bool,
) -> ChangelogDisplay {
    if has_messages {
        return ChangelogDisplay {
            markdown: None,
            update_version: false,
        };
    }
    let entries = crate::utils::changelog::parse_changelog(&changelog_path.display().to_string());
    let Some(last_version) = last_version else {
        return ChangelogDisplay {
            markdown: None,
            update_version: true,
        };
    };
    let new_entries = crate::utils::changelog::get_new_entries(&entries, last_version);
    if new_entries.is_empty() {
        return ChangelogDisplay {
            markdown: None,
            update_version: false,
        };
    }
    let markdown = new_entries
        .iter()
        .map(|entry| {
            let entry_version = format!("{}.{}.{}", entry.major, entry.minor, entry.patch);
            crate::utils::changelog::normalize_changelog_links_for_entry(&entry.content, &entry_version)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    ChangelogDisplay {
        markdown: Some((markdown).into()),
        update_version: true,
    }
}

impl InteractiveMode {
    /// Installs synchronous TUI callbacks that enqueue async work.
    fn bind_callbacks(self: &Rc<Self>) {
        let mode = Rc::clone(self);
        self.default_editor.borrow_mut().editor_mut().on_submit = Some(Rc::new(move |text: &JsString| {
            mode.actions.borrow_mut().push_back(ModeAction::Submit(text.to_owned()));
        }));

        let mode = Rc::clone(self);
        self.default_editor.borrow_mut().editor_mut().on_change = Some(Rc::new(move |text: &JsString| {
            mode.is_bash_mode.set(text.trim_start().starts_with('!'));
        }));

        let mode = Rc::clone(self);
        self.default_editor.borrow_mut().on_escape = Some(Rc::new(move || {
            mode.handle_editor_escape();
        }));
        let mode = Rc::clone(self);
        self.default_editor.borrow_mut().on_ctrl_d = Some(Rc::new(move || {
            mode.handle_ctrl_d();
        }));

        fn action(mode: &Rc<InteractiveMode>, handler: impl Fn(&InteractiveMode) + 'static) -> AppActionHandler {
            let mode = Rc::clone(mode);
            Rc::new(move || {
                handler(&mode);
                true
            })
        }

        self.default_editor
            .borrow_mut()
            .on_action("app.clear", action(self, |mode| mode.handle_ctrl_c()));
        self.default_editor.borrow_mut().on_action("app.suspend", {
            let mode = Rc::clone(self);
            Rc::new(move || {
                mode.handle_ctrl_z();
                true
            })
        });
        let paste_mode = Rc::clone(self);
        self.default_editor.borrow_mut().on_paste_image = Some(Rc::new(move || {
            let mode = Rc::clone(&paste_mode);
            tokio::task::spawn_local(async move {
                mode.handle_clipboard_paste().await;
            });
        }));
        self.default_editor.borrow_mut().on_action(
            "app.thinking.cycle",
            action(self, InteractiveMode::cycle_thinking_level),
        );
        self.default_editor.borrow_mut().on_action(
            "app.model.cycleForward",
            action(self, |mode| {
                mode.actions
                    .borrow_mut()
                    .push_back(ModeAction::CycleModel(CycleDirection::Forward));
            }),
        );
        self.default_editor.borrow_mut().on_action(
            "app.model.cycleBackward",
            action(self, |mode| {
                mode.actions
                    .borrow_mut()
                    .push_back(ModeAction::CycleModel(CycleDirection::Backward));
            }),
        );
        self.default_editor.borrow_mut().on_action(
            "app.tools.expand",
            action(self, InteractiveMode::toggle_tool_output_expansion),
        );
        self.default_editor.borrow_mut().on_action(
            "app.tool.background",
            action(self, |mode| {
                mode.detach_running_tool();
            }),
        );
        self.default_editor.borrow_mut().on_action(
            "app.thinking.toggle",
            action(self, InteractiveMode::toggle_thinking_block_visibility),
        );
        self.default_editor.borrow_mut().on_action(
            "app.editor.external",
            action(self, |mode| {
                mode.actions.borrow_mut().push_back(ModeAction::ExternalEditor);
            }),
        );
        self.default_editor.borrow_mut().on_action(
            "app.message.copy",
            action(self, |mode| {
                mode.actions.borrow_mut().push_back(ModeAction::Copy);
            }),
        );
        self.default_editor.borrow_mut().on_action(
            "app.message.followUp",
            action(self, |mode| {
                mode.actions.borrow_mut().push_back(ModeAction::FollowUp);
            }),
        );
        self.default_editor.borrow_mut().on_action(
            "app.message.dequeue",
            action(self, |mode| {
                mode.actions.borrow_mut().push_back(ModeAction::Dequeue);
            }),
        );
        self.default_editor.borrow_mut().on_action(
            "app.session.new",
            action(self, |mode| {
                mode.actions.borrow_mut().push_back(ModeAction::NewSession);
            }),
        );
        self.default_editor.borrow_mut().on_action(
            "app.session.tree",
            action(self, |mode| mode.show_tree_selector(None, None)),
        );
        self.default_editor.borrow_mut().on_action(
            "app.session.fork",
            action(self, |mode| mode.show_user_message_selector()),
        );
        self.default_editor
            .borrow_mut()
            .on_action("app.session.resume", action(self, |mode| mode.show_session_selector()));
        self.default_editor
            .borrow_mut()
            .on_action("app.agent.select", action(self, InteractiveMode::show_agent_switcher));
        self.register_agent_slot_key_handlers();
        self.register_global_agent_slot_input_handler();

        let mode = Rc::clone(self);
        let debug_handler: Rc<dyn Fn()> = Rc::new(move || {
            let mode = Rc::clone(&mode);
            tokio::task::spawn_local(async move {
                if let Err(error) = mode.session().prompt("/debug", PromptOptions::default()).await {
                    mode.show_error(error.message());
                }
            });
        });
        *self.debug_handler.borrow_mut() = Some(Rc::clone(&debug_handler));
        self.ui.set_on_debug(debug_handler);
        self.update_editor_border_color();
    }

    /// Initializes the TUI, resources, extensions, and initial messages.
    pub async fn init(self: &Rc<Self>) -> Result<(), JsString> {
        if self.is_initialized.get() {
            return Ok(());
        }
        ACTIVE_INTERACTIVE_MODE.with(|slot| *slot.borrow_mut() = Some(Rc::downgrade(self)));

        self.register_signal_handlers();

        self.bind_callbacks();

        let http_idle_timeout_ms = self
            .settings(|settings| settings.get_http_idle_timeout_ms())
            .map_err(|error| JsString::from(error.to_string()))?;
        let fetch = crate::core::http_dispatcher::configure_http_dispatcher(http_idle_timeout_ms as f64)?;
        *self.http_fetch.borrow_mut() = Some(Arc::clone(&fetch));

        // Load changelog (only show new entries, skip for resumed sessions).
        *self.changelog_markdown.borrow_mut() = self.get_changelog_for_display();
        let mut fd_manager = crate::utils::tools_manager::ToolsManager::for_process(Arc::clone(&fetch));
        let mut rg_manager = crate::utils::tools_manager::ToolsManager::for_process(fetch);
        let (fd_path, _rg_path) = tokio::join!(fd_manager.ensure_tool("fd"), rg_manager.ensure_tool("rg"));
        *self.fd_path.borrow_mut() = fd_path.map(JsString::from);
        self.render_widgets();
        self.mount_interactive_tui();
        let editor_ref = Rc::clone(&self.editor_handle.borrow());
        self.ui.set_focus(Some(editor_ref));
        self.ui.start().map_err(|error| JsString::from(error.to_string()))?;
        self.is_initialized.set(true);
        self.subscribe_to_multi_agent_store();
        self.update_terminal_current_directory();
        self.update_terminal_title();
        self.theme_controller.borrow_mut().apply_from_settings();
        self.render_widgets();
        self.build_header();
        self.show_loaded_resources(false, true);
        self.show_startup_notices_if_needed();
        self.bind_extensions().await?;
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
    }

    fn build_header(&self) {
        let mut header = self.header_container.borrow_mut();
        header.clear();
        let quiet = self.settings(|settings| settings.get_quiet_startup());
        if !self.options.verbose && quiet {
            header.add_child(component(Text::new("", 0, 0)));
            return;
        }
        let logo = js_format!(
            "{} {}",
            with_theme(|theme| theme.bold(&theme.fg(ThemeColor::Accent, &crate::config::package_config().app_name))),
            with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("v{}", self.version)))
        );
        let instructions = JsString::join(
            [
                JsString::from("Esc to interrupt"),
                JsString::from("Ctrl+C to clear"),
                JsString::from("Ctrl+C twice to exit"),
                JsString::from("Ctrl+D to exit (empty)"),
                JsString::from("Shift+Tab to cycle thinking level"),
                JsString::from("Ctrl+P/Ctrl+N to cycle models"),
                JsString::from("Ctrl+L to select model"),
                JsString::from("Ctrl+O to expand tools"),
                key_hint("app.tool.background", "to background tools"),
                JsString::from("Ctrl+T to expand thinking"),
                JsString::from("Ctrl+G for external editor"),
                JsString::from("/ for commands"),
                JsString::from("! to run bash"),
                JsString::from("!! to run bash (no context)"),
                JsString::from("Alt+Enter to queue follow-up"),
                JsString::from("Alt+Up to edit queued messages"),
            ],
            "\n",
        );
        let onboarding = with_theme(|theme| {
            theme.fg(
                ThemeColor::Dim,
                "Pi can explain its own features and look up its docs. Ask it how to use or extend Pi.",
            )
        });
        let collapsed = js_format!("{}\n{}\n\n{}", logo, instructions, onboarding);
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

    async fn bind_extensions(self: &Rc<Self>) -> Result<(), JsString> {
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
            set_sandbox_profile: {
                let session = session.clone();
                Arc::new(move |profile, scope| {
                    session
                        .set_sandbox_profile(profile, scope)
                        .map_err(|error| HandlerError::new(error.to_string()))
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
                    Box::pin(await_ui_response(receiver))
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
                    Box::pin(await_ui_response(receiver))
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
                    Box::pin(await_ui_response(receiver))
                })
            },
            switch_session: {
                let sender = sender.clone();
                Arc::new(move |path, _options| {
                    let (respond, receiver) = tokio::sync::oneshot::channel();
                    let _ = sender.send(UiRequest::RuntimeSwitchSession { path, respond });
                    Box::pin(await_ui_response(receiver))
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
                        receiver.await.unwrap_or_else(|_| {
                            Err(crate::core::extensions::types::HandlerError::new(
                                "Agent session runtime is not available",
                            ))
                        })
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
            .await
            .map_err(|error| error.message().clone())?;
        self.setup_autocomplete_provider();
        self.footer.borrow_mut().invalidate();
        self.ui.request_render();
        Ok(())
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
    pub async fn run(self: Rc<Self>) -> Result<(), JsString> {
        self.bind_callbacks();
        let (event_sender, mut event_receiver) = tokio::sync::mpsc::unbounded_channel();
        *self.events.borrow_mut() = Some(event_sender);
        self.init().await?;

        let tmux_mode = Rc::clone(&self);
        tokio::task::spawn_local(async move {
            if let Some(warning) = check_tmux_keyboard_setup().await {
                tmux_mode.show_warning(&warning);
            }
        });

        self.process_control_message().await?;

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
            self.handle_terminal_errors();
            self.ui.flush_pending();
            self.poll_timers();
            self.poll_child_view_events();
            self.poll_multi_agent_updates();
            self.update_editor_border_color();
            self.handle_ui_requests().await;
            self.drain_actions().await;
            if self.shutdown_requested.get() {
                break;
            }
        }

        self.shutdown().await;
        Ok(())
    }

    /// `processControlMessage`: submits a claimed control-DB message and records
    /// its completion or failure.
    async fn process_control_message(&self) -> Result<(), JsString> {
        let Some(message) = self.options.control_message.clone() else {
            return Ok(());
        };
        let Some(control_db_path) = self.options.control_db_path.as_ref() else {
            self.show_error("Cannot submit control message without a control DB path");
            return Ok(());
        };
        let result = match self.session().prompt(&message.content, PromptOptions::default()).await {
            Ok(()) => crate::core::session_control_db::complete_incoming_message(control_db_path, message.id)
                .map_err(|error| JsString::from(error.to_string())),
            Err(error) => Err(error.message().to_owned()),
        };
        if let Err(error_message) = result {
            crate::core::session_control_db::fail_incoming_message(
                control_db_path,
                message.id,
                &(&error_message).clone(),
            )
            .map_err(|error| JsString::from(error.to_string()))?;
            self.show_error(&error_message);
        }
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
                ModeAction::Copy => {
                    let mode = Rc::clone(self);
                    tokio::task::spawn_local(async move {
                        mode.handle_copy_command().await;
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

    async fn submit(&self, text: JsString) {
        let submitted_text = text.clone();
        let text = text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.close_response_complete_notification();
        if text.starts_with('/') {
            self.add_submitted_text_to_history(&text);
        }

        if !text.starts_with('/')
            && !text.starts_with('!')
            && self.submit_selected_agent_steering(&text, &submitted_text).await
        {
            return;
        }

        if let Some(command) = text.strip_prefix('/') {
            let (name, args) = match command.find(' ') {
                Some(index) => (command.slice(0, index), command.slice(index + 1, command.len()).trim()),
                None => (command, JsString::default()),
            };
            let handled = self.handle_slash_command(&name, &args).await;
            if handled {
                self.clear_editor();
                return;
            }
        }

        if let Some(command) = text.strip_prefix('!') {
            let exclude_from_context = command.starts_with('!');
            let command = if exclude_from_context {
                command.slice(1, command.len()).trim()
            } else {
                command.trim()
            };
            if !command.is_empty() {
                if self.session.borrow().is_bash_running() {
                    self.show_warning("A bash command is already running. Press Esc to cancel it first.");
                    self.editor().borrow_mut().set_text(&text);
                    return;
                }
                self.add_submitted_text_to_history(&text);
                self.handle_bash_command(command.to_owned(), exclude_from_context).await;
                self.is_bash_mode.set(false);
                self.update_editor_border_color();
                return;
            }
        }

        if self.submit_pending_compaction_summary_edit(&text) {
            return;
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
                    .push(CompactionQueuedMessage {
                        text: text.into(),
                        mode: "steer",
                    });
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
                        on_user_message_prepared: Some(prepared_user_message_callback()),
                        ..PromptOptions::default()
                    },
                )
                .await
            {
                self.show_error(error.message());
            }
            if self.remove_provisional_user_messages() {
                self.ui.request_render();
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
                    on_user_message_prepared: Some(prepared_user_message_callback()),
                    ..PromptOptions::default()
                },
            )
            .await
        {
            self.show_error(error.message());
        } else {
            self.clipboard_temp_files.borrow_mut().cleanup_referenced_in(&text);
        }
        if self.remove_provisional_user_messages() {
            self.ui.request_render();
        }
    }

    fn is_extension_command(&self, text: &JsString) -> bool {
        let Some(command) = text.strip_prefix('/') else {
            return false;
        };
        let name = command
            .units()
            .split(|unit| pi_js_compat::string::is_js_whitespace_unit(*unit))
            .find(|part| !part.is_empty())
            .map(|part| JsString::from_utf16(part.to_vec()))
            .unwrap_or_default();
        self.session.borrow().extension_runner().get_command(name).is_some()
    }

    async fn handle_slash_command(&self, name: &JsString, args: &JsString) -> bool {
        match name {
            value if value == "help" || value == "hotkeys" => {
                self.show_status("Esc interrupt · Ctrl+C clear · Ctrl+D exit · Ctrl+O tools · Ctrl+T thinking");
            }
            value if value == "settings" => {
                self.show_settings_selector();
            }
            value if value == "scoped-models" => {
                self.show_models_selector().await;
            }
            value if value == "model" => {
                let search = (!args.is_empty()).then(|| args.to_owned());
                self.handle_model_command(search).await;
            }
            value if value == "thinking" => {
                self.show_thinking_selector();
            }
            value if value == "theme" => {
                self.show_theme_selector();
            }
            value if value == "export" => {
                self.handle_export_command(js_format!("/export {}", args)).await;
            }
            value if value == "import" => {
                self.handle_import_command(js_format!("/import {}", args)).await;
            }
            value if value == "share" => {
                self.handle_share_command().await;
            }
            value if value == "copy" => {
                self.handle_copy_command().await;
            }
            value if value == "cd" => {
                self.handle_cd_command(js_format!("/cd {}", args)).await;
            }
            value if value == "name" => self.handle_name_command(args),
            value if value == "unname" => self.handle_unname_command(),
            value if value == "session" => self.handle_session_command(),
            value if value == "tools" => self.handle_tools_command(),
            value if value == "extensions" => self.handle_extensions_command(),
            value if value == "changelog" => self.handle_changelog_command(),
            value if value == "fork" => self.show_user_message_selector(),
            value if value == "clone" => self.handle_clone_command().await,
            value if value == "tree" => self.show_tree_selector(None, None),
            value if value == "trust" => self.show_trust_selector(),
            value if value == "login" => self.handle_login_command(Some(args)).await,
            value if value == "logout" => self.show_oauth_selector(true).await,
            value if value == "new" => self.handle_clear_command().await,
            value if value == "continue" => {
                if self.session.borrow().is_streaming() {
                    self.cancel_streaming_and_submit_queued_messages().await;
                } else {
                    self.clear_editor();
                    if let Err(error) = self.session().continue_run().await {
                        self.show_error(error.message());
                    }
                }
            }
            value if value == "compact" => {
                let instructions = (!args.is_empty()).then(|| JsString::from(args));
                self.handle_compact_command(instructions).await;
            }
            value if value == "reload" => self.handle_reload_command().await,
            value if value == "resume" => self.show_session_selector(),
            value if value == "agents" => self.show_agent_switcher(),
            value if value == "quit" => self.shutdown_requested.set(true),
            value if value == "arminsayshi" => self.handle_armin_says_hi(),
            value if value == "dementedelves" => self.handle_demented_delves(),
            value if value == "daxnuts" => self.handle_daxnuts(),
            _ => {
                if self.session.borrow().extension_runner().get_command(name).is_some() {
                    let text = if args.is_empty() {
                        js_format!("/{}", name)
                    } else {
                        js_format!("/{} {}", name, args)
                    };
                    self.clear_editor();
                    if let Err(error) = self.session().prompt(&text, PromptOptions::default()).await {
                        self.show_error(error.message());
                    }
                    return true;
                }
                let templates = self.session().prompt_templates();
                if let Some(template) = templates.into_iter().find(|template| &template.name == name) {
                    let expanded = crate::core::prompt_templates::expand_prompt_template(
                        &js_format!("/{} {}", name, args),
                        &[template],
                    );
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
            with_mode(mode, |mode| {
                match mode.session().set_thinking_level(level) {
                    Ok(()) => {
                        mode.footer.borrow_mut().invalidate();
                        mode.update_editor_border_color();
                    }
                    Err(error) => mode.show_error(error.message()),
                }
                mode.restore_editor();
            });
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
            .settings(|settings| settings.get_theme().cloned())
            .unwrap_or_else(|| "dark".into());
        let mode = self.ui_id.get();
        let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |name| {
            with_mode(mode, |mode| {
                mode.set_theme_name(&name);
                mode.restore_editor();
            });
        });
        let mode = self.ui_id.get();
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode, |mode| mode.restore_editor());
        });
        let on_preview: Rc<dyn Fn(JsString)> = Rc::new(|_| {});
        let selector = crate::modes::interactive::components::theme_selector::ThemeSelectorComponent::new(
            &current, on_select, on_cancel, on_preview,
        );
        self.show_selector(component(selector));
    }

    fn steering_mode(&self) -> SteeringMode {
        match self.session().steering_mode() {
            pi_agent_core::harness::QueueMode::All => SteeringMode::All,
            pi_agent_core::harness::QueueMode::OneAtATime => SteeringMode::OneAtATime,
        }
    }

    fn follow_up_mode(&self) -> SteeringMode {
        match self.session().follow_up_mode() {
            pi_agent_core::harness::QueueMode::All => SteeringMode::All,
            pi_agent_core::harness::QueueMode::OneAtATime => SteeringMode::OneAtATime,
        }
    }

    fn rebuild_chat_from_messages(&self) {
        self.chat_container.borrow_mut().clear();
        self.render_initial_messages();
    }

    fn show_settings_selector(&self) {
        let session = self.session();
        let Ok(http_idle_timeout_ms) = self.settings(|settings| settings.get_http_idle_timeout_ms()) else {
            self.show_error("Invalid httpIdleTimeoutMs setting");
            return;
        };
        let config = SettingsConfig {
            auto_compact: session.auto_compaction_enabled(),
            show_images: self.settings(|settings| settings.get_show_images()),
            image_width_cells: self.settings(|settings| settings.get_image_width_cells()) as i64,
            auto_resize_images: self.settings(|settings| settings.get_image_auto_resize()),
            block_images: self.settings(|settings| settings.get_block_images()),
            enable_skill_commands: self.settings(|settings| settings.get_enable_skill_commands()),
            steering_mode: self.steering_mode(),
            follow_up_mode: self.follow_up_mode(),
            transport: self.settings(|settings| settings.get_transport()),
            http_idle_timeout_ms,
            thinking_level: session.thinking_level(),
            available_thinking_levels: session.get_available_thinking_levels(),
            current_theme: self
                .settings(|settings| settings.get_theme_setting().cloned())
                .unwrap_or_else(|| "dark".into()),
            terminal_theme: self.theme_controller.borrow().get_terminal_theme(),
            available_themes: match crate::modes::interactive::theme::get_available_themes() {
                Ok(themes) => themes,
                Err(error) => {
                    self.show_error(error.message());
                    return;
                }
            },
            hide_thinking_block: self.hide_thinking_block.get(),
            mermaid_rendering_mode: self.settings(|settings| settings.get_mermaid_rendering_mode()),
            show_cache_miss_notices: self.settings(|settings| settings.get_show_cache_miss_notices()),
            collapse_changelog: self.settings(|settings| settings.get_collapse_changelog()),
            enable_install_telemetry: self.settings(|settings| settings.get_enable_install_telemetry()),
            double_escape_action: self.settings(|settings| settings.get_double_escape_action()),
            tree_filter_mode: self.settings(|settings| settings.get_tree_filter_mode()),
            show_hardware_cursor: self.settings(|settings| settings.get_show_hardware_cursor()),
            editor_padding_x: self.settings(|settings| settings.get_editor_padding_x()) as i64,
            output_pad: self.settings(|settings| settings.get_output_pad()),
            autocomplete_max_visible: self.settings(|settings| settings.get_autocomplete_max_visible()) as i64,
            quiet_startup: self.settings(|settings| settings.get_quiet_startup()),
            default_project_trust: self.settings(|settings| settings.get_default_project_trust()),
            clear_on_shrink: self.settings(|settings| settings.get_clear_on_shrink()),
            show_terminal_progress: self.settings(|settings| settings.get_show_terminal_progress()),
            warnings: {
                let warnings = self.settings(|settings| settings.get_warnings());
                crate::core::settings_manager::WarningSettings {
                    anthropic_extra_usage: warnings.get("anthropicExtraUsage").and_then(JsValue::as_bool),
                }
            },
        };

        let mode_id = self.ui_id.get();

        macro_rules! callback {
            ($body:expr) => {
                Rc::new(move |value| {
                    with_mode(mode_id, |mode| $body(mode, value));
                })
            };
        }

        let callbacks = SettingsCallbacks {
            on_auto_compact_change: callback!(|mode: Rc<InteractiveMode>, enabled: bool| {
                mode.session().set_auto_compaction_enabled(enabled);
                mode.footer.borrow_mut().set_auto_compact_enabled(enabled);
            }),
            on_show_images_change: callback!(|mode: Rc<InteractiveMode>, enabled: bool| {
                mode.settings_mut(|settings| settings.set_show_images(enabled));
                for child in mode.chat_container.borrow().children().to_vec() {
                    if let Some(component) = child
                        .borrow_mut()
                        .as_any_mut()
                        .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                    {
                        component.set_show_images(enabled);
                    }
                }
            }),
            on_image_width_cells_change: callback!(|mode: Rc<InteractiveMode>, width: i64| {
                mode.settings_mut(|settings| settings.set_image_width_cells(width as f64));
                for child in mode.chat_container.borrow().children().to_vec() {
                    if let Some(component) = child
                        .borrow_mut()
                        .as_any_mut()
                        .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                    {
                        component.set_image_width_cells(width.max(0) as usize);
                    }
                }
            }),
            on_auto_resize_images_change: callback!(|mode: Rc<InteractiveMode>, enabled: bool| {
                mode.settings_mut(|settings| settings.set_image_auto_resize(enabled));
            }),
            on_block_images_change: callback!(|mode: Rc<InteractiveMode>, blocked: bool| {
                mode.settings_mut(|settings| settings.set_block_images(blocked));
            }),
            on_enable_skill_commands_change: callback!(|mode: Rc<InteractiveMode>, enabled: bool| {
                mode.settings_mut(|settings| settings.set_enable_skill_commands(enabled));
                mode.setup_autocomplete_provider();
            }),
            on_steering_mode_change: callback!(|mode: Rc<InteractiveMode>, steering: SteeringMode| {
                mode.session().set_steering_mode(match steering {
                    SteeringMode::All => pi_agent_core::harness::QueueMode::All,
                    SteeringMode::OneAtATime => pi_agent_core::harness::QueueMode::OneAtATime,
                });
            }),
            on_follow_up_mode_change: callback!(|mode: Rc<InteractiveMode>, follow_up: SteeringMode| {
                mode.session().set_follow_up_mode(match follow_up {
                    SteeringMode::All => pi_agent_core::harness::QueueMode::All,
                    SteeringMode::OneAtATime => pi_agent_core::harness::QueueMode::OneAtATime,
                });
            }),
            on_transport_change: callback!(|mode: Rc<InteractiveMode>, transport: pi_ai::types::Transport| {
                mode.settings_mut(|settings| settings.set_transport(transport));
                mode.session().agent().set_transport(transport);
            }),
            on_http_idle_timeout_ms_change: callback!(|mode: Rc<InteractiveMode>, timeout_ms: u64| {
                if let Err(error) = mode.settings_mut(|settings| settings.set_http_idle_timeout_ms(timeout_ms as f64)) {
                    mode.show_error(error.message());
                    return;
                }
                let _ = crate::core::http_dispatcher::configure_http_dispatcher(timeout_ms as f64);
                mode.show_status(&js_format!(
                    "HTTP idle timeout: {}",
                    crate::core::http_dispatcher::format_http_idle_timeout_ms(timeout_ms)
                ));
            }),
            on_thinking_level_change: callback!(
                |mode: Rc<InteractiveMode>, level: pi_agent_core::harness::ThinkingLevel| {
                    match mode.session().set_thinking_level(level) {
                        Ok(()) => {
                            mode.footer.borrow_mut().invalidate();
                            mode.update_editor_border_color();
                        }
                        Err(error) => mode.show_error(error.message()),
                    }
                }
            ),
            on_theme_change: callback!(|mode: Rc<InteractiveMode>, theme_setting: &JsString| {
                mode.settings_mut(|settings| settings.set_theme(theme_setting));
                mode.theme_controller.borrow_mut().apply_from_settings();
            }),
            on_theme_preview: Some(Rc::new({
                move |theme_name: &JsString| {
                    with_mode(mode_id, |mode| {
                        mode.theme_controller.borrow_mut().preview(theme_name);
                    });
                }
            })),
            on_hide_thinking_block_change: callback!(|mode: Rc<InteractiveMode>, hidden: bool| {
                mode.hide_thinking_block.set(hidden);
                mode.settings_mut(|settings| settings.set_hide_thinking_block(hidden));
                for child in mode.chat_container.borrow().children().to_vec() {
                    if let Some(component) = child
                        .borrow_mut()
                        .as_any_mut()
                        .and_then(|any| any.downcast_mut::<AssistantMessageComponent>())
                    {
                        component.set_hide_thinking_block(hidden);
                    }
                }
                mode.rebuild_chat_from_messages();
            }),
            on_mermaid_rendering_mode_change: callback!(
                |mode: Rc<InteractiveMode>, mermaid: crate::core::settings_manager::MermaidRenderingMode| {
                    mode.settings_mut(|settings| settings.set_mermaid_rendering_mode(mermaid));
                    mode.chat_container.borrow_mut().invalidate();
                    mode.ui.request_render();
                }
            ),
            on_show_cache_miss_notices_change: callback!(|mode: Rc<InteractiveMode>, shown: bool| {
                mode.settings_mut(|settings| settings.set_show_cache_miss_notices(shown));
                mode.rebuild_chat_from_messages();
            }),
            on_collapse_changelog_change: callback!(|mode: Rc<InteractiveMode>, collapsed: bool| {
                mode.settings_mut(|settings| settings.set_collapse_changelog(collapsed));
            }),
            on_enable_install_telemetry_change: callback!(|mode: Rc<InteractiveMode>, enabled: bool| {
                mode.settings_mut(|settings| settings.set_enable_install_telemetry(enabled));
            }),
            on_quiet_startup_change: callback!(|mode: Rc<InteractiveMode>, enabled: bool| {
                mode.settings_mut(|settings| settings.set_quiet_startup(enabled));
            }),
            on_default_project_trust_change: callback!(
                |mode: Rc<InteractiveMode>, trust: crate::core::settings_manager::DefaultProjectTrust| {
                    mode.settings_mut(|settings| settings.set_default_project_trust(trust));
                }
            ),
            on_double_escape_action_change: callback!(
                |mode: Rc<InteractiveMode>, action: crate::core::settings_manager::DoubleEscapeAction| {
                    mode.settings_mut(|settings| settings.set_double_escape_action(action));
                }
            ),
            on_tree_filter_mode_change: callback!(
                |mode: Rc<InteractiveMode>, filter: crate::core::settings_manager::TreeFilterMode| {
                    mode.settings_mut(|settings| settings.set_tree_filter_mode(filter));
                }
            ),
            on_show_hardware_cursor_change: callback!(|mode: Rc<InteractiveMode>, enabled: bool| {
                mode.settings_mut(|settings| settings.set_show_hardware_cursor(enabled));
                mode.ui.set_show_hardware_cursor(enabled);
            }),
            on_editor_padding_x_change: callback!(|mode: Rc<InteractiveMode>, padding: i64| {
                mode.settings_mut(|settings| settings.set_editor_padding_x(padding as f64));
                mode.default_editor.borrow_mut().set_padding_x(padding.max(0) as usize);
                let current = mode.editor();
                let default: Rc<RefCell<dyn EditorComponent>> = mode.default_editor.clone();
                if !Rc::ptr_eq(&current, &default) {
                    current.borrow_mut().set_padding_x(padding.max(0) as usize);
                }
            }),
            on_output_pad_change: callback!(|mode: Rc<InteractiveMode>, padding: u8| {
                mode.settings_mut(|settings| settings.set_output_pad(padding));
                let streaming = mode.streaming_component.borrow().is_some() || mode.session().is_streaming();
                if streaming {
                    for child in mode.chat_container.borrow().children().to_vec() {
                        if let Some(component) = child
                            .borrow_mut()
                            .as_any_mut()
                            .and_then(|any| any.downcast_mut::<AssistantMessageComponent>())
                        {
                            component.set_output_pad(padding as usize);
                            continue;
                        }
                        if let Some(component) = child
                            .borrow_mut()
                            .as_any_mut()
                            .and_then(|any| any.downcast_mut::<UserMessageComponent>())
                        {
                            component.set_output_pad(padding as usize);
                        }
                    }
                    mode.ui.request_render();
                    return;
                }
                mode.rebuild_chat_from_messages();
            }),
            on_autocomplete_max_visible_change: callback!(|mode: Rc<InteractiveMode>, max_visible: i64| {
                mode.settings_mut(|settings| settings.set_autocomplete_max_visible(max_visible as f64));
                mode.default_editor
                    .borrow_mut()
                    .set_autocomplete_max_visible(max_visible.max(0) as usize);
                let current = mode.editor();
                let default: Rc<RefCell<dyn EditorComponent>> = mode.default_editor.clone();
                if !Rc::ptr_eq(&current, &default) {
                    current
                        .borrow_mut()
                        .set_autocomplete_max_visible(max_visible.max(0) as usize);
                }
            }),
            on_clear_on_shrink_change: callback!(|mode: Rc<InteractiveMode>, enabled: bool| {
                mode.settings_mut(|settings| settings.set_clear_on_shrink(enabled));
                mode.ui.set_clear_on_shrink(enabled);
            }),
            on_show_terminal_progress_change: callback!(|mode: Rc<InteractiveMode>, enabled: bool| {
                mode.settings_mut(|settings| settings.set_show_terminal_progress(enabled));
            }),
            on_warnings_change: callback!(
                |mode: Rc<InteractiveMode>, warnings: crate::core::settings_manager::WarningSettings| {
                    let mut value = JsObject::new();
                    if let Some(anthropic) = warnings.anthropic_extra_usage {
                        value.insert(JsString::from("anthropicExtraUsage"), JsValue::Bool(anthropic));
                    }
                    mode.settings_mut(|settings| settings.set_warnings(&value));
                }
            ),
            on_cancel: {
                Rc::new(move || {
                    with_mode(mode_id, |mode| {
                        mode.restore_editor();
                        mode.ui.request_render();
                    });
                })
            },
        };
        self.show_selector(component(SettingsSelectorComponent::new(config, Rc::new(callbacks))));
    }

    fn show_agent_switcher(&self) {
        let Some(store) = self.multi_agent_store.as_ref() else {
            self.show_status("Agent switcher is unavailable: no multi-agent store.");
            return;
        };
        let agents = store.list_agents();
        if agents.is_empty() {
            self.show_status("No agents to show.");
            return;
        }
        let selected = store.get_selected_agent_id();
        let mode = self.ui_id.get();
        let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |agent_id| {
            with_mode(mode, |mode| {
                mode.select_agent_view_from_bridge(&agent_id);
                mode.restore_editor();
            });
        });
        let mode = self.ui_id.get();
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode, |mode| mode.restore_editor());
        });
        let mode = self.ui_id.get();
        let on_inactive: Rc<dyn Fn(AgentSnapshot)> = Rc::new(move |agent| {
            with_mode(mode, |mode| {
                mode.show_status(&js_format!(
                    "{} is {}",
                    agent.display_name,
                    lifecycle_name(agent.lifecycle)
                ));
                mode.restore_editor();
            });
        });
        let switcher = AgentSwitcherComponent::new(agents, selected, on_select, on_cancel, on_inactive);
        self.show_selector(component(switcher));
    }

    fn show_selector(&self, selector: ComponentRef) {
        let focus = Rc::clone(&selector);
        self.show_selector_with_focus(selector, focus);
    }

    fn show_selector_with_focus(&self, selector: ComponentRef, focus: ComponentRef) {
        self.editor_container.borrow_mut().clear();
        self.editor_container.borrow_mut().add_child(Rc::clone(&selector));
        self.ui.set_focus(Some(focus));
        self.ui.request_render();
    }

    fn restore_editor(&self) {
        self.editor_container.borrow_mut().clear();
        self.editor_container
            .borrow_mut()
            .add_child(Rc::clone(&self.editor_handle.borrow()));
        let editor_ref = Rc::clone(&self.editor_handle.borrow());
        self.ui.set_focus(Some(editor_ref));
        self.ui.request_render();
    }

    fn update_available_provider_count(&self) {
        let session = self.session();
        let scoped = session.scoped_models();
        let models: Vec<Model> = if scoped.is_empty() {
            session.model_runtime().get_available_snapshot()
        } else {
            scoped.into_iter().map(|scoped| scoped.model).collect()
        };
        let unique_providers: HashSet<&str> = models.iter().map(|model| model.provider.as_str()).collect();
        self.footer_data_provider
            .set_available_provider_count(unique_providers.len());
    }

    fn persist_enabled_models_for_selector(&self, enabled_ids: EnabledIds, available_model_count: usize) {
        let enabled_patterns = match enabled_ids {
            None => None,
            Some(ids) if ids.len() == available_model_count => None,
            Some(ids) => Some(ids),
        };
        let project_enabled_models =
            self.settings(|settings| settings.get_project_settings().get("enabledModels").cloned());
        let (patterns, scope) =
            resolve_enabled_models_persist_target(project_enabled_models.as_ref(), enabled_patterns);
        if let Err(error) = self.settings_mut(|settings| settings.set_enabled_models(patterns.as_deref(), scope)) {
            self.show_error(error.message());
            return;
        }
        self.show_status("Model selection saved to settings");
    }

    async fn show_models_selector(&self) {
        let session = self.session();
        let runtime = session.model_runtime().clone();
        if let Err(error) = runtime.reload_config().await {
            self.show_error(error.message());
            return;
        }
        let available_models = match runtime
            .get_available(None, pi_ai::auth::types::AuthOperationOptions::new(None))
            .await
        {
            Ok(models) => models,
            Err(error) => {
                self.show_error(error.message());
                return;
            }
        };
        if available_models.is_empty() {
            self.show_status("No models available");
            return;
        }

        let session_scoped_models = session.scoped_models();
        let current_enabled_ids = if !session_scoped_models.is_empty() {
            Some(
                session_scoped_models
                    .iter()
                    .map(|scoped| js_format!("{}/{}", scoped.model.provider, scoped.model.id))
                    .collect(),
            )
        } else {
            self.settings(|settings| settings.get_enabled_models())
                .filter(|patterns| !patterns.is_empty())
                .map(|patterns| {
                    resolve_model_scope_from_models(&patterns, &available_models)
                        .scoped_models
                        .iter()
                        .map(|scoped| js_format!("{}/{}", scoped.model.provider, scoped.model.id))
                        .collect()
                })
        };
        let mode_id = self.ui_id.get();
        let available_model_count = available_models.len();
        let callback_models = available_models.clone();
        let callbacks = ModelsCallbacks {
            on_change: Rc::new(move |enabled_ids: EnabledIds| {
                let scoped_models = match enabled_ids {
                    Some(ids) if !ids.is_empty() && ids.len() < available_model_count => {
                        resolve_model_scope_from_models(&ids, &callback_models).scoped_models
                    }
                    _ => Vec::new(),
                };
                session.set_scoped_models(scoped_models);
                with_mode(mode_id, |mode| {
                    mode.update_available_provider_count();
                    mode.ui.request_render();
                });
            }),
            on_persist: Rc::new(move |enabled_ids| {
                with_mode(mode_id, |mode| {
                    mode.persist_enabled_models_for_selector(enabled_ids, available_model_count);
                });
            }),
            on_cancel: Rc::new(move || {
                with_mode(mode_id, |mode| {
                    mode.restore_editor();
                    mode.ui.request_render();
                });
            }),
        };
        let selector = ScopedModelsSelectorComponent::new(
            ModelsConfig {
                all_models: available_models,
                enabled_model_ids: current_enabled_ids,
            },
            callbacks,
        );
        self.show_selector(component(selector));
    }

    async fn handle_model_command(&self, search_term: Option<JsString>) {
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
                    JsString::from(&model.id).to_lowercase().contains(&needle)
                        || JsString::from(&model.provider).to_lowercase().contains(&needle)
                        || js_format!("{}/{}", model.provider, model.id)
                            .to_lowercase()
                            .contains(&needle)
                })
                .collect();
            if matches.len() == 1 {
                let model = matches[0].clone();
                match session.set_model(model.clone()).await {
                    Ok(()) => {
                        self.footer.borrow_mut().invalidate();
                        self.update_editor_border_color();
                        self.show_status(&js_format!("Model: {}", model.id));
                        self.maybe_warn_about_anthropic_subscription_auth(Some(model.clone()))
                            .await;
                        self.check_daxnuts_easter_egg(&model);
                    }
                    Err(error) => self.show_error(error.message()),
                }
                return;
            }
        }
        let labels: Vec<JsString> = models
            .iter()
            .map(|model| js_format!("{}/{}", model.provider, model.id))
            .collect();
        let by_label: HashMap<JsString, Model> = models
            .iter()
            .map(|model| (js_format!("{}/{}", model.provider, model.id), model.clone()))
            .collect();
        let mode = self.ui_id.get();
        let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |label| {
            let model = by_label.get(&label).cloned();
            if let Some(mode) = with_mode(mode, |mode| mode) {
                mode.restore_editor();
                if let Some(model) = model {
                    let mode = Rc::clone(&mode);
                    tokio::task::spawn_local(async move {
                        match mode.session().set_model(model.clone()).await {
                            Ok(()) => {
                                mode.footer.borrow_mut().invalidate();
                                mode.update_editor_border_color();
                                mode.show_status(&js_format!("Model: {}", model.id));
                                mode.maybe_warn_about_anthropic_subscription_auth(Some(model.clone()))
                                    .await;
                                mode.check_daxnuts_easter_egg(&model);
                            }
                            Err(error) => mode.show_error(error.message()),
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
            &JsString::from("Select model"),
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
        match self.session().cycle_model(direction).await {
            Ok(Some(result)) => {
                self.footer.borrow_mut().invalidate();
                self.update_editor_border_color();
                let thinking = if result.model.reasoning
                    && result.thinking_level != pi_agent_core::harness::types::ThinkingLevel::Off
                {
                    js_format!(" (thinking: {})", result.thinking_level.as_str())
                } else {
                    JsString::default()
                };
                let name = if result.model.name.is_empty() {
                    result.model.id.clone()
                } else {
                    result.model.name.clone()
                };
                self.show_status(&js_format!("Switched to {}{}", name, thinking));
                self.maybe_warn_about_anthropic_subscription_auth(Some(result.model))
                    .await;
            }
            Ok(None) => {
                let message = if self.session().scoped_models().is_empty() {
                    "Only one model available"
                } else {
                    "Only one model in scope"
                };
                self.show_status(message);
            }
            Err(error) => self.show_error(error.message()),
        }
        self.ui.request_render();
    }

    async fn handle_open_external_editor(&self) {
        let text = self.editor().borrow().get_text();
        let command = self.settings(|settings| settings.get_external_editor_command());
        let result = crate::modes::interactive::external_editor::edit_in_external_editor(
            &crate::modes::interactive::external_editor::ExternalEditorOptions { command, content: text },
        )
        .unwrap_or_else(|error| crate::utils::uncaught::throw_uncaught(crate::utils::uncaught::ERROR, error));
        match result {
            crate::modes::interactive::external_editor::ExternalEditorResult::Complete { content } => {
                self.editor().borrow_mut().set_text(&content);
                self.ui.request_render();
            }
            crate::modes::interactive::external_editor::ExternalEditorResult::Failed => {
                self.show_error("External editor failed");
            }
        }
    }

    fn handle_follow_up(&self) {
        let text = self.editor().borrow().get_text().trim().to_owned();
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
            self.show_status(&js_format!("Restored {} queued message(s)", restored));
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
        let mut text = self.editor().borrow().get_text();
        for message in steering.into_iter().chain(follow_up) {
            if !text.is_empty() {
                text.push_str("\n");
            }
            text.push_str(&(&message).clone());
        }
        self.editor().borrow_mut().set_text(&text);
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

    async fn handle_bash_command(&self, command: JsString, exclude_from_context: bool) {
        let command = JsString::from(command);
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
        self.active_bash_components.borrow_mut().push(Rc::clone(&bash_ref));
        self.pending_messages_container
            .borrow_mut()
            .add_child(Rc::clone(&bash_ref));
        self.set_prompt_activity(PromptActivitySource::Bash, true);
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
                                content: JsString::default(),
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
                self.chat_container.borrow_mut().add_child(Rc::clone(&bash_ref));
                self.footer.borrow_mut().invalidate();
            }
            Err(error) => {
                self.pending_messages_container.borrow_mut().clear();
                self.show_error(error.message());
            }
        }
        self.active_bash_components
            .borrow_mut()
            .retain(|component| !Rc::ptr_eq(component, &bash_ref));
        if let Some(mode) = with_mode(mode, |mode| mode) {
            let _ = mode;
        }
        self.set_prompt_activity(PromptActivitySource::Bash, false);
        self.ui.request_render();
    }

    async fn handle_compact_command(&self, custom_instructions: Option<JsString>) {
        let session = self.session();
        let result = session.compact(custom_instructions.as_ref()).await;
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
            Err(error) => self.handle_fatal_runtime_error(&JsString::from("Failed to create session"), error),
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

    /// Adopts the runtime's replacement session unless the transition was cancelled.
    fn adopt_transitioned_session(&self, runtime: &AgentSessionRuntime, cancelled: bool) {
        if !cancelled {
            self.adopt_session(runtime.session().clone());
        }
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

    async fn handle_export_command(&self, text: JsString) {
        let path = Self::path_command_argument(&text, &JsString::from("/export"));
        match self.session().export_to_html(path.as_ref()).await {
            Ok(path) => self.show_status(&js_format!("Exported session to {}", path)),
            Err(error) => self.show_error(error.message()),
        }
    }

    async fn handle_import_command(&self, text: JsString) {
        let Some(path) = Self::path_command_argument(&text, &JsString::from("/import")) else {
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
            Err(error) => {
                if error.starts_with(crate::core::session_errors::SessionImportFileNotFoundError::MESSAGE_PREFIX) {
                    self.show_error(&js_format!("Failed to import session: {}", error));
                } else {
                    self.handle_fatal_runtime_error(&JsString::from("Failed to import session"), error);
                }
            }
        }
    }

    fn path_command_argument(text: &JsString, command: &JsString) -> Option<JsString> {
        let rest = text.strip_prefix(command)?.trim();
        if rest.is_empty() { None } else { Some(rest.to_owned()) }
    }

    async fn handle_share_command(&self) {
        self.handle_share_command_with_program(&JsString::from("gh")).await;
    }

    /// Share flow with an explicit `gh` program; production passes `"gh"`.
    async fn handle_share_command_with_program(&self, gh_program: &JsString) {
        let auth = std::process::Command::new(encode_node_utf8(gh_program))
            .args(["auth", "status"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match auth {
            Err(_) => {
                self.show_error("GitHub CLI (gh) is not installed. Install it from https://cli.github.com/");
                return;
            }
            Ok(status) if !status.success() => {
                self.show_error("GitHub CLI is not logged in. Run 'gh auth login' first.");
                return;
            }
            Ok(_) => {}
        }

        let tmp_file = std::env::temp_dir().join("session.html");
        let tmp_path = tmp_file.display().to_string();
        if let Err(error) = self.session().export_to_html(Some(&JsString::from(&tmp_path))).await {
            self.show_error(&(JsString::from("Failed to export session: ") + error.message()));
            return;
        }

        let request_render: Rc<dyn Fn()> = Rc::new({
            let ui = Rc::clone(&self.ui);
            move || ui.request_render()
        });
        let loader = Rc::new(RefCell::new(BorderedLoader::new(
            request_render,
            "Creating gist...",
            BorderedLoaderOptions::default(),
        )));
        let signal = loader.borrow().signal();
        self.editor_container.borrow_mut().clear();
        let loader_component: ComponentRef = component(SharedBorderedLoader(Rc::clone(&loader)));
        self.editor_container
            .borrow_mut()
            .add_child(Rc::clone(&loader_component));
        self.ui.set_focus(Some(loader_component));
        self.ui.request_render();

        let pid = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let mode_id = self.ui_id.get();
        let abort_pid = Arc::clone(&pid);
        let abort_loader = Rc::downgrade(&loader);
        let abort_tmp = tmp_path.clone();
        loader.borrow_mut().set_on_abort(Some(Rc::new(move || {
            let child_pid = abort_pid.swap(0, std::sync::atomic::Ordering::SeqCst);
            if child_pid != 0
                && let Some(child_pid) = rustix::process::Pid::from_raw(child_pid as i32)
            {
                let _ = rustix::process::kill_process(child_pid, rustix::process::Signal::KILL);
            }
            if let Some(loader) = abort_loader.upgrade() {
                loader.borrow_mut().dispose();
            }
            let _ = std::fs::remove_file(&abort_tmp);
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                mode.show_status("Share cancelled");
            });
        })));

        let run = match run_gh_gist(gh_program, &JsString::from(&tmp_path), &pid).await {
            Ok(run) => run,
            Err(message) => {
                if !signal.is_cancelled() {
                    self.finish_share_loader(&loader, &JsString::from(&tmp_path));
                    self.show_error(&js_format!("Failed to create gist: {}", message));
                }
                return;
            }
        };
        if signal.is_cancelled() {
            return;
        }
        self.finish_share_loader(&loader, &JsString::from(&tmp_path));

        if !run.status.success() {
            let stderr = run.stderr.trim();
            let error_message = if stderr.is_empty() {
                "Unknown error".into()
            } else {
                stderr
            };
            self.show_error(&js_format!("Failed to create gist: {}", error_message));
            return;
        }

        let stdout = run.stdout;
        let gist_url = stdout.trim().to_owned();
        let gist_id = gist_url.split('/').last().filter(|id| !id.is_empty());
        let Some(gist_id) = gist_id else {
            self.show_error("Failed to parse gist ID from gh output");
            return;
        };
        let preview_url = crate::config::get_share_viewer_url(&encode_node_utf8(&gist_id));
        self.show_status(&js_format!("Share URL: {}\nGist: {}", preview_url, gist_url));
    }

    fn finish_share_loader(&self, loader: &Rc<RefCell<BorderedLoader>>, tmp_path: &JsString) {
        self.restore_editor();
        loader.borrow_mut().dispose();
        let _ = std::fs::remove_file(encode_node_utf8(tmp_path));
    }

    async fn handle_copy_command(&self) {
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
                self.show_status("Copied last agent message to clipboard");
            }
            Err(error) => self.show_error(&js_format!("Failed to copy: {}", error.message())),
        }
    }

    fn handle_name_command(&self, args: &JsString) {
        let name = args.trim();
        if name.is_empty() {
            self.show_error("Usage: /name <name>");
            return;
        }
        if let Err(error) = self.session.borrow().set_session_name(&JsString::from(name)) {
            self.show_error(&(JsString::from("Failed to set session name: ") + error.message()));
            return;
        }
        self.update_terminal_title();
        self.ui.request_render();
    }

    fn handle_unname_command(&self) {
        if let Err(error) = self.session.borrow().clear_session_name() {
            self.show_error(&(JsString::from("Failed to clear session name: ") + error.message()));
            return;
        }
        self.update_terminal_title();
        self.ui.request_render();
    }

    fn handle_session_command(&self) {
        let stats = self.session.borrow().get_session_stats();
        let mut info = js_format!(
            "Session: {}\nFile: {}\nMessages: {}\nUser: {}\nAssistant: {}\nTool calls: {}\nTool results: {}",
            stats.session_id,
            stats
                .session_file
                .as_ref()
                .unwrap_or(&JsString::from("(not persisted)")),
            stats.total_messages,
            stats.user_messages,
            stats.assistant_messages,
            stats.tool_calls,
            stats.tool_results
        );
        info.push_str("\n");
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
        let lines: Vec<JsString> = tools
            .iter()
            .map(|tool| {
                let marker = if active.contains(&tool.name) { "●" } else { "○" };
                js_format!("{} {}", marker, tool.name)
            })
            .collect();
        self.show_status(&js_format!("Tools:\n{}", JsString::join(lines, "\n")));
    }

    fn handle_extensions_command(&self) {
        let extensions = self.session.borrow().resource_loader().get_extensions();
        let lines: Vec<JsString> = extensions
            .extensions
            .iter()
            .map(|extension| extension.read().path.clone())
            .collect();
        self.show_status(&js_format!("Extensions:\n{}", JsString::join(lines, "\n")));
    }

    fn get_changelog_for_display(&self) -> Option<JsString> {
        let last_version = self.settings(|settings| settings.get_last_changelog_version().cloned());
        let changelog_path = crate::config::get_changelog_path();
        let version = self.version.clone();
        let display = changelog_display_for(
            last_version.as_ref(),
            &changelog_path,
            !self.session().messages().is_empty(),
        );
        if display.update_version {
            self.settings_mut(|settings| settings.set_last_changelog_version(&version));
            self.report_install_telemetry(&version);
        }
        display.markdown
    }

    fn report_install_telemetry(&self, version: &JsString) {
        if std::env::var_os("PI_OFFLINE").is_some() {
            return;
        }
        if !crate::core::telemetry::is_install_telemetry_enabled(&self.session.borrow().settings_manager(), None) {
            return;
        }
        let Some(fetch) = self.http_fetch.borrow().clone() else {
            return;
        };
        let url = js_format!(
            "https://pi.dev/api/report-install?version={}",
            pi_js_compat::string::encode_uri_component(version)
                .unwrap_or_else(|error| throw_uncaught("URIError", error.to_string()))
        );
        let user_agent = crate::utils::pi_user_agent::get_pi_user_agent(&version.into());
        tokio::task::spawn_local(async move {
            let abort = tokio_util::sync::CancellationToken::new();
            let timeout = abort.clone();
            let timeout_task = tokio::task::spawn_local(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                timeout.cancel();
            });
            let request = HttpRequest {
                method: "GET".into(),
                url: encode_node_utf8(&url),
                headers: vec![("User-Agent".into(), encode_node_utf8(&user_agent))],
                body: None,
            };
            let _ = fetch.fetch(request, Some(abort)).await;
            timeout_task.abort();
        });
    }

    fn show_startup_notices_if_needed(&self) {
        if self.startup_notices_shown.get() {
            return;
        }
        self.startup_notices_shown.set(true);

        let Some(markdown) = self.changelog_markdown.borrow().clone() else {
            return;
        };
        if !self.chat_container.borrow().children().is_empty() {
            self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
        }
        let border_color = Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Border, text)));
        self.chat_container
            .borrow_mut()
            .add_child(component(pi_tui::components::dynamic_border::DynamicBorder::new(
                border_color.clone(),
            )));
        if self.settings(|settings| settings.get_collapse_changelog()) {
            let version_match = Regex::new(r"##\s+\[?(\d+\.\d+\.\d+)\]?")
                .expect("changelog version regex")
                .captures(&markdown)
                .and_then(|captures| captures.get(1).map(|value| value.text()));
            let latest_version = version_match.unwrap_or_else(|| self.version.clone());
            let condensed = js_format!(
                "Updated to v{}. Use {} to view full changelog.",
                latest_version,
                with_theme(|theme| theme.bold("/changelog"))
            );
            self.chat_container
                .borrow_mut()
                .add_child(component(Text::new(condensed, 1, 0)));
        } else {
            self.chat_container.borrow_mut().add_child(component(Text::new(
                with_theme(|theme| theme.bold(&theme.fg(ThemeColor::Accent, "What's New"))),
                1,
                0,
            )));
            self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
            self.chat_container.borrow_mut().add_child(component(
                pi_tui::components::markdown::Markdown::with_shared_theme(
                    markdown.trim().to_owned(),
                    1,
                    0,
                    self.markdown_theme(),
                    None,
                    pi_tui::components::markdown::MarkdownOptions::default(),
                ),
            ));
            self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
        }
        self.chat_container
            .borrow_mut()
            .add_child(component(pi_tui::components::dynamic_border::DynamicBorder::new(
                border_color,
            )));
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

    async fn handle_cd_command(&self, text: JsString) {
        let Some(path) = Self::path_command_argument(&text, &JsString::from("/cd")) else {
            self.show_error("Usage: /cd <path>");
            return;
        };
        let mut runtime = self.runtime.lock().await;
        match runtime.relocate(&path).await {
            Ok(()) => {
                let session = runtime.session().clone();
                drop(runtime);
                self.adopt_session(session);
                self.show_status(&js_format!("Changed directory to {}", path));
            }
            Err(error) => self.show_error(&error),
        }
    }

    async fn handle_clone_command(&self) {
        let leaf_id = self.session.borrow().session_manager().get_leaf_id().cloned();
        let Some(leaf_id) = leaf_id else {
            self.show_status("Nothing to clone yet");
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
                self.editor().borrow_mut().set_text(&JsString::from(""));
                self.show_status("Cloned to new session");
            }
            Ok(_) => {
                drop(runtime);
                self.ui.request_render();
            }
            Err(error) => self.show_error(&error),
        }
    }

    fn show_user_message_selector(&self) {
        let session = self.session();
        let user_messages = session.get_user_messages_for_forking();
        if user_messages.is_empty() {
            self.show_status("No messages to fork from");
            return;
        }
        let initial_selected_id = user_messages.last().map(|message| message.entry_id.clone());
        let messages: Vec<UserMessageItem> = user_messages
            .into_iter()
            .map(|message| UserMessageItem {
                id: message.entry_id,
                text: message.text,
            })
            .collect();
        let mode_id = self.ui_id.get();
        let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |entry_id: JsString| {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                tokio::task::spawn_local(async move {
                    mode.fork_from_entry(&entry_id).await;
                });
            });
        });
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                mode.ui.request_render();
            });
        });
        let selector = UserMessageSelectorComponent::new(messages, on_select, on_cancel, initial_selected_id.as_ref());
        let focus = selector.get_message_list();
        self.show_selector_with_focus(component(selector), focus);
    }

    async fn fork_from_entry(&self, entry_id: &JsString) {
        let mut runtime = self.runtime.lock().await;
        match runtime
            .fork(entry_id, crate::core::agent_session_runtime::ForkPosition::Before, None)
            .await
        {
            Ok(result) if !result.cancelled => {
                let session = runtime.session().clone();
                drop(runtime);
                self.adopt_session(session);
                self.editor()
                    .borrow_mut()
                    .set_text(&result.selected_text.unwrap_or_default());
                self.show_status("Forked to new session");
            }
            Ok(_) => {
                drop(runtime);
                self.ui.request_render();
            }
            Err(error) => self.show_error(&error),
        }
    }

    fn show_tree_selector(&self, initial_selected_id: Option<JsString>, filter_mode: Option<TreeFilterMode>) {
        let session = self.session();
        let (tree, real_leaf_id) = {
            let manager = session.session_manager();
            (manager.get_tree(), manager.get_leaf_id().cloned())
        };
        let initial_filter_mode =
            filter_mode.unwrap_or_else(|| self.settings(|settings| settings.get_tree_filter_mode()));
        if tree.is_empty() {
            self.show_status("No entries in session");
            return;
        }

        let mode_id = self.ui_id.get();
        let leaf_for_select = real_leaf_id.clone();
        let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |entry_id: JsString| {
            let leaf = leaf_for_select.clone();
            with_mode(mode_id, |mode| {
                tokio::task::spawn_local(async move {
                    mode.handle_tree_selection(entry_id, leaf, initial_filter_mode).await;
                });
            });
        });
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                mode.ui.request_render();
            });
        });
        let on_label_change: crate::modes::interactive::components::tree_selector::LabelEditHandler =
            Rc::new(move |entry_id: &JsString, label: Option<&JsString>| {
                with_mode(mode_id, |mode| {
                    if let Err(error) = mode.session().session_manager().append_label_change(entry_id, label) {
                        mode.show_error(error.message());
                    }
                    mode.ui.request_render();
                });
            });
        let mut selector = TreeSelectorComponent::new(
            tree,
            real_leaf_id,
            self.ui.terminal().borrow().rows(),
            on_select,
            on_cancel,
            Some(on_label_change),
            initial_selected_id,
            Some(tree_filter_mode(initial_filter_mode)),
        );
        selector.set_on_copy(Some(Rc::new(move |text: Option<JsString>| {
            with_mode(mode_id, |mode| {
                tokio::task::spawn_local(async move {
                    mode.copy_tree_selection(text).await;
                });
            });
        })));
        self.show_selector(component(selector));
    }

    async fn copy_tree_selection(&self, text: Option<JsString>) {
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            self.show_error("Selected entry has no text to copy");
            return;
        };
        match crate::utils::clipboard::copy_to_clipboard(
            &text,
            &crate::utils::clipboard_image::ClipboardEnvironment::from_process(),
            None,
        )
        .await
        {
            Ok(()) => self.show_status("Copied selected message to clipboard"),
            Err(error) => self.show_error(error.message()),
        }
    }

    async fn handle_tree_selection(
        &self,
        entry_id: JsString,
        real_leaf_id: Option<JsString>,
        initial_filter_mode: TreeFilterMode,
    ) {
        let selected_entry = self.session().session_manager().get_entry(&entry_id).cloned();
        if let Some(entry) = selected_entry
            .as_ref()
            .and_then(SessionEntry::as_compaction)
            .map(|entry| entry.into_owned())
        {
            self.restore_editor();
            if entry.provider_native.is_some() {
                self.materialize_compaction_summary_for_edit(entry, initial_filter_mode)
                    .await;
            } else {
                self.start_compaction_summary_edit(&entry, entry.summary.clone());
            }
            return;
        }
        if real_leaf_id.as_ref() == Some(&entry_id) {
            self.restore_editor();
            self.show_status("Already at this point");
            return;
        }

        self.restore_editor();
        let mut wants_summary = false;
        let mut custom_instructions: Option<JsString> = None;
        if !self.settings(|settings| settings.get_branch_summary_skip_prompt()) {
            loop {
                let choice = self
                    .show_extension_selector(
                        &JsString::from("Summarize branch?"),
                        &[
                            JsString::from("No summary"),
                            JsString::from("Summarize"),
                            JsString::from("Summarize with custom prompt"),
                        ],
                    )
                    .await;
                let Some(choice) = choice else {
                    self.show_tree_selector(Some(entry_id), Some(initial_filter_mode));
                    return;
                };
                wants_summary = choice != "No summary";
                if choice == "Summarize with custom prompt" {
                    match self
                        .show_extension_editor(&JsString::from("Custom summarization instructions"), None)
                        .await
                    {
                        Some(text) => custom_instructions = Some(text.into()),
                        None => continue,
                    }
                }
                break;
            }
        }

        if self.session().is_streaming() {
            self.restore_queued_messages_to_editor(true);
            if let Err(error) = self.session().abort().await {
                self.show_error(error.message());
            }
        }

        let mode_id = self.ui_id.get();
        let original_on_escape = self.default_editor.borrow().on_escape.clone();
        let summary_loader: Option<ComponentRef> = if wants_summary {
            self.default_editor.borrow_mut().on_escape = Some(Rc::new(move || {
                with_mode(mode_id, |mode| mode.session().abort_branch_summary());
            }));
            self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
            let request_render: Rc<dyn Fn()> = Rc::new(move || {
                with_mode(mode_id, |mode| mode.ui.request_render());
            });
            let loader = component(pi_tui::components::loader::Loader::new(
                request_render,
                Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Accent, text))),
                Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
                js_format!(
                    "Summarizing branch... ({} to cancel)",
                    crate::modes::interactive::components::keybinding_hints::key_text("app.interrupt")
                ),
                None,
            ));
            if let Some(loader) = loader
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<pi_tui::components::loader::Loader>())
            {
                loader.start();
            }
            self.status_container.borrow_mut().add_child(Rc::clone(&loader));
            self.set_prompt_activity(PromptActivitySource::BranchSummary, true);
            self.ui.request_render();
            Some(loader)
        } else {
            None
        };

        let result = self
            .session()
            .navigate_tree(
                &entry_id,
                crate::core::agent_session::NavigateTreeOptions {
                    summarize: Some(wants_summary),
                    custom_instructions,
                    replace_instructions: None,
                    label: None,
                },
            )
            .await;

        match result {
            Ok(result) if result.aborted == Some(true) => {
                self.show_status("Branch summarization cancelled");
                self.show_tree_selector(Some(entry_id), Some(initial_filter_mode));
            }
            Ok(result) if result.cancelled => {
                self.show_status("Navigation cancelled");
            }
            Ok(result) => {
                *self.pending_compaction_summary_edit_id.borrow_mut() = None;
                self.chat_container.borrow_mut().clear();
                self.render_initial_messages();
                if let Some(editor_text) = &result.editor_text
                    && self.editor().borrow().get_text().trim().is_empty()
                {
                    self.editor()
                        .borrow_mut()
                        .set_text(&JsString::from(&(editor_text).clone()));
                }
                self.show_status("Navigated to selected point");
                self.flush_compaction_queue(false).await;
            }
            Err(error) => self.show_error(error.message()),
        }

        if let Some(loader) = summary_loader {
            if let Some(loader) = loader
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<pi_tui::components::loader::Loader>())
            {
                loader.stop();
            }
            self.status_container.borrow_mut().clear();
            self.set_prompt_activity(PromptActivitySource::BranchSummary, false);
        }
        self.default_editor.borrow_mut().on_escape = original_on_escape;
    }

    fn start_compaction_summary_edit(&self, entry: &CompactionEntry, summary: JsString) {
        if !self.editor().borrow().get_text().trim().is_empty() {
            self.show_warning("Clear the editor before editing a compaction summary");
            return;
        }
        *self.pending_compaction_summary_edit_id.borrow_mut() = Some(entry.id.clone());
        self.editor()
            .borrow_mut()
            .set_text(&JsString::from(&(&summary).clone()));
        self.show_status("Editing compaction summary");
        self.ui.request_render();
    }

    fn submit_pending_compaction_summary_edit(&self, summary: &JsString) -> bool {
        let Some(entry_id) = self.pending_compaction_summary_edit_id.borrow().clone() else {
            return false;
        };
        if let Err(error) = self.session().update_compaction_summary(&entry_id, &summary.into()) {
            self.show_error(error.message());
            return true;
        }
        *self.pending_compaction_summary_edit_id.borrow_mut() = None;
        self.add_submitted_text_to_history(summary);
        self.editor().borrow_mut().set_text(&JsString::from(""));
        self.chat_container.borrow_mut().clear();
        self.render_initial_messages();
        self.show_status("Compaction summary updated");
        self.ui.request_render();
        true
    }

    async fn materialize_compaction_summary_for_edit(
        &self,
        entry: CompactionEntry,
        initial_filter_mode: TreeFilterMode,
    ) {
        let mode_id = self.ui_id.get();
        let original_on_escape = self.default_editor.borrow().on_escape.clone();
        self.default_editor.borrow_mut().on_escape = Some(Rc::new(move || {
            with_mode(mode_id, |mode| {
                mode.session().abort_compaction_summary_materialization()
            });
        }));
        self.status_container.borrow_mut().clear();
        let request_render: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| mode.ui.request_render());
        });
        let loader = component(pi_tui::components::loader::Loader::new(
            request_render,
            Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Accent, text))),
            Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
            js_format!(
                "Loading compaction summary... ({} to cancel)",
                crate::modes::interactive::components::keybinding_hints::key_text("app.interrupt")
            ),
            None,
        ));
        if let Some(loader) = loader
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<pi_tui::components::loader::Loader>())
        {
            loader.start();
        }
        self.status_container.borrow_mut().add_child(Rc::clone(&loader));
        self.set_prompt_activity(PromptActivitySource::SummaryMaterialization, true);
        self.ui.request_render();

        let outcome = match self.session().materialize_compaction_summary(&entry.id).await {
            Ok(crate::core::compaction::MaterializeCompactionSummaryResult::Summary(summary)) => Ok(summary),
            Ok(crate::core::compaction::MaterializeCompactionSummaryResult::Aborted) => {
                Err(Some(JsString::from("Compaction summary loading cancelled")))
            }
            Err(error) => Err(Some(error.message().to_owned())),
        };
        if let Some(loader) = loader
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<pi_tui::components::loader::Loader>())
        {
            loader.stop();
        }
        self.status_container.borrow_mut().clear();
        self.set_prompt_activity(PromptActivitySource::SummaryMaterialization, false);
        self.default_editor.borrow_mut().on_escape = original_on_escape;

        match outcome {
            Ok(summary) => self.start_compaction_summary_edit(&entry, summary),
            Err(message) => {
                if let Some(message) = message {
                    if message == "Compaction summary loading cancelled" {
                        self.show_status(&message);
                    } else {
                        self.show_error(&message);
                    }
                }
                self.show_tree_selector(Some(entry.id), Some(initial_filter_mode));
            }
        }
    }

    async fn show_extension_selector(&self, title: &JsString, options: &[JsString]) -> Option<JsString> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        let _ = self.ui_sender.send(UiRequest::Select {
            title: title.to_owned(),
            options: options.to_vec(),
            timeout: None,
            respond,
        });
        receiver.await.ok().flatten()
    }

    async fn show_extension_editor(&self, title: &JsString, prefill: Option<&JsString>) -> Option<JsString> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        let _ = self.ui_sender.send(UiRequest::Editor {
            title: title.to_owned(),
            prefill: prefill.cloned(),
            respond,
        });
        receiver.await.ok().flatten()
    }

    fn show_session_selector(&self) {
        let session = self.session();
        let (cwd, session_dir, uses_default_dir, current_file) = {
            let manager = session.session_manager();
            (
                manager.get_cwd().to_owned(),
                manager.get_session_dir().to_owned(),
                manager.uses_default_session_dir(),
                manager.get_session_file().cloned(),
            )
        };
        let control_db_path = self.options.control_db_path.clone();
        let mode_id = self.ui_id.get();
        let apply = Rc::new(sort_named_sessions_first);

        let current_loader: SessionListLoader = {
            let apply = Rc::clone(&apply);
            let cwd = cwd.clone();
            let session_dir = session_dir.clone();
            let control_db_path = control_db_path.clone();
            Rc::new(move |progress: Option<ProgressCallback>| {
                let mut progress_fn =
                    progress.map(|callback| move |loaded: usize, total: usize| callback(loaded, total));
                let result = crate::core::session_manager::SessionManager::list(
                    &cwd,
                    Some(&session_dir),
                    progress_fn
                        .as_mut()
                        .map(|callback| callback as &mut dyn FnMut(usize, usize)),
                    control_db_path.as_ref(),
                )
                .map_err(|error| error.message().clone());
                let applied = result.map(|sessions| apply(sessions));
                Box::pin(async move { applied })
            })
        };
        let all_loader: SessionListLoader = {
            let apply = Rc::clone(&apply);
            let session_dir = session_dir.clone();
            let control_db_path = control_db_path.clone();
            Rc::new(move |progress: Option<ProgressCallback>| {
                let mut progress_fn =
                    progress.map(|callback| move |loaded: usize, total: usize| callback(loaded, total));
                let dir = (!uses_default_dir).then(|| session_dir.clone());
                let result = crate::core::session_manager::SessionManager::list_all(
                    dir.as_ref(),
                    progress_fn
                        .as_mut()
                        .map(|callback| callback as &mut dyn FnMut(usize, usize)),
                    control_db_path.as_ref(),
                )
                .map_err(|error| error.message().clone());
                let applied = result.map(|sessions| apply(sessions));
                Box::pin(async move { applied })
            })
        };
        let archived_loader: SessionListLoader = {
            let apply = Rc::clone(&apply);
            let session_dir = session_dir.clone();
            let control_db_path = control_db_path.clone();
            Rc::new(move |_progress: Option<ProgressCallback>| {
                let dir = (!uses_default_dir).then(|| session_dir.clone());
                let result =
                    crate::core::session_manager::SessionManager::list_archived(dir.as_ref(), control_db_path.as_ref())
                        .map_err(|error| error.message().clone());
                let applied = result.map(|sessions| apply(sessions));
                Box::pin(async move { applied })
            })
        };

        let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |path: JsString| {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                *mode.active_session_selector.borrow_mut() = None;
                tokio::task::spawn_local(async move {
                    mode.handle_resume_session(&path).await;
                });
            });
        });
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| {
                *mode.active_session_selector.borrow_mut() = None;
                mode.restore_editor();
                mode.ui.request_render();
            });
        });
        let on_exit: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| {
                tokio::task::spawn_local(async move {
                    mode.shutdown().await;
                });
            });
        });
        let request_render: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| mode.ui.request_render());
        });
        let rename_session: RenameSessionFn = {
            let control_db_path = control_db_path.clone();
            Rc::new(move |path: JsString, next_name: Option<JsString>| {
                let next_name = next_name.map(JsString::from);
                let control_db_path = control_db_path.clone();
                Box::pin(async move {
                    let Some(control_db_path) = control_db_path else {
                        return Ok(());
                    };
                    crate::core::session_control_db::write_session_name(&control_db_path, &path, next_name.as_ref())
                        .map_err(|error| error.message().clone())?;
                    Ok(())
                })
            })
        };

        let selector = Rc::new(tokio::sync::Mutex::new(SessionSelectorComponent::new(
            current_loader,
            all_loader,
            on_select,
            on_cancel,
            on_exit,
            request_render,
            Some(rename_session),
            Some(true),
            Some(self.keybindings.clone()),
            control_db_path,
            Some(archived_loader),
            current_file.as_ref(),
        )));
        *self.active_session_selector.borrow_mut() = Some(Rc::clone(&selector));
        self.session_selector_loaded.set(false);
        self.show_selector(component(SharedSessionSelector(selector)));
    }

    async fn poll_active_session_selector(&self) {
        let Some(selector) = self.active_session_selector.borrow().clone() else {
            return;
        };
        if !self.session_selector_loaded.get() {
            selector.lock().await.load_current_sessions().await;
            self.session_selector_loaded.set(true);
            self.ui.request_render();
        }
        while selector.lock().await.has_pending_action() {
            selector.lock().await.run_pending_action().await;
            self.ui.request_render();
        }
        if selector.lock().await.poll(Instant::now()) {
            self.ui.request_render();
        }
    }

    async fn continue_interrupted_resumed_session(&self) {
        let extension_requested_continuation = self.session().consume_resume_continuation_request();
        if !extension_requested_continuation
            && !crate::core::agent_session::should_continue_interrupted_session(&self.session().messages())
        {
            return;
        }
        if let Err(error) = self.session().continue_run().await {
            self.show_error(error.message());
        }
    }

    async fn handle_resume_session(&self, session_path: &JsString) {
        self.status_container.borrow_mut().clear();
        let switched = {
            let mut runtime = self.runtime.lock().await;
            match runtime.switch_session(session_path, None, None).await {
                Ok(result) if result.cancelled => return,
                Ok(_) => Ok(runtime.session().clone()),
                Err(error) => Err(error),
            }
        };
        match switched {
            Ok(session) => {
                self.adopt_session(session);
                self.continue_interrupted_resumed_session().await;
                self.show_status("Resumed session");
            }
            Err(error) => {
                let issue = self.missing_session_cwd_issue(session_path);
                let Some(issue) = issue else {
                    self.handle_fatal_runtime_error(&JsString::from("Failed to resume session"), error);
                };
                let confirmed = self
                    .show_extension_confirm(
                        &JsString::from("Session cwd not found"),
                        &crate::core::session_cwd::format_missing_session_cwd_prompt(&issue),
                    )
                    .await;
                if !confirmed {
                    self.show_status("Resume cancelled");
                    return;
                }
                let switched = {
                    let mut runtime = self.runtime.lock().await;
                    match runtime
                        .switch_session(session_path, Some(&issue.fallback_cwd), None)
                        .await
                    {
                        Ok(result) if result.cancelled => return,
                        Ok(_) => Ok(runtime.session().clone()),
                        Err(error) => Err(error),
                    }
                };
                match switched {
                    Ok(session) => {
                        self.adopt_session(session);
                        self.continue_interrupted_resumed_session().await;
                        self.show_status("Resumed session in current cwd");
                    }
                    Err(error) => self.handle_fatal_runtime_error(&JsString::from("Failed to resume session"), error),
                }
            }
        }
    }

    fn missing_session_cwd_issue(&self, session_path: &JsString) -> Option<crate::core::session_cwd::SessionCwdIssue> {
        let manager = crate::core::session_manager::SessionManager::open(session_path, None, None).ok()?;
        crate::core::session_cwd::get_missing_session_cwd_issue(&manager, self.session().cwd())
    }

    async fn show_extension_confirm(&self, title: &JsString, message: &JsString) -> bool {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        let _ = self.ui_sender.send(UiRequest::Confirm {
            title: title.to_owned(),
            message: message.to_owned(),
            timeout: None,
            respond,
        });
        receiver.await.unwrap_or(false)
    }

    fn poll_auth_ui(&self) {
        loop {
            let (event, dialog) = {
                let auth = self.auth_ui.borrow();
                let Some(state) = auth.as_ref() else {
                    return;
                };
                (state.receiver.try_recv().ok(), Rc::clone(&state.dialog))
            };
            let Some(event) = event else {
                return;
            };
            match event {
                AuthUiEvent::Notify(event) => self.notify_auth_dialog(&dialog, event),
                AuthUiEvent::Prompt(prompt, respond) => self.show_auth_prompt(&dialog, prompt, respond),
            }
        }
    }

    fn mount_auth_dialog(&self, dialog: &Rc<RefCell<LoginDialogComponent>>) {
        self.editor_container.borrow_mut().clear();
        let component: ComponentRef = component(SharedLoginDialog(Rc::clone(dialog)));
        self.editor_container.borrow_mut().add_child(Rc::clone(&component));
        self.ui.set_focus(Some(component));
        self.ui.request_render();
    }

    fn restore_auth_dialog(&self, dialog: &Rc<RefCell<LoginDialogComponent>>) {
        self.mount_auth_dialog(dialog);
    }

    fn notify_auth_dialog(&self, dialog: &Rc<RefCell<LoginDialogComponent>>, event: AuthEvent) {
        let dialog = dialog.borrow();
        match event {
            AuthEvent::AuthUrl { url, instructions } => {
                dialog.show_auth(&url, instructions.as_ref());
            }
            AuthEvent::DeviceCode {
                user_code,
                verification_uri,
                interval_seconds,
                expires_in_seconds,
            } => {
                dialog.show_device_code(&pi_ai::OAuthDeviceCodeInfo {
                    user_code,
                    verification_uri,
                    interval_seconds,
                    expires_in_seconds,
                });
                dialog.show_waiting(&"Waiting for authentication...".into());
            }
            AuthEvent::Info { message, links } => {
                dialog.show_info(&message, links.as_deref().unwrap_or(&[]), false);
            }
            AuthEvent::Progress { message } => dialog.show_progress(&message),
        }
    }

    fn show_auth_prompt(
        &self,
        dialog: &Rc<RefCell<LoginDialogComponent>>,
        prompt: AuthPrompt,
        respond: tokio::sync::oneshot::Sender<Result<JsString, JsString>>,
    ) {
        match prompt.kind {
            AuthPromptKind::Select { message, options } => {
                let mode_id = self.ui_id.get();
                let dialog = Rc::clone(dialog);
                let respond = Rc::new(RefCell::new(Some(respond)));
                let labels: Vec<JsString> = options.iter().map(|option| option.label.clone()).collect();
                let select_respond = Rc::clone(&respond);
                let select_dialog = Rc::clone(&dialog);
                let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |label: JsString| {
                    if let Some(respond) = select_respond.borrow_mut().take() {
                        let id = options
                            .iter()
                            .find(|option| option.label == label)
                            .map(|option| option.id.clone());
                        let _ = respond.send(match id {
                            Some(id) => Ok((id).into()),
                            None => Err(JsString::from("Login cancelled")),
                        });
                    }
                    with_mode(mode_id, |mode| mode.restore_auth_dialog(&select_dialog));
                });
                let cancel_respond = Rc::clone(&respond);
                let cancel_dialog = Rc::clone(&dialog);
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    respond_once(&cancel_respond, Err(JsString::from("Login cancelled")));
                    with_mode(mode_id, |mode| mode.restore_auth_dialog(&cancel_dialog));
                });
                let selector = ExtensionSelectorComponent::new(
                    &JsString::from(&message),
                    labels,
                    on_select,
                    on_cancel,
                    ExtensionSelectorOptions::default(),
                );
                self.editor_container.borrow_mut().clear();
                let component: ComponentRef = component(selector);
                self.editor_container.borrow_mut().add_child(Rc::clone(&component));
                self.ui.set_focus(Some(component));
                self.ui.request_render();
            }
            kind => {
                self.mount_auth_dialog(dialog);
                let receiver = match kind {
                    AuthPromptKind::Text { message, placeholder } => {
                        dialog.borrow().show_prompt(&message, placeholder.as_ref())
                    }
                    AuthPromptKind::Secret { message, placeholder } => {
                        dialog.borrow().show_prompt(&message, placeholder.as_ref())
                    }
                    AuthPromptKind::ManualCode { message, .. } => dialog.borrow().show_manual_input(&message),
                    AuthPromptKind::Select { .. } => unreachable!(),
                };
                tokio::task::spawn_local(async move {
                    let result = match receiver.await {
                        Ok(Ok(value)) => Ok(value),
                        Ok(Err(error)) => Err(error.0),
                        Err(_) => Err(JsString::from("Login cancelled")),
                    };
                    let _ = respond.send(result);
                });
            }
        }
    }

    async fn handle_login_command(&self, provider_ref: Option<&JsString>) {
        let provider_ref = provider_ref
            .map(|text| text.trim())
            .filter(|provider_ref| !provider_ref.is_empty());
        let Some(provider_ref) = provider_ref else {
            self.show_login_auth_type_selector(None);
            return;
        };
        let provider_options = self.find_login_provider_options(&provider_ref);
        if provider_options.len() == 1 {
            self.start_provider_login(provider_options[0].clone()).await;
            return;
        }
        if provider_options.len() > 1 {
            let unique_ids: HashSet<&str> = provider_options.iter().map(|option| option.id.as_str()).collect();
            if unique_ids.len() == 1 {
                self.show_login_auth_type_selector(Some(provider_options));
                return;
            }
        }
        self.show_login_provider_selector(None, Some(provider_ref.to_owned()));
    }

    fn find_login_provider_options(&self, provider_ref: &JsString) -> Vec<AuthSelectorProvider> {
        let normalized = provider_ref.trim().to_lowercase();
        if normalized.is_empty() {
            return Vec::new();
        }
        get_login_provider_options(self.session().model_runtime(), None)
            .into_iter()
            .filter(|provider| {
                JsString::from(provider.id.to_lowercase()) == normalized
                    || JsString::from(provider.name.to_lowercase()) == normalized
            })
            .collect()
    }

    fn get_login_auth_type_selector_options(
        provider_options: Option<&[AuthSelectorProvider]>,
    ) -> (JsString, Vec<JsString>) {
        let oauth_label = provider_options
            .and_then(|options| options.iter().find(|provider| provider.auth_type == AuthType::Oauth))
            .and_then(|provider| provider.method_name.clone());
        let subscription_label = oauth_label
            .map(JsString::from)
            .unwrap_or_else(|| JsString::from("Sign in with an account"));
        let available: Vec<AuthType> = match provider_options {
            Some(options) => options.iter().map(|provider| provider.auth_type).collect(),
            None => vec![AuthType::Oauth, AuthType::ApiKey],
        };
        let mut options = Vec::new();
        if available.contains(&AuthType::Oauth) {
            options.push(subscription_label.clone());
        }
        if available.contains(&AuthType::ApiKey) {
            options.push(JsString::from("Sign in with an API key"));
        }
        ((subscription_label).into(), options)
    }

    fn show_login_auth_type_selector(&self, provider_options: Option<Vec<AuthSelectorProvider>>) {
        let (subscription_label, options) = Self::get_login_auth_type_selector_options(provider_options.as_deref());
        if options.is_empty() {
            self.show_status("No login methods available.");
            return;
        }
        let first_provider_option = provider_options.as_ref().and_then(|options| options.first().cloned());
        if let Some(first_provider_option) = first_provider_option.clone()
            && options.len() == 1
        {
            let mode_id = self.ui_id.get();
            tokio::task::spawn_local(async move {
                if let Some(mode) = with_mode(mode_id, |mode| mode) {
                    mode.start_provider_login(first_provider_option).await;
                }
            });
            return;
        }
        let title = match &first_provider_option {
            Some(provider) => js_format!("Select authentication method for {}:", provider.name),
            None => JsString::from("Select authentication method:"),
        };
        let mode_id = self.ui_id.get();
        let options_for_select = provider_options.clone();
        let subscription_for_select = subscription_label.clone();
        let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |option: JsString| {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                let auth_type = if option == subscription_for_select {
                    AuthType::Oauth
                } else {
                    AuthType::ApiKey
                };
                match &options_for_select {
                    None => mode.show_login_provider_selector(Some(auth_type), None),
                    Some(providers) => {
                        if let Some(provider_option) = providers
                            .iter()
                            .find(|provider| provider.auth_type == auth_type)
                            .cloned()
                        {
                            tokio::task::spawn_local(async move {
                                mode.start_provider_login(provider_option).await;
                            });
                        }
                    }
                }
            });
        });
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                mode.ui.request_render();
            });
        });
        self.show_selector(component(ExtensionSelectorComponent::new(
            &title,
            options,
            on_select,
            on_cancel,
            ExtensionSelectorOptions::default(),
        )));
    }

    fn show_login_provider_selector(&self, auth_type: Option<AuthType>, initial_search: Option<JsString>) {
        let provider_options = get_login_provider_options(self.session().model_runtime(), auth_type);
        if provider_options.is_empty() {
            let message = match auth_type {
                Some(AuthType::Oauth) => "No subscription providers available.",
                Some(AuthType::ApiKey) => "No API key providers available.",
                None => "No login providers available.",
            };
            self.show_status(message);
            return;
        }
        let mode_id = self.ui_id.get();
        let options_for_select = provider_options.clone();
        let on_select: Rc<dyn Fn(String, AuthType)> =
            Rc::new(move |provider_id: String, selected_auth_type: AuthType| {
                with_mode(mode_id, |mode| {
                    mode.restore_editor();
                    if let Some(provider_option) = options_for_select
                        .iter()
                        .find(|provider| provider.id == provider_id && provider.auth_type == selected_auth_type)
                        .cloned()
                    {
                        tokio::task::spawn_local(async move {
                            mode.start_provider_login(provider_option).await;
                        });
                    }
                });
            });
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                if auth_type.is_some() {
                    mode.show_login_auth_type_selector(None);
                } else {
                    mode.ui.request_render();
                }
            });
        });
        self.show_selector(component(OAuthSelectorComponent::new(
            AuthSelectorMode::Login,
            provider_options,
            on_select,
            on_cancel,
            initial_search.as_ref(),
        )));
    }

    async fn show_oauth_selector(&self, logout: bool) {
        if !logout {
            self.show_login_auth_type_selector(None);
            return;
        }
        let provider_options = match get_logout_provider_options(self.session().model_runtime()).await {
            Ok(options) => options,
            Err(error) => {
                self.show_error(&js_format!("Could not read stored credentials: {}", error));
                return;
            }
        };
        if provider_options.is_empty() {
            self.show_status(
                "No stored credentials to remove. /logout only removes credentials saved by /login; environment variables and models.json config are unchanged.",
            );
            return;
        }
        let mode_id = self.ui_id.get();
        let options_for_select = provider_options.clone();
        let on_select: Rc<dyn Fn(String, AuthType)> = Rc::new(move |provider_id: String, _auth_type: AuthType| {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                let Some(provider_option) = options_for_select
                    .iter()
                    .find(|provider| provider.id == provider_id)
                    .cloned()
                else {
                    return;
                };
                tokio::task::spawn_local(async move {
                    let runtime = mode.session().model_runtime().clone();
                    let timeout = crate::main_runner::TimeoutSignal::new(15_000);
                    match runtime
                        .logout(
                            &provider_option.id,
                            pi_ai::auth::types::AuthOperationOptions::new(Some(timeout.token())),
                        )
                        .await
                    {
                        Ok(()) => {
                            mode.update_available_provider_count();
                            let message = if provider_option.auth_type == AuthType::Oauth {
                                js_format!("Logged out of {}", provider_option.name)
                            } else {
                                js_format!(
                                    "Removed stored API key for {}. Environment variables and models.json config are unchanged.",
                                    provider_option.name
                                )
                            };
                            mode.show_status(&message);
                        }
                        Err(error) => {
                            let message = error.message();
                            if matches!(error, crate::core::model_runtime::ModelRuntimeError::CredentialSync(_)) {
                                mode.show_error(&js_format!(
                                    "Credentials removed for {}, but local model state could not be synchronized: {}",
                                    provider_option.name,
                                    message
                                ));
                            } else {
                                mode.show_error(&js_format!("Logout failed: {}", message));
                            }
                        }
                    }
                });
            });
        });
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                mode.ui.request_render();
            });
        });
        self.show_selector(component(OAuthSelectorComponent::new(
            AuthSelectorMode::Logout,
            provider_options,
            on_select,
            on_cancel,
            None,
        )));
    }

    async fn start_provider_login(&self, provider_option: AuthSelectorProvider) {
        match provider_option.auth_type {
            AuthType::Oauth => {
                self.show_login_dialog(&provider_option.id, &provider_option.name).await;
            }
            AuthType::ApiKey => {
                if provider_option.login_supported {
                    self.show_api_key_login_dialog(&provider_option.id, &provider_option.name)
                        .await;
                } else {
                    self.show_ambient_auth_dialog(&provider_option);
                }
            }
        }
    }

    async fn show_login_dialog(&self, provider_id: &str, provider_name: &str) {
        if let Err(error) = self
            .run_provider_login(provider_id, provider_name, AuthType::Oauth)
            .await
        {
            let message = error.message();
            if matches!(error, crate::core::model_runtime::ModelRuntimeError::CredentialSync(_)) {
                self.show_error(&js_format!(
                    "Logged in to {}, but local model state could not be synchronized: {}",
                    provider_name,
                    message
                ));
            } else if message != "Login cancelled" {
                self.show_error(&js_format!("Failed to login to {}: {}", provider_name, message));
            }
        }
    }

    async fn show_api_key_login_dialog(&self, provider_id: &str, provider_name: &str) {
        if let Err(error) = self
            .run_provider_login(provider_id, provider_name, AuthType::ApiKey)
            .await
        {
            let message = error.message();
            if matches!(error, crate::core::model_runtime::ModelRuntimeError::CredentialSync(_)) {
                self.show_error(&js_format!(
                    "Logged in to {}, but local model state could not be synchronized: {}",
                    provider_name,
                    message
                ));
            } else if message != "Login cancelled" {
                self.show_error(&js_format!("Failed to save API key for {}: {}", provider_name, message));
            }
        }
    }

    async fn run_provider_login(
        &self,
        provider_id: &str,
        provider_name: &str,
        auth_type: AuthType,
    ) -> Result<(), crate::core::model_runtime::ModelRuntimeError> {
        let previous_model = self.session().model();
        let mode_id = self.ui_id.get();
        let request_render: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| mode.ui.request_render());
        });
        let dialog = Rc::new(RefCell::new(LoginDialogComponent::new(
            request_render,
            provider_id,
            Rc::new(|_, _| {}),
            Some(provider_name),
            None,
        )));
        if provider_id == "amazon-bedrock" {
            let docs_path = crate::config::get_docs_path().join("providers.md");
            dialog.borrow().show_details(&[
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Text,
                        "You can also use an AWS profile, IAM keys, or role-based credentials.",
                    )
                }),
                with_theme(|theme| theme.fg(ThemeColor::Muted, "See:")),
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Accent,
                        &js_format!("  {}", JsString::from(docs_path.to_string_lossy().as_ref())),
                    )
                }),
            ]);
        }
        self.mount_auth_dialog(&dialog);
        let (sender, receiver) = std::sync::mpsc::channel();
        let signal = dialog.borrow().signal();
        *self.auth_ui.borrow_mut() = Some(AuthUiState {
            receiver,
            dialog: Rc::clone(&dialog),
        });
        let interaction = Arc::new(InteractiveAuthInteraction {
            sender: std::sync::Mutex::new(sender),
            signal,
        });
        let runtime = self.session().model_runtime().clone();
        let result = runtime.login(provider_id, auth_type, interaction).await;
        *self.auth_ui.borrow_mut() = None;
        self.restore_editor();
        result?;
        self.complete_provider_authentication(provider_id, provider_name, auth_type, previous_model)
            .await;
        Ok(())
    }

    async fn maybe_warn_about_anthropic_subscription_auth(&self, model: Option<Model>) {
        let warnings_disabled = self.settings(|settings| {
            settings
                .get_warnings()
                .get("anthropicExtraUsage")
                .and_then(JsValue::as_bool)
        }) == Some(false);
        if warnings_disabled || self.anthropic_subscription_warning_shown.get() {
            return;
        }
        let model = model.unwrap_or_else(|| self.session().model());
        if model.provider != "anthropic" {
            return;
        }
        let runtime = self.session().model_runtime().clone();
        let check = runtime
            .check_auth("anthropic", pi_ai::auth::types::AuthOperationOptions::new(None))
            .await;
        if let Ok(Some(check)) = check
            && check.auth_type == AuthType::Oauth
        {
            self.anthropic_subscription_warning_shown.set(true);
            self.show_warning(ANTHROPIC_SUBSCRIPTION_AUTH_WARNING);
            return;
        }
        let auth = runtime
            .get_auth("anthropic", pi_ai::auth::resolve::AuthResolutionOverrides::default())
            .await;
        if let Ok(Some(auth)) = auth
            && auth
                .auth
                .api_key
                .as_ref()
                .is_some_and(|api_key| api_key.starts_with("sk-ant-oat"))
        {
            self.anthropic_subscription_warning_shown.set(true);
            self.show_warning(ANTHROPIC_SUBSCRIPTION_AUTH_WARNING);
        }
    }

    fn handle_armin_says_hi(&self) {
        self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
        let request_render: Rc<dyn Fn()> = Rc::new({
            let ui = Rc::clone(&self.ui);
            move || ui.request_render()
        });
        self.chat_container
            .borrow_mut()
            .add_child(component(ArminComponent::new(request_render)));
        self.ui.request_render();
    }

    fn handle_demented_delves(&self) {
        self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
        self.chat_container
            .borrow_mut()
            .add_child(component(EarendilAnnouncementComponent::new(
                crate::modes::interactive::theme::current_theme(),
            )));
        self.ui.request_render();
    }

    fn handle_daxnuts(&self) {
        self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
        let request_render: Rc<dyn Fn()> = Rc::new({
            let ui = Rc::clone(&self.ui);
            move || ui.request_render()
        });
        self.chat_container
            .borrow_mut()
            .add_child(component(DaxnutsComponent::new(request_render)));
        self.ui.request_render();
    }

    fn check_daxnuts_easter_egg(&self, model: &Model) {
        if model.provider == "opencode" && model.id.to_lowercase().contains("kimi-k2.5") {
            self.handle_daxnuts();
        }
    }

    fn show_ambient_auth_dialog(&self, provider_option: &AuthSelectorProvider) {
        let mode_id = self.ui_id.get();
        let request_render: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| mode.ui.request_render());
        });
        let dialog = Rc::new(RefCell::new(LoginDialogComponent::new(
            request_render,
            &provider_option.id,
            Rc::new(|_, _| {}),
            Some(&provider_option.name),
            Some(&js_format!("{} setup", provider_option.name)),
        )));
        dialog.borrow().show_info(
            &js_format!(
                "{} is configured outside pi.",
                provider_option
                    .method_name
                    .clone()
                    .unwrap_or_else(|| "Authentication".into())
            ),
            &[],
            true,
        );
        self.mount_auth_dialog(&dialog);
    }

    async fn complete_provider_authentication(
        &self,
        provider_id: &str,
        provider_name: &str,
        auth_type: AuthType,
        previous_model: Model,
    ) {
        let action_label = if auth_type == AuthType::Oauth {
            js_format!("Logged in to {}", provider_name)
        } else {
            js_format!("Saved API key for {}", provider_name)
        };
        let mut selected_model: Option<Model> = None;
        let mut selection_error: Option<JsString> = None;
        if is_unknown_model(Some(&previous_model)) {
            let available_models = self.session().model_runtime().get_available_snapshot();
            let provider_models: Vec<Model> = available_models
                .into_iter()
                .filter(|model| model.provider == provider_id)
                .collect();
            match crate::core::model_resolver::default_model_for_provider(provider_id) {
                None => {
                    selection_error = Some(js_format!(
                        "{}, but no default model is configured for provider \"{}\". Use /model to select a model.",
                        action_label,
                        provider_id
                    ));
                }
                Some(_default_model_id) if provider_models.is_empty() => {
                    selection_error = Some(js_format!(
                        "{}, but no models are available for that provider. Use /model to select a model.",
                        action_label
                    ));
                }
                Some(default_model_id) => {
                    selected_model = provider_models
                        .iter()
                        .find(|model| model.id == default_model_id)
                        .cloned();
                    match selected_model.clone() {
                        None => {
                            selection_error = Some(js_format!(
                                "{}, but its default model \"{}\" is not available. Use /model to select a model.",
                                action_label,
                                default_model_id
                            ));
                        }
                        Some(model) => {
                            if let Err(error) = self.session().set_model(model).await {
                                selected_model = None;
                                selection_error = Some(js_format!(
                                    "{}, but selecting its default model failed: {}. Use /model to select a model.",
                                    action_label,
                                    (error.message()).clone()
                                ));
                            }
                        }
                    }
                }
            }
        }

        self.update_available_provider_count();
        self.footer.borrow_mut().invalidate();
        self.update_editor_border_color();
        let auth_path = crate::config::get_auth_path().display().to_string();
        if let Some(model) = &selected_model {
            self.show_status(&js_format!(
                "{}. Selected {}. Credentials saved to {}",
                action_label,
                model.id,
                auth_path
            ));
            self.maybe_warn_about_anthropic_subscription_auth(Some(model.clone()))
                .await;
            self.check_daxnuts_easter_egg(model);
        } else {
            self.show_status(&js_format!("{}. Credentials saved to {}", action_label, auth_path));
            if let Some(error) = selection_error {
                self.show_error(&error);
            } else {
                self.maybe_warn_about_anthropic_subscription_auth(None).await;
            }
        }

        let mode_id = self.ui_id.get();
        let runtime = self.session().model_runtime().clone();
        let provider_id = provider_id.to_owned();
        tokio::task::spawn_local(async move {
            let abort = tokio_util::sync::CancellationToken::new();
            let timed_out = Rc::new(Cell::new(false));
            let timeout_task = {
                let abort = abort.clone();
                let timed_out = Rc::clone(&timed_out);
                tokio::task::spawn_local(async move {
                    tokio::time::sleep(Duration::from_secs(15)).await;
                    timed_out.set(true);
                    abort.cancel();
                })
            };
            let result = runtime
                .refresh(pi_ai::models::ModelsRefreshOptions {
                    providers: Some(vec![provider_id]),
                    signal: Some(abort),
                    ..Default::default()
                })
                .await;
            timeout_task.abort();
            let warning = match &result {
                Ok(result) if result.aborted && timed_out.get() => Some(js_format!(
                    "{}, but its model catalog refresh timed out; using cached models.",
                    action_label
                )),
                Ok(result) if result.aborted => None,
                Ok(result) if !result.errors.is_empty() => Some(js_format!(
                    "{}, but its model catalog could not be refreshed; using cached models.",
                    action_label
                )),
                Err(error) => Some(js_format!(
                    "{}, but its model catalog could not be refreshed: {}",
                    action_label,
                    error.message()
                )),
                Ok(_) => None,
            };
            with_mode(mode_id, |mode| {
                if let Some(warning) = warning {
                    mode.show_warning(&warning);
                }
                mode.update_available_provider_count();
                mode.footer.borrow_mut().invalidate();
                mode.ui.request_render();
            });
        });
    }

    fn set_extension_status(&self, key: &JsString, text: Option<&JsString>) {
        self.footer_data_provider.set_extension_status(key, text);
        self.ui.request_render();
    }

    fn show_trust_selector(&self) {
        let session = self.session();
        let cwd = session.cwd().to_owned();
        let trust_store = ProjectTrustStore::new(session.agent_dir())
            .unwrap_or_else(|error| throw_uncaught(ERROR, error.message().clone()));
        let saved_decision = match trust_store.get_entry(&cwd) {
            Ok(decision) => decision,
            Err(error) => {
                self.show_error(error.message());
                return;
            }
        };
        let mode_id = self.ui_id.get();
        let options = TrustSelectorOptions {
            cwd,
            saved_decision,
            project_trusted: self.settings(|settings| settings.is_project_trusted()),
            on_select: Rc::new(move |selection| {
                with_mode(mode_id, |mode| {
                    let trust_store = ProjectTrustStore::new(mode.session().agent_dir())
                        .unwrap_or_else(|error| throw_uncaught(ERROR, error.message().clone()));
                    if let Err(error) = trust_store.set_many(&selection.updates) {
                        mode.show_error(error.message());
                        return;
                    }
                    mode.restore_editor();
                    mode.show_status(&js_format!(
                        "Saved trust decision: {}. Restart pi for this to take effect.",
                        if selection.trusted { "trusted" } else { "untrusted" }
                    ));
                });
            }),
            on_cancel: cancel_restores_editor(mode_id),
        };
        self.show_selector(component(
            TrustSelectorComponent::new(options).unwrap_or_else(|error| throw_uncaught(ERROR, error.message().clone())),
        ));
    }

    fn show_approval_selector(&self) {
        let mode_id = self.ui_id.get();
        let options = ApprovalSelectorOptions {
            current_preset: self.settings(|settings| settings.get_approval_preset()),
            on_select: Rc::new(move |selection| {
                with_mode(mode_id, |mode| {
                    let scope = match selection.scope {
                        crate::modes::interactive::components::approval_selector::ApprovalScope::Global => {
                            crate::core::settings_manager::SettingsScope::Global
                        }
                        crate::modes::interactive::components::approval_selector::ApprovalScope::Project => {
                            crate::core::settings_manager::SettingsScope::Project
                        }
                    };
                    if let Err(error) =
                        mode.settings_mut(|settings| settings.set_approval_preset(selection.preset, scope))
                    {
                        mode.show_error(error.message());
                        return;
                    }
                    mode.restore_editor();
                    mode.show_status(&js_format!(
                        "Saved approval preset to {} settings. Restart pi for this to take effect.",
                        selection.scope.as_str()
                    ));
                });
            }),
            on_cancel: cancel_restores_editor(mode_id),
        };
        self.show_selector(component(ApprovalSelectorComponent::new(options)));
    }

    fn update_bwrap_status_for_sandbox_profile(&self) {
        let profile = self.settings(|settings| settings.get_explicit_sandbox_profile());
        let profile = profile.filter(|profile| *profile != SandboxProfileName::FullAccess);
        let Some(profile) = profile else {
            self.set_extension_status(
                &JsString::from("bwrap"),
                Some(&with_theme(|theme| theme.fg(ThemeColor::Muted, "bwrap: full access"))),
            );
            return;
        };
        let bwrap_available = std::process::Command::new("bwrap")
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        let text = if bwrap_available {
            with_theme(|theme| theme.fg(ThemeColor::Accent, &js_format!("bwrap: {}", profile.as_str())))
        } else {
            with_theme(|theme| theme.fg(ThemeColor::Error, "bwrap unavailable"))
        };
        self.set_extension_status(&JsString::from("bwrap"), Some(&text));
    }

    fn show_sandbox_selector(&self) {
        let mode_id = self.ui_id.get();
        let options = SandboxSelectorOptions {
            current_profile: self.settings(|settings| settings.get_configured_sandbox_profile()),
            session_profile: self.settings(|settings| settings.get_session_sandbox_profile()),
            on_select: Rc::new(move |selection| {
                with_mode(mode_id, |mode| {
                    let (profile, scope) = match selection {
                        SandboxSelectorSelection::Saved { profile, scope } => (
                            Some(profile),
                            match scope {
                                SandboxScope::Global => SandboxProfileScope::Global,
                                SandboxScope::Project => SandboxProfileScope::Project,
                                SandboxScope::Session => SandboxProfileScope::Session,
                            },
                        ),
                        SandboxSelectorSelection::Session { profile } => (profile, SandboxProfileScope::Session),
                    };
                    if let Err(error) = mode.session().set_sandbox_profile(profile, scope) {
                        mode.show_error(error.message());
                        return;
                    }
                    mode.restore_editor();
                    mode.update_bwrap_status_for_sandbox_profile();
                    mode.show_status(&js_format!(
                        "Saved sandbox profile {} to {} scope.",
                        profile.map(SandboxProfileName::as_str).unwrap_or("inherit"),
                        scope.as_str()
                    ));
                });
            }),
            on_cancel: cancel_restores_editor(mode_id),
        };
        self.show_selector(component(SandboxSelectorComponent::new(options)));
    }

    async fn shutdown(&self) {
        if self.is_shutting_down.swap(true, Ordering::SeqCst) {
            return;
        }
        self.clear_extension_widgets();
        self.close_response_complete_notification();
        self.theme_controller.borrow_mut().disable_auto_sync();
        self.ui.terminal().borrow_mut().drain_input(1000, 50);
        self.stop();
        self.runtime.lock().await.dispose().await;
        let resume_command = {
            let session = self.session();
            let manager = session.session_manager();
            format_resume_command(&manager)
        };
        if let Some(resume_command) = resume_command {
            use std::io::Write;
            let hint = crate::utils::chalk::Chalk::stdout().dim("To resume this session:");
            let _ =
                std::io::stdout().write_all(encode_node_utf8(&js_format!("{} {}\n", hint, resume_command)).as_bytes());
        }
    }

    /// `registerSignalHandlers`: SIGTERM performs the graceful shutdown,
    /// SIGHUP restarts the process. Both kill tracked detached children first.
    /// The `uncaughtException` listener becomes the process crash handler; the
    /// stdin/stdout/stderr `error` listeners are `handle_terminal_errors`.
    fn register_signal_handlers(self: &Rc<Self>) {
        self.unregister_signal_handlers();
        let handlers = spawn_signal_handlers(self);
        *self.signal_handlers.borrow_mut() = handlers;
        let shutting_down = Arc::clone(&self.is_shutting_down);
        let handlers: Vec<_> = self
            .signal_handlers
            .borrow()
            .iter()
            .map(|handler| handler.abort_handle())
            .collect();
        let restore = self.ui.terminal().borrow().crash_restore();
        set_process_crash_handler(Some(Arc::new(move |payload| {
            Self::uncaught_crash(payload, &shutting_down, &handlers, restore.as_ref());
        })));
    }

    fn unregister_signal_handlers(&self) {
        for handler in self.signal_handlers.borrow_mut().drain(..) {
            handler.abort();
        }
        set_process_crash_handler(None);
    }

    /// `emergencyTerminalExit`.
    fn emergency_terminal_exit(&self) -> ! {
        self.is_shutting_down.store(true, Ordering::SeqCst);
        self.unregister_signal_handlers();
        kill_tracked_detached_children();
        // The terminal is gone. Do not run normal shutdown because TUI and
        // extension cleanup can write restore sequences and re-trigger EIO.
        std::process::exit(129)
    }

    /// `uncaughtCrash`: last-resort handler for uncaught exceptions. Unlike
    /// `emergency_terminal_exit` the terminal is still alive, so `ui.stop()`
    /// restores cooked mode, the cursor, and the keyboard protocols.
    fn uncaught_crash(
        payload: &(dyn Any + Send),
        shutting_down: &AtomicBool,
        handlers: &[tokio::task::AbortHandle],
        restore: Option<&pi_tui::terminal::TerminalRestore>,
    ) -> ! {
        let dead_terminal = payload_is_dead_terminal_error(payload);
        let action = crash_action(dead_terminal, shutting_down.swap(true, Ordering::SeqCst));
        if action == CrashAction::ExitOne {
            std::process::exit(1);
        }
        if action == CrashAction::EmergencyExit {
            Self::unregister_crash_handlers(handlers);
            kill_tracked_detached_children();
            std::process::exit(129);
        }
        swallow_panics(|| Self::unregister_crash_handlers(handlers));
        swallow_panics(kill_tracked_detached_children);
        if let Some(restore) = restore {
            swallow_panics(|| {
                restore.restore_then(|_result| {
                    // TS catches ui.stop() failures and still reports the original error.
                    Self::report_uncaught_and_exit(payload);
                })
            });
        }
        Self::report_uncaught_and_exit(payload)
    }

    fn unregister_crash_handlers(handlers: &[tokio::task::AbortHandle]) {
        set_process_crash_handler(None);
        for handler in handlers {
            handler.abort();
        }
    }

    fn report_uncaught_and_exit(payload: &(dyn Any + Send)) -> ! {
        eprintln!("pi exiting due to uncaughtException:");
        eprintln!("{}", encode_node_utf8(&format_uncaught(payload)));
        std::process::exit(1)
    }

    /// `terminalErrorHandler`: the stdin/stdout/stderr `error` listener.
    fn terminal_error_handler(&self, failure: &pi_tui::terminal::TerminalIoError) {
        match terminal_error_action(&failure.error) {
            TerminalErrorAction::EmergencyExit => self.emergency_terminal_exit(),
            TerminalErrorAction::Ignore => {}
            TerminalErrorAction::Rethrow => throw_uncaught(ERROR, terminal_error_message(failure)),
        }
    }

    /// Delivers the terminal's queued I/O failures to `terminal_error_handler`
    /// (Node emits them as stream `error` events).
    fn handle_terminal_errors(&self) {
        let errors = self.ui.terminal().borrow_mut().take_errors();
        for failure in &errors {
            self.terminal_error_handler(failure);
        }
    }

    async fn shutdown_from_signal(&self) {
        if self.is_shutting_down.swap(true, Ordering::SeqCst) {
            return;
        }
        self.runtime.lock().await.dispose().await;
        self.theme_controller.borrow_mut().disable_auto_sync();
        self.ui.terminal().borrow_mut().drain_input(1000, 50);
        self.stop();
        std::process::exit(0);
    }

    async fn restart_from_signal(&self) {
        self.theme_controller.borrow_mut().disable_auto_sync();
        let result = self.runtime.lock().await.restart(None, true).await;
        // `void this.restartProcess(...)`: a rejection is an uncaught exception.
        if let Err(error) = result {
            throw_uncaught(ERROR, error);
        }
    }

    async fn handle_ui_requests(&self) {
        self.poll_auth_ui();
        self.poll_active_session_selector().await;
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
            UiRequest::SelectAgentView { agent_id, respond } => {
                let selected = self.select_agent_view_from_bridge(&agent_id);
                let _ = respond.send(selected);
            }
            UiRequest::Select {
                title,
                options,
                timeout,
                respond,
            } => {
                let respond = Rc::new(RefCell::new(Some(respond)));
                let select_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |option| {
                    respond_once(&select_respond, Some(option));
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let cancel_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    respond_once(&cancel_respond, None);
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
                let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |option| {
                    respond_once(&yes_respond, option == "Yes");
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let cancel_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    respond_once(&cancel_respond, false);
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let selector = ExtensionSelectorComponent::new(
                    &pi_js_compat::js_format!("{}\n{}", title, message),
                    vec!["Yes".into(), "No".into()],
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
                let on_submit: Rc<dyn Fn(JsString)> = Rc::new(move |value| {
                    respond_once(&submit_respond, Some(value));
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let cancel_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    respond_once(&cancel_respond, None);
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let input = ExtensionInputComponent::new(
                    &title,
                    placeholder.as_ref(),
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
                let on_submit: Rc<dyn Fn(JsString)> = Rc::new(move |value| {
                    respond_once(&submit_respond, Some(value));
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let cancel_respond = Rc::clone(&respond);
                let mode = self.ui_id.get();
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    respond_once(&cancel_respond, None);
                    with_mode(mode, |mode| mode.restore_editor());
                });
                let editor = ExtensionEditorComponent::new(
                    Rc::new(InteractiveTuiHost(Rc::clone(&self.ui))),
                    self.keybindings.clone(),
                    &title,
                    prefill.as_ref(),
                    on_submit,
                    on_cancel,
                    EditorOptions::default(),
                    Some(self.settings(|settings| settings.get_external_editor_command()).into()),
                );
                self.show_selector(component(editor));
            }
            UiRequest::Notify { message, notify_type } => match notify_type {
                Some(NotifyType::Error) => self.show_error(&message),
                Some(NotifyType::Warning) => self.show_warning(&message),
                Some(NotifyType::Info) | None => self.show_status(&message),
            },
            UiRequest::RequestRender => self.ui.request_render(),
            UiRequest::RequestRenderForce => self.ui.request_render_force(),
            UiRequest::SetStatus { key, text } => {
                if let Some(text) = &text {
                    self.extension_statuses.borrow_mut().insert(key.clone(), text.clone());
                } else {
                    self.extension_statuses.borrow_mut().shift_remove(&key);
                }
                self.footer_data_provider.set_extension_status(&key, text.as_ref());
                self.footer.borrow_mut().invalidate();
                self.ui.request_render();
            }
            UiRequest::SetWorkingMessage(message) => {
                *self.working_message.borrow_mut() = message;
                if let Some(loader) = self.loading_animation.borrow().as_ref() {
                    set_loader_message(loader, self.get_working_loader_message());
                }
                self.ui.request_render();
            }
            UiRequest::SetWorkingVisible(visible) => self.set_working_visible(visible),
            UiRequest::SetWorkingIndicator(options) => self.set_working_indicator(options),
            UiRequest::SetHiddenThinkingLabel(label) => {
                *self.hidden_thinking_label.borrow_mut() = label.unwrap_or_else(|| "Thinking...".into());
                self.ui.request_render();
            }
            UiRequest::SetWidget {
                key,
                content,
                placement,
            } => self.add_extension_widget(
                &key,
                content.map(|lines| ExtensionWidget::Lines(Self::string_widget_lines(&lines))),
                placement,
            ),
            UiRequest::SetWidgetComponent {
                key,
                content,
                placement,
            } => {
                let widget = content.map(|factory| {
                    let component_ref = component(ExtensionComponentAdapter {
                        inner: factory(self.extension_factory_context()).into(),
                    });
                    ExtensionWidget::Component(component_ref)
                });
                self.add_extension_widget(&key, widget, placement);
            }
            UiRequest::SetTitle(title) => self.ui.terminal().borrow_mut().set_title(&title),
            UiRequest::PasteToEditor(text) => {
                self.editor()
                    .borrow_mut()
                    .handle_input(&pi_js_compat::js_format!("\x1b[200~{}\x1b[201~", text));
                self.ui.request_render();
            }
            UiRequest::SetEditorText(text) => {
                self.editor().borrow_mut().set_text(&text);
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
            } => self.show_extension_custom(Rc::new(move |context, done| factory(context, done)), options, respond),
            UiRequest::AddTerminalInput { handler, respond } => {
                let handler = Arc::new(handler);
                let registered: TerminalInputHandler = Arc::clone(&handler);
                let id = self.ui.add_input_listener(Rc::new(move |data: &JsString| {
                    let result = handler(data);
                    match result {
                        Some(result) => TuiInputListenerResult {
                            consume: result.consume.unwrap_or(false),
                            data: result.data,
                        },
                        None => TuiInputListenerResult::default(),
                    }
                }));
                self.terminal_input_listeners.borrow_mut().push((id, registered));
                let _ = respond.send(id);
            }
            UiRequest::RemoveTerminalInput(id) => {
                self.ui.remove_input_listener(id);
                self.terminal_input_listeners
                    .borrow_mut()
                    .retain(|(listener_id, _)| *listener_id != id);
            }
            UiRequest::AddAutocompleteProvider(factory) => {
                self.autocomplete_provider_wrappers.borrow_mut().push(factory);
                self.setup_autocomplete_provider();
            }
            UiRequest::SetEditorComponent(factory) => self.set_custom_editor_component(factory),
            UiRequest::AbortAndRestoreQueued => {
                self.restore_queued_messages_to_editor(true);
            }
            UiRequest::ShutdownRequested => {
                self.shutdown_requested.set(true);
            }
            UiRequest::ShowApprovalSelector => {
                self.show_approval_selector();
            }
            UiRequest::ShowSandboxSelector => {
                self.show_sandbox_selector();
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
                if let Ok(result) = &result {
                    self.adopt_transitioned_session(&runtime, result.cancelled);
                }
                let transition = match result {
                    Ok(result) => Ok(SessionTransitionResult {
                        cancelled: result.cancelled,
                    }),
                    Err(error) => self.handle_fatal_runtime_error(&JsString::from("Failed to create session"), error),
                };
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
                if let Ok(result) = &result {
                    self.adopt_transitioned_session(&runtime, result.cancelled);
                }
                let transition = match result {
                    Ok(result) => Ok(SessionTransitionResult {
                        cancelled: result.cancelled,
                    }),
                    Err(error) => self.handle_fatal_runtime_error(&JsString::from("Failed to fork session"), error),
                };
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
                    .map(|_| SessionTransitionResult { cancelled: false })
                    .map_err(|error| error.message().to_owned());
                let _ = respond.send(transition);
            }
            UiRequest::RuntimeSwitchSession { path, respond } => {
                let mut runtime = self.runtime.lock().await;
                let result = runtime.switch_session(&path, None, None).await;
                if let Ok(result) = &result {
                    self.adopt_transitioned_session(&runtime, result.cancelled);
                }
                let transition = result
                    .map(|result| SessionTransitionResult {
                        cancelled: result.cancelled,
                    })
                    .map_err(JsString::from);
                let _ = respond.send(transition);
            }
            UiRequest::RuntimeReload { respond } => {
                let _ = self.session().reload(None).await;
                let _ = respond.send(());
            }
            UiRequest::RuntimeRestart { options, respond } => {
                let mut runtime = self.runtime.lock().await;
                let result = runtime
                    .restart(options.as_ref().and_then(|options| options.notice.as_ref()), true)
                    .await
                    .map_err(crate::core::extensions::types::HandlerError::new);
                let _ = respond.send(result);
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
                mode_id: self.ui_id.get(),
                shared: Arc::clone(&self.ui_shared),
                sender: self.ui_sender.clone(),
            })),
            theme: crate::modes::interactive::theme::current_theme(),
            footer_data: (*self.footer_data_provider).clone(),
        };
        component(ExtensionComponentAdapter {
            inner: factory(context).into(),
        })
    }

    fn extension_factory_context(&self) -> crate::core::extensions::types::ExtensionUiFactoryContext {
        crate::core::extensions::types::ExtensionUiFactoryContext {
            tui: ExtensionUiTui::new(Arc::new(InteractiveUiTuiBackend {
                mode_id: self.ui_id.get(),
                shared: Arc::clone(&self.ui_shared),
                sender: self.ui_sender.clone(),
            })),
            theme: crate::modes::interactive::theme::current_theme(),
            keybindings: self.keybindings.clone(),
        }
    }

    fn set_extension_header(&self, factory: Option<ExtensionUiComponentFactory>) {
        self.custom_header.borrow_mut().take();
        self.header_container.borrow_mut().clear();
        match factory {
            Some(factory) => {
                let component_ref = component(ExtensionComponentAdapter {
                    inner: factory(self.extension_factory_context()).into(),
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
        factory: LocalExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
        respond: tokio::sync::oneshot::Sender<Result<Option<JsValue>, JsString>>,
    ) {
        let context = self.extension_factory_context();
        let respond = Arc::new(std::sync::Mutex::new(Some(respond)));
        let done_respond = Arc::clone(&respond);
        let component_slot = Rc::new(RefCell::new(None::<std::rc::Weak<dyn ExtensionUiComponent>>));
        let done_component = Rc::clone(&component_slot);
        let mode = self.ui_id.get();
        let overlay = options.as_ref().and_then(|options| options.overlay).unwrap_or(false);
        let saved_text = self.editor().borrow().get_text();
        let done_text = saved_text.clone();
        let closed = Rc::new(Cell::new(false));
        let done_closed = closed.clone();
        let done = ExtensionUiDone::new(Rc::new(move |value| {
            if done_closed.replace(true) {
                return;
            }
            let respond = done_respond
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let component = done_component.borrow().as_ref().and_then(std::rc::Weak::upgrade);
            with_mode(mode, |mode| {
                if overlay {
                    mode.ui.hide_overlay();
                } else {
                    mode.restore_editor();
                    mode.editor().borrow_mut().set_text(&done_text);
                }
            });
            if let Some(respond) = respond {
                let _ = respond.send(Ok(value));
            }
            if let Some(component) = component {
                component.dispose();
            }
        }));
        let pending = match factory(context, done) {
            Ok(pending) => pending,
            Err(error) => {
                // A synchronous TS factory throw rejects before the chained
                // async catch: no mounting, restoration, or disposal occurs.
                if let Some(respond) = respond.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
                    let _ = respond.send(Err(error));
                }
                return;
            }
        };
        tokio::task::spawn_local(async move {
            let result = pending.await;
            if closed.get() {
                return;
            }
            match result {
                Ok(inner) => {
                    with_mode(mode, |mode| {
                        mode.mount_extension_custom(inner.into(), options, component_slot)
                    });
                }
                Err(error) => {
                    with_mode(mode, |mode| {
                        if !overlay {
                            mode.restore_editor();
                            mode.editor().borrow_mut().set_text(&saved_text);
                        }
                    });
                    if let Some(respond) = respond.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
                        let _ = respond.send(Err(error));
                    }
                }
            }
        });
    }

    fn mount_extension_custom(
        &self,
        inner: Rc<dyn ExtensionUiComponent>,
        options: Option<ExtensionUiCustomOptions>,
        component_slot: Rc<RefCell<Option<std::rc::Weak<dyn ExtensionUiComponent>>>>,
    ) {
        let overlay = options.as_ref().and_then(|options| options.overlay).unwrap_or(false);
        *component_slot.borrow_mut() = Some(Rc::downgrade(&inner));
        let overlay_options = if overlay {
            options
                .as_ref()
                .and_then(|options| options.overlay_options.as_ref())
                .map(|resolve| resolve().into())
                .or_else(|| {
                    inner
                        .width()
                        .filter(|width| !matches!(width, pi_tui::tui::SizeValue::Absolute(0.0)))
                        .map(|width| OverlayOptions {
                            width: Some(width),
                            ..Default::default()
                        })
                })
        } else {
            None
        };
        let component_ref = component(ExtensionComponentAdapter {
            inner: Rc::clone(&inner),
        });
        if overlay {
            let handle: Rc<dyn crate::core::extensions::ExtensionUiOverlayHandle> = Rc::new(InteractiveOverlayHandle(
                self.ui.show_overlay(component_ref, overlay_options),
            ));
            inner.on_overlay_handle(Rc::clone(&handle));
            if let Some(on_handle) = options.and_then(|options| options.on_handle) {
                on_handle(handle);
            }
        } else {
            self.show_selector(component_ref);
        }
    }

    /// `handleHiddenMainSessionDisplayEvent`.
    fn handle_hidden_main_session_display_event(&self, event: &AgentSessionEvent) -> bool {
        if !self.is_viewing_agent_session() || !is_main_session_display_event(event) {
            return false;
        }
        self.sync_tool_execution_tracking_for_hidden_main_event(event);
        if !matches!(event, AgentSessionEvent::Agent(AgentEvent::MessageUpdate { .. })) {
            self.ui.request_render();
        }
        true
    }

    /// `syncToolExecutionTrackingForHiddenMainEvent`.
    fn sync_tool_execution_tracking_for_hidden_main_event(&self, event: &AgentSessionEvent) {
        match event {
            AgentSessionEvent::Agent(AgentEvent::AgentStart | AgentEvent::AgentEnd { .. }) => {
                self.executing_tool_names.borrow_mut().clear();
                self.executing_tool_started_at.borrow_mut().clear();
                self.stop_tool_waiting_timer_if_idle();
                self.set_working_message_for_active_tools();
            }
            AgentSessionEvent::Agent(AgentEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                started_at,
                ..
            }) => {
                self.executing_tool_names
                    .borrow_mut()
                    .insert(tool_call_id.clone().into(), tool_name.clone());
                self.executing_tool_started_at
                    .borrow_mut()
                    .insert((tool_call_id.clone()).into(), *started_at);
            }
            AgentSessionEvent::Agent(AgentEvent::ToolExecutionEnd {
                tool_call_id,
                started_at,
                finished_at,
                ..
            }) => {
                self.completed_tool_timings
                    .borrow_mut()
                    .insert((tool_call_id.clone()).into(), (*started_at, *finished_at));
                self.executing_tool_names.borrow_mut().shift_remove(tool_call_id);
                self.executing_tool_started_at.borrow_mut().shift_remove(tool_call_id);
                self.stop_tool_waiting_timer_if_idle();
                self.set_working_message_for_active_tools();
            }
            _ => {}
        }
    }

    async fn handle_event(&self, event: AgentSessionEvent) {
        let (event, runtime_message_marker) = match event {
            AgentSessionEvent::Message(message) => {
                (AgentSessionEvent::Agent(message.event), message.runtime_message_marker)
            }
            other => (other, None),
        };
        if self.handle_hidden_main_session_display_event(&event) {
            return;
        }
        match event {
            AgentSessionEvent::Agent(AgentEvent::AgentStart) => {
                self.close_response_complete_notification();
                self.thinking_follows_tool.set(false);
                self.clear_pending_tools();
                self.executing_tool_names.borrow_mut().clear();
                self.executing_tool_started_at.borrow_mut().clear();
                self.stop_tool_waiting_timer_if_idle();
                self.stop_thinking_timer();
                self.set_default_working_message("Thinking...");
                if self.settings(|settings| settings.get_show_terminal_progress()) {
                    self.ui.terminal().borrow_mut().set_progress(true);
                }
                if let Some(handler) = self.retry_escape_handler.borrow_mut().take() {
                    self.default_editor.borrow_mut().on_escape = Some(handler);
                }
                self.retry_countdown.borrow_mut().take();
                self.stop_working_loader();
                if let Some(loader) = self.retry_loader.borrow_mut().take() {
                    set_loader_message(&loader, self.get_working_loader_message());
                    *self.loading_animation.borrow_mut() = Some(loader);
                    *self.working_loader_view.borrow_mut() = Some(WorkingLoaderView::Main);
                }
                self.sync_working_loader_visibility();
                self.ui.request_render();
            }
            AgentSessionEvent::Agent(AgentEvent::ModelRequestStart) => {
                self.start_thinking_timer();
                self.ui.request_render();
            }
            AgentSessionEvent::Agent(AgentEvent::ModelRequestEnd) => {
                self.stop_thinking_timer();
                self.ui.request_render();
            }
            AgentSessionEvent::Agent(AgentEvent::MessageStart { message }) => match &message {
                AgentMessage::Assistant(assistant) => {
                    self.set_working_message_for_active_tools();
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
                    if self.ui.mode() == TuiMode::Regular {
                        self.transcript_tail_container
                            .borrow_mut()
                            .add_child(Rc::clone(&component_ref));
                    } else {
                        self.chat_container.borrow_mut().add_child(Rc::clone(&component_ref));
                    }
                    *self.streaming_component.borrow_mut() = Some(component_ref);
                    *self.streaming_message.borrow_mut() = Some((**assistant).clone());
                    self.ui.request_render();
                }
                AgentMessage::Custom(_) => {
                    self.add_message_to_chat(&message, false);
                    self.ui.request_render();
                }
                AgentMessage::User(_) => {
                    if !self.consume_prepared_user_message(&message) {
                        self.add_message_to_chat(&message, false);
                    }
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
                self.stop_thinking_timer();
                self.set_default_working_message("Streaming...");
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
                    for content in &assistant.content {
                        if let pi_ai::types::AssistantContent::ToolCall(call) = content {
                            let existing = self.pending_tools.borrow().contains_key(&call.id);
                            if existing {
                                if let Some(component_ref) = self.pending_tools.borrow().get(&call.id).cloned()
                                    && let Some(component_value) = component_ref
                                        .borrow_mut()
                                        .as_any_mut()
                                        .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                                {
                                    component_value.update_args(JsValue::Object(call.arguments.clone()));
                                }
                            } else {
                                self.ensure_tool_component(
                                    &call.name,
                                    &call.id,
                                    JsValue::Object(call.arguments.clone()),
                                );
                            }
                        }
                    }
                    self.ui.request_render();
                }
            }
            AgentSessionEvent::Agent(AgentEvent::MessageEnd { message }) => {
                self.footer.borrow_mut().invalidate();
                if matches!(message, AgentMessage::User(_)) {
                    return;
                }
                if matches!(message, AgentMessage::Assistant(_))
                    && is_duplicate_turn_assistant_message(None, runtime_message_marker)
                {
                    self.discard_streaming_component();
                    self.ui.request_render();
                    return;
                }
                if let AgentMessage::Assistant(assistant) = &message
                    && self.streaming_component.borrow().is_some()
                {
                    let mut assistant = (**assistant).clone();
                    let mut error_message = None;
                    if assistant.stop_reason == pi_ai::types::StopReason::Aborted {
                        let retry_attempt = self.session().retry_attempt();
                        error_message = Some(if retry_attempt > 0 {
                            js_format!(
                                "Aborted after {} retry attempt{}",
                                retry_attempt,
                                if retry_attempt > 1 { "s" } else { "" }
                            )
                        } else {
                            JsString::from("Operation aborted")
                        });
                        assistant.error_message = error_message.clone();
                    }
                    *self.streaming_message.borrow_mut() = Some(assistant.clone());
                    if let Some(streaming) = self.streaming_component.borrow().as_ref()
                        && let Some(component_value) = streaming
                            .borrow_mut()
                            .as_any_mut()
                            .and_then(|any| any.downcast_mut::<AssistantMessageComponent>())
                    {
                        component_value.update_content(assistant.clone(), false);
                    }
                    self.move_streaming_component_to_chat();
                    self.add_completed_thinking_duration_before_tool_call(&assistant);

                    if matches!(
                        assistant.stop_reason,
                        pi_ai::types::StopReason::Aborted | pi_ai::types::StopReason::Error
                    ) {
                        let error_message = error_message
                            .or_else(|| assistant.error_message.clone())
                            .unwrap_or_else(|| JsString::from("Error"));
                        for component in self.pending_tools.borrow().values() {
                            if let Some(component_value) = component
                                .borrow_mut()
                                .as_any_mut()
                                .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                            {
                                component_value.update_result(
                                    ToolExecutionResult {
                                        content: vec![pi_ai::types::TextOrImageContent::Text(
                                            pi_ai::types::TextContent::new(error_message.clone()),
                                        )],
                                        details: None,
                                        is_error: true,
                                    },
                                    false,
                                    None,
                                );
                            }
                        }
                        self.clear_pending_tools();
                        self.executing_tool_names.borrow_mut().clear();
                        self.executing_tool_started_at.borrow_mut().clear();
                        self.stop_tool_waiting_timer_if_idle();
                    } else {
                        for content in &assistant.content {
                            if let pi_ai::types::AssistantContent::ToolCall(call) = content {
                                self.ensure_tool_component(
                                    &call.name,
                                    &call.id,
                                    JsValue::Object(call.arguments.clone()),
                                );
                            }
                        }
                        for component in self.pending_tools.borrow().values() {
                            if let Some(component_value) = component
                                .borrow_mut()
                                .as_any_mut()
                                .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                            {
                                component_value.set_args_complete();
                            }
                        }
                        self.maybe_show_cache_miss_notice(&assistant);
                    }
                    *self.streaming_component.borrow_mut() = None;
                    *self.streaming_message.borrow_mut() = None;
                }
                self.ui.request_render();
            }
            AgentSessionEvent::Agent(AgentEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                args,
                started_at,
            }) => {
                self.executing_tool_names
                    .borrow_mut()
                    .insert(tool_call_id.clone().into(), tool_name.clone());
                self.executing_tool_started_at
                    .borrow_mut()
                    .insert((tool_call_id.clone()).into(), started_at);
                self.ensure_tool_component(&tool_name, &tool_call_id, args);
                if let Some(component_ref) = self.pending_tools.borrow().get(&tool_call_id).cloned()
                    && let Some(component_value) = component_ref
                        .borrow_mut()
                        .as_any_mut()
                        .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                {
                    component_value.mark_execution_started(started_at);
                }
                self.start_tool_waiting_timer();
                self.set_working_message_for_active_tools();
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
                started_at,
                finished_at,
                ..
            }) => {
                self.completed_tool_timings
                    .borrow_mut()
                    .insert((tool_call_id.clone()).into(), (started_at, finished_at));
                let component_ref = self.pending_tools.borrow_mut().shift_remove(&tool_call_id);
                let had_component = component_ref.is_some();
                if let Some(component_ref) = component_ref
                    && let Some(component_value) = component_ref
                        .borrow_mut()
                        .as_any_mut()
                        .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                {
                    component_value.mark_execution_started(started_at);
                    component_value.update_result(
                        ToolExecutionResult {
                            content: result.content.clone(),
                            details: result.details.clone(),
                            is_error,
                        },
                        false,
                        Some(finished_at),
                    );
                }
                self.executing_tool_names.borrow_mut().shift_remove(&tool_call_id);
                self.executing_tool_started_at.borrow_mut().shift_remove(&tool_call_id);
                self.stop_tool_waiting_timer_if_idle();
                self.thinking_follows_tool.set(true);
                self.set_default_working_message("Thinking...");
                self.set_working_message_for_active_tools();
                if had_component {
                    self.ui.request_render();
                }
            }
            AgentSessionEvent::AgentEnd(event) => {
                if !event.will_retry {
                    self.notify_response_complete();
                }
                self.thinking_follows_tool.set(false);
                self.stop_thinking_timer();
                self.executing_tool_names.borrow_mut().clear();
                self.executing_tool_started_at.borrow_mut().clear();
                self.stop_tool_waiting_timer_if_idle();
                if self.settings(|settings| settings.get_show_terminal_progress()) {
                    self.ui.terminal().borrow_mut().set_progress(false);
                }
                self.stop_working_loader();
                crate::modes::interactive::working_editor::clear_working_editor(&mut *self.editor().borrow_mut());
                self.discard_streaming_component();
                self.clear_pending_tools();
                self.ui.request_render();
            }
            AgentSessionEvent::QueueUpdate(_) => {
                self.update_pending_messages_display();
                self.ui.request_render();
            }
            AgentSessionEvent::SteeringMessageQueued(_) => {
                if let Some(wake) = self.options.wake_wait_agents_after_steering.as_ref() {
                    wake();
                }
            }
            AgentSessionEvent::AgentSettled(_) => {}
            AgentSessionEvent::CompactionStart(event) => {
                if self.settings(|settings| settings.get_show_terminal_progress()) {
                    self.ui.terminal().borrow_mut().set_progress(true);
                }
                let previous_escape = self.default_editor.borrow().on_escape.clone();
                *self.auto_compaction_escape_handler.borrow_mut() = previous_escape;
                let session = self.session();
                self.default_editor.borrow_mut().on_escape = Some(Rc::new(move || {
                    session.abort_compaction();
                }));
                self.status_container.borrow_mut().clear();
                *self.auto_compaction_source_hint.borrow_mut() = event.source_hint.clone();
                let cancel_hint = js_format!(
                    "({} to cancel)",
                    crate::modes::interactive::components::keybinding_hints::key_text("app.interrupt")
                );
                let label = format_compaction_start_label(
                    &JsString::from(&event.reason),
                    event.source_hint.as_ref(),
                    &cancel_hint,
                );
                let ui = Rc::clone(&self.ui);
                let request_render: Rc<dyn Fn()> = Rc::new(move || {
                    ui.request_render();
                });
                let loader = component(Loader::new(
                    request_render,
                    Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Accent, text))),
                    Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
                    label,
                    Some(pi_tui::LoaderIndicatorOptions {
                        frames: Some(Vec::new()),
                        interval_ms: None,
                    }),
                ));
                *self.auto_compaction_loader.borrow_mut() = Some(Rc::clone(&loader));
                self.status_container.borrow_mut().add_child(loader);
                self.set_prompt_activity(PromptActivitySource::Compaction, true);
                self.ui.request_render();
            }
            AgentSessionEvent::CompactionEnd(event) => {
                if self.settings(|settings| settings.get_show_terminal_progress()) {
                    self.ui.terminal().borrow_mut().set_progress(false);
                }
                if let Some(handler) = self.auto_compaction_escape_handler.borrow_mut().take() {
                    self.default_editor.borrow_mut().on_escape = Some(handler);
                }
                if self.auto_compaction_loader.borrow().is_some() {
                    self.stop_auto_compaction_loader();
                    self.status_container.borrow_mut().clear();
                }
                self.set_prompt_activity(PromptActivitySource::Compaction, false);
                if event.aborted {
                    if event.reason == "manual" {
                        self.show_error("Compaction cancelled");
                    } else {
                        self.show_status("Auto-compaction cancelled");
                    }
                } else if let Some(result) = &event.result {
                    let source_log_message = format_compaction_source_log_message(result.source.as_ref());
                    self.rebuild_chat_after_compaction(result);
                    self.footer.borrow_mut().invalidate();
                    self.show_status(&source_log_message);
                } else if let Some(error) = &event.error_message {
                    let source_hint = self.auto_compaction_source_hint.borrow().clone();
                    let failure_message =
                        format_compaction_failure_message(error, &JsString::from(&event.reason), source_hint.as_ref());
                    if event.reason == "manual" {
                        self.show_error(&failure_message);
                    } else {
                        self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
                        self.chat_container.borrow_mut().add_child(component(Text::new(
                            with_theme(|theme| theme.fg(ThemeColor::Error, &(&failure_message).clone())),
                            1,
                            0,
                        )));
                    }
                }
                *self.auto_compaction_source_hint.borrow_mut() = None;
                self.flush_compaction_queue(event.will_retry).await;
                self.ui.request_render();
            }
            AgentSessionEvent::AutoRetryStart(event) => {
                let previous_escape = self.default_editor.borrow().on_escape.clone();
                *self.retry_escape_handler.borrow_mut() = previous_escape;
                let session = self.session();
                self.default_editor.borrow_mut().on_escape = Some(Rc::new(move || {
                    session.abort_retry();
                }));
                self.status_container.borrow_mut().clear();
                self.retry_countdown.borrow_mut().take();
                let attempt = event.attempt;
                let max_attempts = event.max_attempts;
                let retry_message = move |seconds: i64| {
                    js_format!(
                        "Retrying ({}/{}) in {}s... ({} to cancel)",
                        attempt,
                        max_attempts,
                        seconds,
                        crate::modes::interactive::components::keybinding_hints::key_text("app.interrupt")
                    )
                };
                let ui = Rc::clone(&self.ui);
                let request_render: Rc<dyn Fn()> = Rc::new(move || {
                    ui.request_render();
                });
                let loader = component(Loader::new(
                    Rc::clone(&request_render),
                    Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Warning, text))),
                    Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
                    retry_message((event.delay_ms / 1000.0).ceil() as i64),
                    Some(self.get_working_loader_indicator(WorkingLoaderView::Main)),
                ));
                *self.retry_loader.borrow_mut() = Some(Rc::clone(&loader));
                let tick_loader = Rc::clone(&loader);
                let countdown = CountdownTimer::new(
                    event.delay_ms.max(0.0) as u64,
                    Some(request_render),
                    Rc::new(move |seconds| set_loader_message(&tick_loader, retry_message(seconds))),
                    Rc::new(|| {}),
                );
                *self.retry_countdown.borrow_mut() = Some(countdown);
                self.status_container.borrow_mut().add_child(loader);
                self.set_prompt_activity(PromptActivitySource::Retry, true);
                self.ui.request_render();
            }
            AgentSessionEvent::AutoRetryEnd(event) => {
                if let Some(handler) = self.retry_escape_handler.borrow_mut().take() {
                    self.default_editor.borrow_mut().on_escape = Some(handler);
                }
                self.retry_countdown.borrow_mut().take();
                self.stop_retry_loader();
                self.set_prompt_activity(PromptActivitySource::Retry, false);
                if !event.success {
                    self.show_error(&js_format!(
                        "Retry failed after {} attempts: {}",
                        event.attempt,
                        event.final_error.as_ref().unwrap_or(&JsString::from("Unknown error"))
                    ));
                }
                self.ui.request_render();
            }
            AgentSessionEvent::ProviderStreamRetry(event) => {
                let retry = &event.retry;
                self.chat_container.borrow_mut().add_child(component(Text::new(
                    with_theme(|theme| theme.fg(ThemeColor::Warning, &js_format!("provider retry: {}", retry.reason))),
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
            AgentSessionEvent::EntryAppended(event) => {
                if let Some(KnownSessionEntry::Custom(custom)) = event.entry.known().as_deref()
                    && self.add_custom_entry_to_chat(custom)
                {
                    self.ui.request_render();
                }
            }
            AgentSessionEvent::BashExecutionUpdate(_) => {}
            AgentSessionEvent::Agent(AgentEvent::AgentEnd { .. }) => {}
            AgentSessionEvent::Agent(AgentEvent::TurnStart) => {}
            AgentSessionEvent::Agent(AgentEvent::TurnEnd { .. }) => {}
            AgentSessionEvent::Message(_) => {}
        }
    }

    fn ensure_tool_component(&self, tool_name: &str, tool_call_id: &str, args: JsValue) {
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
        let component_ref = self.create_tool_component(tool_name, tool_call_id, args, &self.cwd());
        self.chat_container.borrow_mut().add_child(Rc::clone(&component_ref));
        self.pending_tools
            .borrow_mut()
            .insert((tool_call_id.to_owned()).into(), component_ref);
    }

    /// Builds a tool execution component without mounting it.
    fn create_tool_component(
        &self,
        tool_name: &str,
        tool_call_id: &str,
        args: JsValue,
        cwd: &JsString,
    ) -> ComponentRef {
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
            cwd,
        );
        component_value.set_expanded(self.tool_output_expanded.get());
        component(component_value)
    }

    fn clear_pending_tools(&self) {
        self.pending_tools.borrow_mut().clear();
    }

    fn notify_response_complete(&self) {
        self.close_response_complete_notification();
        let notification = crate::core::desktop_notification::send_desktop_notification(
            &crate::core::desktop_notification::DesktopNotification {
                title: JsString::from("Pi response complete"),
                body: JsString::from("Pi is idle and ready for your next message."),
                urgency: Some(crate::core::desktop_notification::DesktopNotificationUrgency::Normal),
                expire_time_ms: Some(crate::core::desktop_notification::PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS),
            },
        );
        *self.response_complete_notification.borrow_mut() = notification;
    }

    /// `clearTranscriptTail`.
    fn clear_transcript_tail(&self) {
        if let Some(region) = self.transcript_tail_region.borrow().as_ref() {
            region.clear();
        }
        self.transcript_tail_container.borrow_mut().clear();
    }

    /// `moveStreamingComponentToChat`.
    fn move_streaming_component_to_chat(&self) {
        let Some(component_ref) = self.streaming_component.borrow().clone() else {
            return;
        };
        self.clear_transcript_tail();
        if !self
            .chat_container
            .borrow()
            .children()
            .iter()
            .any(|child| Rc::ptr_eq(child, &component_ref))
        {
            self.chat_container.borrow_mut().add_child(component_ref);
        }
    }

    /// `discardStreamingComponent`.
    fn discard_streaming_component(&self) {
        if self.streaming_component.borrow().is_none() {
            return;
        }
        self.clear_transcript_tail();
        *self.streaming_component.borrow_mut() = None;
        *self.streaming_message.borrow_mut() = None;
    }

    /// `maybeShowCacheMissNotice`.
    fn maybe_show_cache_miss_notice(&self, message: &AssistantMessage) {
        if !self.settings(|settings| settings.get_show_cache_miss_notices()) {
            return;
        }
        let session = self.session();
        let entries = session.session_manager().get_entries();
        let prices = ModelRuntimePrices(session.model_runtime());
        if let Some(miss) = crate::core::cache_stats::detect_cache_miss(&entries, message, &prices) {
            self.add_cache_miss_notice(miss);
        }
    }

    /// `addCacheMissNotice`.
    fn add_cache_miss_notice(&self, miss: crate::core::cache_stats::CacheMiss) {
        if miss.missed_tokens < 20_000.0 && miss.missed_cost < 0.1 {
            return;
        }
        let cost = if miss.missed_cost >= 0.01 {
            js_format!(" (~${})", pi_js_compat::number::to_fixed(miss.missed_cost, 2))
        } else {
            JsString::default()
        };
        let re_billed = js_format!("{} tokens re-billed{}", format_tokens(miss.missed_tokens), cost);
        let mut label = JsString::from("Cache miss");
        if miss.model_changed {
            label = JsString::from("Cache miss after model switch");
        } else if miss.idle_ms >= crate::core::cache_stats::CACHE_TTL_MS as f64 {
            label = js_format!("Cache miss after {}m idle", (miss.idle_ms / 60_000.0).round() as i64);
        }
        let text = with_theme(|theme| theme.fg(ThemeColor::Warning, &js_format!("{}: {}", label, re_billed)));
        self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
        self.chat_container
            .borrow_mut()
            .add_child(component(Text::new(text, 1, 0)));
    }
}

/// `detectCacheMiss` price source backed by the session model runtime.
struct ModelRuntimePrices<'a>(&'a crate::core::model_runtime::ModelRuntime);

impl crate::core::cache_stats::ModelPriceSource for ModelRuntimePrices<'_> {
    fn get_model(&self, provider: &str, model_id: &str) -> Option<crate::core::cache_stats::ModelPrice> {
        self.0
            .get_model(provider, model_id)
            .map(|model| crate::core::cache_stats::ModelPrice {
                cache_read: model.cost.rates.cache_read,
            })
    }
}

/// `formatCompactionStartLabel`.
pub fn format_compaction_start_label(
    reason: &JsString,
    source: Option<&crate::core::compaction::CompactionSourceInfo>,
    cancel_hint: &JsString,
) -> JsString {
    let prefix = if reason == "manual" {
        "Compacting context"
    } else {
        "Auto-compacting"
    };
    let overflow_prefix = if reason == "overflow" {
        "Context overflow detected, "
    } else {
        ""
    };
    js_format!(
        "{}{}{}... {}",
        overflow_prefix,
        prefix,
        format_compaction_source_progress_suffix(source),
        cancel_hint
    )
}

fn format_compaction_source_progress_suffix(
    source: Option<&crate::core::compaction::CompactionSourceInfo>,
) -> JsString {
    use crate::core::compaction::CompactionSourceType;
    let Some(source) = source else {
        return JsString::default();
    };
    match source.r#type {
        CompactionSourceType::OpenAiRemote => {
            let endpoint_suffix = source
                .endpoint
                .as_ref()
                .map(|endpoint| js_format!(", {}", endpoint))
                .unwrap_or_default();
            js_format!(
                " via OpenAI remote endpoint ({}/{}{})",
                source.provider,
                source.model,
                endpoint_suffix
            )
        }
        CompactionSourceType::Local => JsString::from(" locally"),
    }
}

/// `formatCompactionFailureMessage`.
pub fn format_compaction_failure_message(
    error_message: impl Into<JsString>,
    reason: &JsString,
    source_hint: Option<&crate::core::compaction::CompactionSourceInfo>,
) -> JsString {
    use crate::core::compaction::CompactionSourceType;
    let error_message = error_message.into();
    let error_message = &error_message;
    let original_error = format_compaction_original_error(error_message, reason);
    let mut lines = vec![format_compaction_failure_summary(error_message, reason)];
    if let Some(source) = source_hint
        && source.r#type == CompactionSourceType::Local
    {
        lines.push(js_format!("Model: {}/{}.", source.provider, source.model).into());
    }
    if is_compaction_timeout(error_message) {
        lines.push("The compaction request did not finish before the timeout.".into());
    }
    lines.push(format_compaction_save_impact(reason));
    lines.push(JsString::from("Original error: ") + format_compaction_sentence(&original_error));
    JsString::join(lines, "\n")
}

fn is_compaction_timeout(error_message: &JsString) -> bool {
    error_message.to_lowercase().contains("timed out")
}

fn format_compaction_save_impact(reason: &JsString) -> JsString {
    if reason == "manual" {
        "No compaction was saved; context remains unchanged.".into()
    } else {
        "No compaction was saved; the previous context is still too large.".into()
    }
}

fn format_compaction_sentence(text: &JsString) -> JsString {
    if text.ends_with(".") || text.ends_with("!") || text.ends_with("?") {
        text.into()
    } else {
        text.clone() + "."
    }
}

fn format_compaction_failure_summary(error_message: &JsString, reason: &JsString) -> JsString {
    if is_compaction_timeout(error_message) {
        return if reason == "overflow" {
            "Context overflow recovery failed after compaction timeout."
        } else {
            "Compaction failed after timeout."
        }
        .into();
    }
    if reason == "overflow" {
        "Context overflow recovery failed during compaction."
    } else {
        "Compaction failed."
    }
    .into()
}

fn format_compaction_original_error(error_message: &JsString, reason: &JsString) -> JsString {
    let prefixes: &[&JsString] = if reason == "overflow" {
        &[&JsString::from("Context overflow recovery failed: ")]
    } else {
        &[
            &JsString::from("Compaction failed: "),
            &JsString::from("Auto-compaction failed: "),
        ]
    };
    for prefix in prefixes {
        if let Some(rest) = error_message.strip_prefix(*prefix) {
            return rest.into();
        }
    }
    error_message.into()
}

/// `formatCompactionSourceLogMessage`.
fn format_compaction_source_log_message(source: Option<&crate::core::compaction::CompactionSourceInfo>) -> JsString {
    use crate::core::compaction::CompactionSourceType;
    let Some(source) = source else {
        return JsString::from("Compaction completed locally");
    };
    match source.r#type {
        CompactionSourceType::OpenAiRemote => {
            let endpoint_suffix = source
                .endpoint
                .as_ref()
                .map(|endpoint| js_format!(", {}", endpoint))
                .unwrap_or_default();
            js_format!(
                "Compaction completed via OpenAI remote endpoint ({}/{}{})",
                source.provider,
                source.model,
                endpoint_suffix
            )
        }
        CompactionSourceType::Local => {
            js_format!("Compaction completed locally ({}/{})", source.provider, source.model)
        }
    }
}

/// `checkTmuxKeyboardSetup`; returns the warning to show, if any.
pub async fn check_tmux_keyboard_setup() -> Option<JsString> {
    let tmux_env = std::env::var_os("TMUX");
    check_tmux_keyboard_setup_with(tmux_env.as_deref(), &"tmux".into()).await
}

/// Runs the tmux keyboard check against an explicit environment and program.
pub async fn check_tmux_keyboard_setup_with(
    tmux_env: Option<&std::ffi::OsStr>,
    program: &JsString,
) -> Option<JsString> {
    tmux_env?;
    let keys_option = JsString::from("extended-keys");
    let format_option = JsString::from("extended-keys-format");
    let (extended_keys, extended_keys_format) = tokio::join!(
        run_tmux_show(program, &keys_option),
        run_tmux_show(program, &format_option),
    );
    tmux_keyboard_warning(extended_keys.as_ref(), extended_keys_format.as_ref())
}

fn tmux_keyboard_warning(
    extended_keys: Option<&JsString>,
    extended_keys_format: Option<&JsString>,
) -> Option<JsString> {
    let extended_keys = extended_keys?;
    if extended_keys != "on" && extended_keys != "always" {
        return Some(
            ("tmux extended-keys is off. Modified Enter keys may not work. Add `set -g extended-keys on` to ~/.tmux.conf and restart tmux."
                .to_owned()).into(),
        );
    }
    if extended_keys_format == Some(&JsString::from("xterm")) {
        return Some(
            ("tmux extended-keys-format is xterm. Pi works best with csi-u. Add `set -g extended-keys-format csi-u` to ~/.tmux.conf and restart tmux."
                .to_owned()).into(),
        );
    }
    None
}

async fn run_tmux_show(program: &JsString, option: &JsString) -> Option<JsString> {
    let child = tokio::process::Command::new(encode_node_utf8(program))
        .args(["show", "-gv"])
        .arg(encode_node_utf8(option))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let output = tokio::time::timeout(Duration::from_millis(2_000), child.wait_with_output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some((pi_js_compat::string::js_trim(&String::from_utf8_lossy(&output.stdout)).to_owned()).into())
}

/// `isMainSessionDisplayEvent`.
fn is_main_session_display_event(event: &AgentSessionEvent) -> bool {
    matches!(
        event,
        AgentSessionEvent::Agent(
            AgentEvent::AgentStart
                | AgentEvent::AgentEnd { .. }
                | AgentEvent::MessageStart { .. }
                | AgentEvent::MessageUpdate { .. }
                | AgentEvent::MessageEnd { .. }
                | AgentEvent::ModelRequestStart
                | AgentEvent::ModelRequestEnd
                | AgentEvent::ToolExecutionStart { .. }
                | AgentEvent::ToolExecutionUpdate { .. }
                | AgentEvent::ToolExecutionEnd { .. }
        ) | AgentSessionEvent::CompactionStart(_)
            | AgentSessionEvent::CompactionEnd(_)
            | AgentSessionEvent::EntryAppended(_)
            | AgentSessionEvent::QueueUpdate(_)
    )
}

#[cfg(test)]
mod ui_tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    use pi_agent_core::agent::{Agent, AgentInitialState, AgentOptions};
    use pi_agent_core::harness::types::ThinkingLevel;
    use pi_ai::providers::faux::{
        FauxModelDefinition, RegisterFauxProviderOptions, faux_assistant_message, faux_provider, faux_text,
    };
    use pi_ai::request::SimpleStreamOptions;
    use pi_ai::types::{Context, Model};
    use pi_tui::terminal::{Terminal, TerminalEvent};

    use super::*;
    use crate::core::agent_session::{AgentSession, AgentSessionConfig};
    use crate::core::agent_session_runtime::{
        CreateAgentSessionRuntimeOptions, CreateAgentSessionRuntimeResult, create_agent_session_runtime,
        share_agent_session_runtime,
    };
    use crate::core::agent_session_services::AgentSessionServices;
    use crate::core::auth_storage::AuthStorage;
    use crate::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
    use crate::core::resource_loader::StaticResourceLoader;
    use crate::core::session_manager::SessionManager;
    use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};

    struct CaptureTerminal {
        rows: usize,
        columns: usize,
        writes: Vec<JsString>,
        events: Vec<TerminalEvent>,
        progress: Vec<bool>,
        start_error: Option<TerminalRawModeError>,
    }

    impl CaptureTerminal {
        fn send_input(&mut self, data: &str) {
            self.events.push(TerminalEvent::Input(JsString::from(data.to_owned())));
        }

        fn output(&self) -> JsString {
            pi_tui::utils::strip_terminal_sequences(JsString::join(&self.writes, ""))
        }
    }

    impl Terminal for CaptureTerminal {
        fn start(&mut self) -> Result<(), TerminalRawModeError> {
            match &self.start_error {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        }
        fn stop(&mut self) {}
        fn drain_input(&mut self, _max_ms: u64, _idle_ms: u64) {}
        fn write(&mut self, data: &JsString) {
            self.writes.push(data.clone());
        }
        fn columns(&self) -> usize {
            self.columns
        }
        fn rows(&self) -> usize {
            self.rows
        }
        fn kitty_protocol_active(&self) -> bool {
            false
        }
        fn move_by(&mut self, _lines: isize) {}
        fn hide_cursor(&mut self) {}
        fn show_cursor(&mut self) {}
        fn clear_line(&mut self) {}
        fn clear_from_cursor(&mut self) {}
        fn clear_screen(&mut self) {}
        fn set_title(&mut self, _title: &JsString) {}
        fn set_progress(&mut self, active: bool) {
            self.progress.push(active);
        }
        fn poll_events(&mut self) -> Vec<TerminalEvent> {
            std::mem::take(&mut self.events)
        }
    }

    async fn build_mode() -> (
        Rc<InteractiveMode>,
        Rc<RefCell<CaptureTerminal>>,
        Arc<pi_ai::providers::faux::FauxProviderHandle>,
        tempfile::TempDir,
    ) {
        build_mode_with_loader(
            StaticResourceLoader::new() as Arc<dyn crate::core::resource_loader::ResourceLoader>,
            None,
        )
        .await
    }

    async fn build_mode_with_loader(
        loader: Arc<dyn crate::core::resource_loader::ResourceLoader>,
        initial_active_tool_names: Option<Vec<String>>,
    ) -> (
        Rc<InteractiveMode>,
        Rc<RefCell<CaptureTerminal>>,
        Arc<pi_ai::providers::faux::FauxProviderHandle>,
        tempfile::TempDir,
    ) {
        build_mode_with_settings(
            loader,
            initial_active_tool_names,
            SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default()),
        )
        .await
    }

    async fn build_mode_with_settings(
        loader: Arc<dyn crate::core::resource_loader::ResourceLoader>,
        initial_active_tool_names: Option<Vec<String>>,
        settings: SettingsManager,
    ) -> (
        Rc<InteractiveMode>,
        Rc<RefCell<CaptureTerminal>>,
        Arc<pi_ai::providers::faux::FauxProviderHandle>,
        tempfile::TempDir,
    ) {
        build_mode_with_extra_providers(
            loader,
            initial_active_tool_names,
            settings,
            Vec::new(),
            serde_json::Map::new(),
            false,
            TestModeSetup::default(),
        )
        .await
    }

    async fn build_mode_with_persisted_session() -> (
        Rc<InteractiveMode>,
        Rc<RefCell<CaptureTerminal>>,
        Arc<pi_ai::providers::faux::FauxProviderHandle>,
        tempfile::TempDir,
    ) {
        build_mode_with_extra_providers(
            StaticResourceLoader::new(),
            None,
            SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default()),
            Vec::new(),
            serde_json::Map::new(),
            true,
            TestModeSetup::default(),
        )
        .await
    }

    #[derive(Default)]
    struct TestModeSetup {
        multi_agent_store: Option<Arc<MultiAgentStore>>,
    }

    async fn build_mode_with_extra_providers(
        loader: Arc<dyn crate::core::resource_loader::ResourceLoader>,
        initial_active_tool_names: Option<Vec<String>>,
        settings: SettingsManager,
        extra_providers: Vec<Arc<dyn pi_ai::models::Provider>>,
        extra_credentials: serde_json::Map<String, Value>,
        persist_session: bool,
        setup: TestModeSetup,
    ) -> (
        Rc<InteractiveMode>,
        Rc<RefCell<CaptureTerminal>>,
        Arc<pi_ai::providers::faux::FauxProviderHandle>,
        tempfile::TempDir,
    ) {
        init_dark_theme();
        let temp = tempfile::tempdir().expect("temp dir");
        let cwd = temp.path().display().to_string();
        let faux = Arc::new(faux_provider(RegisterFauxProviderOptions {
            provider: Some("faux".to_owned()),
            api: Some("faux".to_owned()),
            models: Some(vec![
                FauxModelDefinition {
                    id: "faux-1".to_owned(),
                    name: Some("Faux Model".to_owned()),
                    reasoning: Some(true),
                    input: None,
                    cost: None,
                    context_window: Some(100_000.0),
                    max_tokens: Some(4096.0),
                },
                FauxModelDefinition {
                    id: "faux-2".to_owned(),
                    name: Some("Faux Second Model".to_owned()),
                    reasoning: Some(false),
                    input: None,
                    cost: None,
                    context_window: Some(100_000.0),
                    max_tokens: Some(4096.0),
                },
            ]),
            ..Default::default()
        }));
        faux.set_responses(vec![
            faux_assistant_message(
                faux_text("hello from faux"),
                pi_ai::providers::faux::FauxAssistantMessageOptions::default(),
            )
            .into(),
        ]);
        let mut credentials = extra_credentials;
        credentials.insert(
            "faux".to_owned(),
            serde_json::json!({"type": "api_key", "key": "faux-key"}),
        );
        let model_runtime = ModelRuntime::create(CreateModelRuntimeOptions {
            credentials: Some(Arc::new(AuthStorage::in_memory(credentials))),
            models_path: Some(None),
            refresh_on_create: Some(false),
            ..Default::default()
        })
        .await
        .expect("model runtime");
        model_runtime.register_native_provider(Arc::clone(&faux.provider));
        for provider in extra_providers {
            model_runtime.register_native_provider(provider);
        }

        let factory = {
            let model_runtime = model_runtime.clone();
            let faux = faux.clone();
            let loader = Arc::clone(&loader);
            let settings = Arc::new(Mutex::new(settings));
            Arc::new(move |request: CreateAgentSessionRuntimeOptions| {
                let model_runtime = model_runtime.clone();
                let faux = faux.clone();
                let loader = Arc::clone(&loader);
                let settings = Arc::clone(&settings);
                let initial_active_tool_names = initial_active_tool_names.clone();
                Box::pin(async move {
                    let model = faux.get_model(None).expect("faux model");
                    let stream_runtime = model_runtime.clone();
                    let agent = Agent::new(AgentOptions {
                        initial_state: Some(AgentInitialState {
                            model: Some(model),
                            system_prompt: Some(JsString::default()),
                            thinking_level: Some(ThinkingLevel::Off),
                            tools: Some(Vec::new()),
                            messages: None,
                        }),
                        stream_fn: Some(Arc::new(
                            move |model: Model, context: Context, options: SimpleStreamOptions| {
                                let runtime = stream_runtime.clone();
                                Box::pin(async move { runtime.stream_simple(model, context, options) })
                            },
                        )),
                        ..Default::default()
                    })
                    .expect("agent");
                    let settings_manager = Arc::clone(&settings);
                    let session = AgentSession::new(AgentSessionConfig {
                        agent,
                        session_manager: Arc::new(Mutex::new(request.session_manager)),
                        settings_manager: Arc::clone(&settings_manager),
                        cwd: request.cwd.clone(),
                        scoped_models: Vec::new(),
                        resource_loader: Arc::clone(&loader),
                        model_runtime: model_runtime.clone(),
                        initial_active_tool_names,
                        allowed_tool_names: None,
                        excluded_tool_names: None,
                        permission_prompt_tool: None,
                        agent_dir: Some(request.agent_dir.clone()),
                        control_db_path: Some(js_format!("{}/control.sqlite", request.agent_dir)),
                        base_tools: Vec::new(),
                        custom_tools: Vec::new(),
                        custom_tool_definitions: Vec::new(),
                        multi_agent_runtime_role: None,
                        multi_agent_execution_capability: None,
                        thinking_phase_timeout_ms: None,
                        supervisor_decision_requester: None,
                        multi_agent_store: None,
                        multi_agent_agent_id: None,
                        multi_agent_parent_session_id: None,
                        multi_agent_requires_agent_id: false,
                        disable_runtime_coordination_inbound: false,
                        on_shared_channel_message_delivered: None,
                        extension_runner_ref: None,
                        session_start_event: None,
                    })
                    .map_err(|error| error.to_string())?;
                    Ok(CreateAgentSessionRuntimeResult {
                        session,
                        services: AgentSessionServices {
                            cwd: request.cwd,
                            agent_dir: request.agent_dir,
                            model_runtime,
                            settings_manager,
                            resource_loader: loader,
                            diagnostics: Vec::new(),
                        },
                        diagnostics: Vec::new(),
                        model_fallback_message: None,
                    })
                })
                    as pi_ai::request::BoxFuture<'static, Result<CreateAgentSessionRuntimeResult, JsString>>
            }) as crate::core::agent_session_runtime::CreateAgentSessionRuntimeFactory
        };
        let session_manager = if persist_session {
            let session_dir = temp.path().join("sessions").display().to_string();
            SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&session_dir)), None)
                .expect("persisted session manager")
        } else {
            SessionManager::in_memory(Some(&JsString::from(&cwd)), None).expect("session manager")
        };
        let runtime =
            create_agent_session_runtime(factory, &JsString::from(&cwd), &JsString::from(&cwd), session_manager)
                .await
                .expect("runtime");
        let runtime = share_agent_session_runtime(runtime).await;
        let terminal = Rc::new(RefCell::new(CaptureTerminal {
            rows: 24,
            columns: 80,
            writes: Vec::new(),
            events: Vec::new(),
            progress: Vec::new(),
            start_error: None,
        }));
        let terminal_trait: Rc<RefCell<dyn Terminal>> = terminal.clone();
        let mode = InteractiveMode::new(
            runtime,
            InteractiveModeOptions {
                multi_agent_store: setup.multi_agent_store,
                ..Default::default()
            },
            Some(terminal_trait),
        )
        .await
        .expect("interactive mode");
        (mode, terminal, faux, temp)
    }

    fn js_path(path: &Path) -> JsString {
        JsString::from(path.to_str().expect("utf-8 test path"))
    }

    fn init_dark_theme() {
        let theme_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("theme");
        let dark = crate::modes::interactive::theme::load_theme_from_path(&js_path(&theme_dir.join("dark.json")), None)
            .expect("dark theme");
        let light =
            crate::modes::interactive::theme::load_theme_from_path(&js_path(&theme_dir.join("light.json")), None)
                .expect("light theme");
        crate::modes::interactive::theme::set_registered_themes(vec![
            std::sync::Arc::new(dark),
            std::sync::Arc::new(light),
        ])
        .expect("register fixture themes");
        crate::modes::interactive::theme::init_theme(Some(&JsString::from("dark")), false).expect("dark theme");
    }

    async fn pump(
        mode: &Rc<InteractiveMode>,
        events: &mut tokio::sync::mpsc::UnboundedReceiver<AgentSessionEvent>,
        terminal: &Rc<RefCell<CaptureTerminal>>,
        predicate: impl Fn(&JsString) -> bool,
    ) -> JsString {
        let mut handled = 0usize;
        for _ in 0..200 {
            mode.ui.process_events();
            mode.ui.flush_pending();
            mode.poll_timers();
            mode.poll_child_view_events();
            mode.poll_multi_agent_updates();
            while let Ok(event) = events.try_recv() {
                handled += 1;
                mode.handle_event(event).await;
            }
            mode.handle_ui_requests().await;
            mode.drain_actions().await;
            tokio::time::sleep(Duration::from_millis(5)).await;
            let output = terminal.borrow().output();
            if predicate(&output) {
                return output;
            }
        }
        let chat_render = mode
            .chat_container
            .try_borrow_mut()
            .map(|mut chat| JsString::join(chat.render(80), "\n"))
            .unwrap_or_default();
        panic!(
            "timed out; messages={}; streaming={}; actions={}; events={handled:?}; chat={}; assistant={:?}; chat_render={chat_render:?}; output: {:?}",
            mode.session().messages().len(),
            mode.session().is_streaming(),
            mode.actions.borrow().len(),
            mode.chat_container.borrow().children().len(),
            mode.session().get_last_assistant_text(),
            terminal.borrow().output()
        );
    }

    async fn start(mode: &Rc<InteractiveMode>) -> tokio::sync::mpsc::UnboundedReceiver<AgentSessionEvent> {
        mode.bind_callbacks();
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        *mode.events.borrow_mut() = Some(sender);
        mode.init().await.expect("init");
        mode.subscribe_to_agent();
        receiver
    }

    #[tokio::test(flavor = "current_thread")]
    async fn renders_startup_screen_and_prompt_submission() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                let output = pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;
                assert!(output.contains("faux-1"));

                terminal.borrow_mut().send_input("hi");
                mode.ui.process_events();
                assert_eq!(mode.editor().borrow().get_text(), "hi");
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                assert!(
                    !mode.actions.borrow().is_empty(),
                    "submit action must be queued after Enter"
                );
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("hello from faux")
                })
                .await;
                assert!(
                    !mode.session().messages().is_empty(),
                    "session must contain the prompt and response"
                );
                assert!(output.contains("hello from faux"));
                assert!(output.contains("hi"));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn loaded_resources_show_context_rules_and_skills() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let loader = Arc::new(StaticResourceLoader {
                    skills: crate::core::resource_loader::LoadedSkills {
                        skills: vec![crate::core::skills::Skill {
                            name: JsString::from("commit"),
                            description: JsString::from("commit work"),
                            file_path: JsString::from("/tmp/skill/SKILL.md"),
                            base_dir: JsString::from("/tmp/skill"),
                            source_info: crate::core::source_info::SourceInfo {
                                path: JsString::from("/tmp/skill/SKILL.md"),
                                source: JsString::from("local"),
                                scope: crate::core::source_info::SourceScope::Project,
                                origin: crate::core::source_info::SourceOrigin::TopLevel,
                                base_dir: Some(JsString::from("/tmp/skill")),
                            },
                            disable_model_invocation: false,
                        }],
                        diagnostics: Vec::new(),
                    },
                    agents_files: crate::core::resource_loader::LoadedAgentsFiles {
                        agents_files: vec![crate::core::system_prompt::ContextFile {
                            path: JsString::from("/tmp/project/AGENTS.md"),
                            content: JsString::default(),
                        }],
                    },
                    rules_files: crate::core::resource_loader::LoadedRulesFiles {
                        rules_files: vec![crate::core::resource_loader::RuleFile {
                            path: JsString::from("/tmp/rules/01-identity.md"),
                            content: JsString::default(),
                        }],
                    },
                    ..StaticResourceLoader::default()
                }) as Arc<dyn crate::core::resource_loader::ResourceLoader>;
                let (mode, _terminal, _faux, _temp) = build_mode_with_loader(loader, None).await;
                let _events = start(&mode).await;
                mode.show_loaded_resources(false, false);
                let rendered = pi_tui::utils::strip_terminal_sequences(&JsString::join(
                    mode.loaded_resources_container.borrow_mut().render(220),
                    "\n",
                ));
                assert!(rendered.contains("[Context]"), "{rendered:?}");
                assert!(rendered.contains("AGENTS.md"), "{rendered:?}");
                assert!(rendered.contains("[User Rules]"), "{rendered:?}");
                assert!(rendered.contains("01-identity.md"), "{rendered:?}");
                assert!(rendered.contains("[Skills]"), "{rendered:?}");
                assert!(rendered.contains("commit"), "{rendered:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn bash_mode_executes_and_renders_command_output() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                terminal.borrow_mut().send_input("!printf bash-ok");
                terminal.borrow_mut().send_input("\r");
                let output = pump(&mode, &mut events, &terminal, |output| output.contains("bash-ok")).await;
                assert!(output.contains("printf bash-ok"));
                assert!(output.contains("bash-ok"));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_select_dialog_resolves_and_restores_editor() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                let ui = mode.ui_context();
                let selection = Rc::new(RefCell::new(None));
                let result = Rc::clone(&selection);
                let handle = tokio::task::spawn_local(async move {
                    let value = ui
                        .select(
                            &JsString::from("Pick"),
                            &[JsString::from("one"), JsString::from("two")],
                            None,
                        )
                        .await
                        .expect("select result");
                    *result.borrow_mut() = value;
                });
                for _ in 0..50 {
                    mode.handle_ui_requests().await;
                    let mounted = mode
                        .editor_container
                        .borrow()
                        .children()
                        .first()
                        .is_some_and(|child| !Rc::ptr_eq(child, &mode.editor_handle.borrow()));
                    if mounted {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
                let selector = mode
                    .editor_container
                    .borrow()
                    .children()
                    .first()
                    .cloned()
                    .expect("selector mounted");
                selector.borrow_mut().handle_input(&JsString::from("\r"));
                for _ in 0..50 {
                    mode.handle_ui_requests().await;
                    if selection.borrow().is_some() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
                let _ = handle.await;
                assert_eq!(selection.borrow().as_ref(), Some(&JsString::from("one")));
                assert_eq!(mode.editor_container.borrow().children().len(), 1);
                let _ = pump(&mode, &mut events, &terminal, |_| true).await;
            })
            .await;
    }

    struct StaticWidget;

    impl ExtensionUiComponent for StaticWidget {
        fn render(&self, _width: usize) -> Vec<JsString> {
            vec![JsString::from("custom widget")]
        }

        fn wants_input(&self) -> bool {
            false
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_string_widgets_truncate_at_ten_lines() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let lines: Vec<String> = (0..12).map(|index| format!("line-{index}")).collect();
                mode.ui_sender
                    .send(UiRequest::SetWidget {
                        key: JsString::from("status"),
                        content: Some(lines.into_iter().map(JsString::from).collect::<Vec<_>>()),
                        placement: WidgetPlacement::AboveEditor,
                    })
                    .expect("widget request");
                mode.handle_ui_requests().await;
                let rendered = JsString::join(mode.widget_container_above.borrow_mut().render(80), "\n");
                assert!(rendered.contains("line-0"), "{rendered:?}");
                assert!(rendered.contains("line-9"), "{rendered:?}");
                assert!(!rendered.contains("line-10"), "{rendered:?}");
                assert!(rendered.contains("... (widget truncated)"), "{rendered:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_component_widgets_render_factory_components() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let factory: ExtensionUiComponentFactory = Arc::new(|_context| Box::new(StaticWidget));
                mode.ui_sender
                    .send(UiRequest::SetWidgetComponent {
                        key: JsString::from("custom"),
                        content: Some(factory),
                        placement: WidgetPlacement::AboveEditor,
                    })
                    .expect("widget request");
                mode.handle_ui_requests().await;
                let rendered = JsString::join(mode.widget_container_above.borrow_mut().render(80), "\n");
                assert!(rendered.contains("custom widget"), "{rendered:?}");

                mode.ui_sender
                    .send(UiRequest::SetWidgetComponent {
                        key: JsString::from("custom"),
                        content: None,
                        placement: WidgetPlacement::BelowEditor,
                    })
                    .expect("widget removal");
                mode.handle_ui_requests().await;
                let rendered = JsString::join(mode.widget_container_above.borrow_mut().render(80), "\n");
                assert!(!rendered.contains("custom widget"), "{rendered:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn set_theme_object_installs_the_instance() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let theme = std::sync::Arc::new(
                    crate::modes::interactive::theme::load_theme_from_path(
                        &js_path(&Path::new(env!("CARGO_MANIFEST_DIR")).join("theme").join("light.json")),
                        None,
                    )
                    .expect("light theme"),
                );
                let result = mode.set_theme_instance(std::sync::Arc::clone(&theme));
                assert!(result.success, "{result:?}");
                assert!(std::sync::Arc::ptr_eq(
                    &crate::modes::interactive::theme::current_theme(),
                    &theme
                ));
            })
            .await;
    }

    async fn open_settings(
        mode: &Rc<InteractiveMode>,
        events: &mut tokio::sync::mpsc::UnboundedReceiver<AgentSessionEvent>,
        terminal: &Rc<RefCell<CaptureTerminal>>,
    ) {
        terminal.borrow_mut().send_input("/settings");
        mode.ui.process_events();
        terminal.borrow_mut().send_input("\r");
        mode.ui.process_events();
        for _ in 0..100 {
            mode.drain_actions().await;
            mode.handle_ui_requests().await;
            mode.ui.flush_pending();
            let mounted = mode
                .editor_container
                .borrow()
                .children()
                .first()
                .is_some_and(|child| !Rc::ptr_eq(child, &mode.editor_handle.borrow()));
            if mounted {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        let _ = pump(mode, events, terminal, |_| true).await;
        assert!(
            mode.editor_container
                .borrow()
                .children()
                .first()
                .is_some_and(|child| !Rc::ptr_eq(child, &mode.editor_handle.borrow())),
            "settings selector must be mounted"
        );
    }

    fn activate_setting(mode: &Rc<InteractiveMode>, terminal: &Rc<RefCell<CaptureTerminal>>, label: &str) {
        for _ in 0..64 {
            terminal.borrow_mut().send_input("\u{7f}");
        }
        mode.ui.process_events();
        terminal.borrow_mut().send_input(label);
        mode.ui.process_events();
        terminal.borrow_mut().send_input("\r");
        mode.ui.process_events();
    }

    fn settings_file(agent_dir: &Path) -> serde_json::Value {
        let path = agent_dir.join("settings.json");
        let content = std::fs::read_to_string(&path).expect("settings.json must be written");
        serde_json::from_str(&content).expect("settings.json must be valid JSON")
    }

    #[tokio::test(flavor = "current_thread")]
    async fn settings_selector_updates_http_timeout_and_writes_settings_file() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                init_dark_theme();
                let agent_dir = tempfile::tempdir().expect("agent dir");
                let cwd = tempfile::tempdir().expect("cwd");
                let settings = SettingsManager::create(
                    &js_path(cwd.path()),
                    Some(&js_path(agent_dir.path())),
                    SettingsManagerCreateOptions::default(),
                )
                .expect("settings manager");
                let (mode, terminal, _faux, _temp) = build_mode_with_settings(
                    StaticResourceLoader::new() as Arc<dyn crate::core::resource_loader::ResourceLoader>,
                    None,
                    settings,
                )
                .await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;
                open_settings(&mode, &mut events, &terminal).await;

                activate_setting(&mode, &terminal, "HTTP idle timeout");
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("HTTP idle timeout: disabled")
                })
                .await;
                assert!(output.contains("HTTP idle timeout: disabled"));

                let value = settings_file(agent_dir.path());
                assert_eq!(value.get("httpIdleTimeoutMs"), Some(&serde_json::json!(0)));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn trust_selector_saves_decision_and_reports_status() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                init_dark_theme();
                let agent_dir = tempfile::tempdir().expect("agent dir");
                let cwd = tempfile::tempdir().expect("cwd");
                let settings = SettingsManager::create(
                    &js_path(cwd.path()),
                    Some(&js_path(agent_dir.path())),
                    SettingsManagerCreateOptions::default(),
                )
                .expect("settings manager");
                let (mode, terminal, _faux, _temp) = build_mode_with_settings(
                    StaticResourceLoader::new() as Arc<dyn crate::core::resource_loader::ResourceLoader>,
                    None,
                    settings,
                )
                .await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;

                terminal.borrow_mut().send_input("/trust");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| output.contains("Project trust")).await;
                assert!(output.contains("Project trust"));

                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Saved trust decision: trusted.")
                })
                .await;
                assert!(output.contains("Saved trust decision: trusted. Restart pi for this to take effect."));

                let session_cwd = mode.session().cwd().to_owned();
                let session_agent_dir = mode.session().agent_dir().to_owned();
                let store =
                    crate::core::trust_manager::ProjectTrustStore::new(&session_agent_dir).expect("trust store");
                let entry = store
                    .get_entry(&session_cwd)
                    .expect("trust entry read")
                    .expect("saved trust entry");
                assert_eq!(
                    entry.decision,
                    crate::core::trust_manager::ProjectTrustDecision::Trusted
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sandbox_selector_saves_profile_and_updates_bwrap_status() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                init_dark_theme();
                let agent_dir = tempfile::tempdir().expect("agent dir");
                let cwd = tempfile::tempdir().expect("cwd");
                let settings = SettingsManager::create(
                    &js_path(cwd.path()),
                    Some(&js_path(agent_dir.path())),
                    SettingsManagerCreateOptions::default(),
                )
                .expect("settings manager");
                let (mode, terminal, _faux, _temp) = build_mode_with_settings(
                    StaticResourceLoader::new() as Arc<dyn crate::core::resource_loader::ResourceLoader>,
                    None,
                    settings,
                )
                .await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;

                mode.show_sandbox_selector();
                pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Sandbox profiles")
                })
                .await;
                terminal.borrow_mut().send_input("\u{1b}[B");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Saved sandbox profile full-access to global scope.")
                })
                .await;
                assert!(output.contains("Saved sandbox profile full-access to global scope."));

                let value = settings_file(agent_dir.path());
                assert_eq!(value.get("sandboxProfile"), Some(&serde_json::json!("full-access")));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn approval_selector_saves_preset_and_reports_status() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                init_dark_theme();
                let agent_dir = tempfile::tempdir().expect("agent dir");
                let cwd = tempfile::tempdir().expect("cwd");
                let settings = SettingsManager::create(
                    &js_path(cwd.path()),
                    Some(&js_path(agent_dir.path())),
                    SettingsManagerCreateOptions::default(),
                )
                .expect("settings manager");
                let (mode, terminal, _faux, _temp) = build_mode_with_settings(
                    StaticResourceLoader::new() as Arc<dyn crate::core::resource_loader::ResourceLoader>,
                    None,
                    settings,
                )
                .await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;

                mode.show_approval_selector();
                pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Approval presets")
                })
                .await;
                terminal.borrow_mut().send_input("\u{1b}[B");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Saved approval preset to global settings.")
                })
                .await;
                assert!(
                    output.contains("Saved approval preset to global settings. Restart pi for this to take effect.")
                );

                let value = settings_file(agent_dir.path());
                assert_eq!(
                    value.get("approvalPreset"),
                    Some(&serde_json::json!("llm-approved-deny"))
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn resume_selector_switches_to_the_selected_session() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;

                let session = mode.session();
                let cwd = session.cwd().to_owned();
                let session_dir = tempfile::tempdir().expect("session dir").keep();
                let session_dir = session_dir.display().to_string();
                {
                    let mut manager = session.session_manager();
                    *manager = SessionManager::create(&cwd, Some(&JsString::from(&session_dir)), None)
                        .expect("persisted manager");
                }
                let mut target =
                    SessionManager::create(&cwd, Some(&JsString::from(&session_dir)), None).expect("target session");
                target
                    .append_message(pi_agent_core::agent::types::AgentMessage::User(
                        pi_ai::types::UserMessage {
                            role: pi_ai::types::UserRole::Tag,
                            content: pi_ai::types::UserContent::Text("resume target prompt".into()),
                            input_source: None,
                            provider_native: None,
                            timestamp: 1,
                        },
                    ))
                    .expect("append message");
                target.persist_for_recovery().expect("persist target session");
                let target_id = target.get_session_id().to_owned();
                let target_file = target.get_session_file().expect("target file");
                assert!(std::path::Path::new(&&encode_node_utf8(&target_file)).exists());

                terminal.borrow_mut().send_input("/resume");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Resume Session")
                })
                .await;
                let mut ready = false;
                for _ in 0..200 {
                    mode.ui.process_events();
                    mode.ui.flush_pending();
                    mode.handle_ui_requests().await;
                    ready = mode.active_session_selector.borrow().as_ref().is_some_and(|selector| {
                        selector
                            .try_lock()
                            .map(|selector| selector.session_list().get_selected_session_path().is_some())
                            .unwrap_or(false)
                    });
                    if ready {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                assert!(ready, "session selector must list the target session");

                let selector = mode.active_session_selector.borrow().clone();
                if let Some(selector) = selector {
                    selector.lock().await.handle_input(&JsString::from("\r"));
                }
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Resumed session")
                })
                .await;
                assert!(output.contains("Resumed session"));
                assert_eq!(mode.session().session_id(), target_id);
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn login_selector_opens_the_ambient_auth_dialog() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;

                terminal.borrow_mut().send_input("/login");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Select authentication method")
                })
                .await;

                terminal.borrow_mut().send_input("\u{1b}[B");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Select provider to configure")
                })
                .await;

                terminal.borrow_mut().send_input("faux");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("is configured outside pi.")
                })
                .await;
                assert!(output.contains("Faux is configured outside pi."));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn logout_selector_removes_the_stored_api_key() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;

                terminal.borrow_mut().send_input("/logout");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                pump(&mode, &mut events, &terminal, |output| output.contains("faux")).await;

                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Removed stored API key for faux.")
                })
                .await;
                assert!(output.contains("Removed stored API key for faux."));
                let remaining = mode
                    .session()
                    .model_runtime()
                    .list_credentials(pi_ai::auth::types::AuthOperationOptions::new(None))
                    .await
                    .expect("credentials");
                assert!(remaining.iter().all(|credential| credential.provider_id != "faux"));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tree_selector_navigates_to_the_selected_entry() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                init_dark_theme();
                let mut settings_map = serde_json::Map::new();
                settings_map.insert("branchSummary".to_owned(), serde_json::json!({"skipPrompt": true}));
                let settings = SettingsManager::in_memory(
                    settings_map
                        .into_iter()
                        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                        .collect::<pi_js_compat::js_json::JsObject>(),
                    SettingsManagerCreateOptions::default(),
                );
                let (mode, terminal, _faux, _temp) = build_mode_with_settings(
                    StaticResourceLoader::new() as Arc<dyn crate::core::resource_loader::ResourceLoader>,
                    None,
                    settings,
                )
                .await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;

                terminal.borrow_mut().send_input("hi");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                pump(&mode, &mut events, &terminal, |output| {
                    output.contains("hello from faux")
                })
                .await;
                assert!(mode.session().messages().len() >= 2);

                terminal.borrow_mut().send_input("/tree");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| output.contains("Session Tree")).await;
                assert!(output.contains("Session Tree"));

                terminal.borrow_mut().send_input("\u{1b}[A");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Navigated to selected point")
                })
                .await;
                assert!(output.contains("Navigated to selected point"));
                assert_eq!(mode.session().messages().len(), 2);
                assert_eq!(mode.session().get_last_assistant_text(), Some("hello from faux".into()));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fork_selector_forks_from_the_selected_user_message() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;

                terminal.borrow_mut().send_input("hi");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                pump(&mode, &mut events, &terminal, |output| {
                    output.contains("hello from faux")
                })
                .await;

                terminal.borrow_mut().send_input("/fork");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Fork from Message")
                })
                .await;

                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Forked to new session")
                })
                .await;
                assert!(output.contains("Forked to new session"));
                assert_eq!(mode.editor().borrow().get_text(), "hi");
                assert!(mode.session().messages().is_empty());
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn scoped_models_selector_updates_session_scope_and_persists_selection() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                init_dark_theme();
                let agent_dir = tempfile::tempdir().expect("agent dir");
                let cwd = tempfile::tempdir().expect("cwd");
                let settings = SettingsManager::create(
                    &js_path(cwd.path()),
                    Some(&js_path(agent_dir.path())),
                    SettingsManagerCreateOptions::default(),
                )
                .expect("settings manager");
                let (mode, terminal, _faux, _temp) = build_mode_with_settings(
                    StaticResourceLoader::new() as Arc<dyn crate::core::resource_loader::ResourceLoader>,
                    None,
                    settings,
                )
                .await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;

                terminal.borrow_mut().send_input("/scoped-models");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Model Configuration")
                })
                .await;
                assert!(output.contains("Model Configuration"));

                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let scoped = mode.session().scoped_models();
                assert_eq!(scoped.len(), 1, "first toggle keeps only the selected model");
                assert_eq!(scoped[0].model.id, "faux-1");

                terminal.borrow_mut().send_input("\x13");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Model selection saved to settings")
                })
                .await;
                assert!(output.contains("Model selection saved to settings"));
                let value = settings_file(agent_dir.path());
                assert_eq!(value.get("enabledModels"), Some(&serde_json::json!(["faux/faux-1"])));

                terminal.borrow_mut().send_input("\u{1b}");
                mode.ui.process_events();
                pump(&mode, &mut events, &terminal, |_| true).await;
                assert!(Rc::ptr_eq(
                    mode.editor_container
                        .borrow()
                        .children()
                        .first()
                        .expect("editor mounted"),
                    &mode.editor_handle.borrow()
                ));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn settings_selector_updates_session_auto_compact_and_thinking_level() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;
                assert!(mode.session().auto_compaction_enabled());
                open_settings(&mode, &mut events, &terminal).await;

                activate_setting(&mode, &terminal, "Auto-compact");
                pump(&mode, &mut events, &terminal, |_| true).await;
                assert!(!mode.session().auto_compaction_enabled());

                let before = mode.session().thinking_level();
                activate_setting(&mode, &terminal, "Thinking level");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\x1b[B");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                pump(&mode, &mut events, &terminal, |_| true).await;
                assert_ne!(mode.session().thinking_level(), before);
            })
            .await;
    }

    struct EmptyBody;

    impl pi_ai::request::HttpBody for EmptyBody {
        fn next_chunk(&mut self) -> pi_ai::request::BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
            Box::pin(async { None })
        }
    }

    struct RecordingFetch {
        requests: Arc<Mutex<Vec<HttpRequest>>>,
    }

    impl Fetch for RecordingFetch {
        fn fetch(
            &self,
            request: HttpRequest,
            _signal: Option<tokio_util::sync::CancellationToken>,
        ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, CallbackError>> {
            self.requests.lock().expect("requests").push(request);
            Box::pin(async {
                Ok(pi_ai::request::HttpResponse {
                    status: 200,
                    status_text: "OK".to_owned(),
                    headers: Vec::new(),
                    body: Box::new(EmptyBody),
                })
            })
        }
    }

    #[test]
    fn changelog_display_returns_only_entries_newer_than_the_seen_version() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("CHANGELOG.md");
        std::fs::write(&path, "## [1.1.0]\n- Added feature\n\n## [1.0.0]\n- Older feature\n").expect("changelog");

        let display = changelog_display_for(Some(&JsString::from("1.0.0")), &path, false);
        assert!(display.update_version);
        let markdown = display.markdown.expect("new entry markdown");
        assert!(markdown.contains("Added feature"), "{markdown:?}");
        assert!(!markdown.contains("Older feature"), "{markdown:?}");

        let display = changelog_display_for(Some(&JsString::from("1.1.0")), &path, false);
        assert!(!display.update_version);
        assert!(display.markdown.is_none());

        let display = changelog_display_for(None, &path, false);
        assert!(display.update_version);
        assert!(display.markdown.is_none(), "fresh installs record without rendering");

        let display = changelog_display_for(None, &path, true);
        assert!(!display.update_version);
        assert!(display.markdown.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn startup_notices_render_new_changelog_entries() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                *mode.changelog_markdown.borrow_mut() = Some(JsString::from("## [1.2.3]\n- Added feature"));
                mode.startup_notices_shown.set(false);
                mode.show_startup_notices_if_needed();
                let rendered = pi_tui::utils::strip_terminal_sequences(&JsString::join(
                    mode.chat_container.borrow_mut().render(120),
                    "\n",
                ));
                assert!(rendered.contains("What's New"), "{rendered:?}");
                assert!(rendered.contains("Added feature"), "{rendered:?}");
                let children = mode.chat_container.borrow().children().len();
                mode.show_startup_notices_if_needed();
                assert_eq!(mode.chat_container.borrow().children().len(), children);

                mode.chat_container.borrow_mut().clear();
                mode.settings_mut(|settings| settings.set_collapse_changelog(true));
                mode.startup_notices_shown.set(false);
                mode.show_startup_notices_if_needed();
                let rendered = pi_tui::utils::strip_terminal_sequences(&JsString::join(
                    mode.chat_container.borrow_mut().render(120),
                    "\n",
                ));
                assert!(
                    rendered.contains("Updated to v1.2.3. Use /changelog to view full changelog."),
                    "{rendered:?}"
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn install_telemetry_reports_the_version_through_the_configured_fetch() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let requests = Arc::new(Mutex::new(Vec::new()));
                *mode.http_fetch.borrow_mut() = Some(Arc::new(RecordingFetch {
                    requests: Arc::clone(&requests),
                }));
                mode.report_install_telemetry(&JsString::from("0.84.1"));
                for _ in 0..50 {
                    if !requests.lock().expect("requests").is_empty() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                {
                    let recorded = requests.lock().expect("requests");
                    assert_eq!(recorded.len(), 1);
                    assert_eq!(recorded[0].url, "https://pi.dev/api/report-install?version=0.84.1");
                    assert_eq!(
                        recorded[0].headers,
                        vec![(
                            "User-Agent".to_owned(),
                            encode_node_utf8(&crate::utils::pi_user_agent::get_pi_user_agent(&"0.84.1".into()))
                        )]
                    );
                }
                requests.lock().expect("requests").clear();
                mode.settings_mut(|settings| settings.set_enable_install_telemetry(false));
                mode.report_install_telemetry(&JsString::from("0.84.1"));
                tokio::time::sleep(Duration::from_millis(20)).await;
                assert!(requests.lock().expect("requests").is_empty());
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prompt_activity_animates_the_prompt_until_the_source_finishes() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;

                let working = |mode: &Rc<InteractiveMode>| {
                    let editor = mode.editor();
                    let rendered =
                        pi_tui::utils::strip_terminal_sequences(&JsString::join(editor.borrow_mut().render(80), "\n"));
                    pi_tui::components::loader::DEFAULT_FRAMES
                        .iter()
                        .any(|frame| rendered.contains(*frame))
                };
                assert!(!working(&mode));
                mode.set_prompt_activity(PromptActivitySource::SummaryMaterialization, true);
                assert!(working(&mode));
                mode.set_prompt_activity(PromptActivitySource::SummaryMaterialization, false);
                assert!(!working(&mode));

                mode.working_visible.set(false);
                mode.set_prompt_activity(PromptActivitySource::Compaction, true);
                assert!(!working(&mode), "hidden working state must not animate");
                mode.working_visible.set(true);
                mode.set_prompt_activity(PromptActivitySource::Compaction, false);
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn editor_layout_callback_positions_the_working_editor() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;
                let position = mode.default_editor.borrow_mut().editor_mut().get_prompt_position();
                assert!(position.is_some(), "editor layout must place the prompt cell");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn anthropic_subscription_auth_warning_is_shown_once() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let mut credentials = serde_json::Map::new();
                credentials.insert(
                    "anthropic".to_owned(),
                    serde_json::json!({"type": "oauth", "access": "token", "refresh": "refresh", "expires": 0}),
                );
                let (mode, terminal, _faux, _temp) = build_mode_with_extra_providers(
                    StaticResourceLoader::new(),
                    None,
                    SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default()),
                    vec![pi_ai::providers::anthropic::anthropic_provider()],
                    credentials,
                    false,
                    TestModeSetup::default(),
                )
                .await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;
                let model = mode
                    .session()
                    .model_runtime()
                    .get_provider("anthropic")
                    .expect("anthropic provider")
                    .get_models()
                    .into_iter()
                    .next()
                    .expect("anthropic model");

                mode.maybe_warn_about_anthropic_subscription_auth(Some(model.clone()))
                    .await;
                let rendered = pi_tui::utils::strip_terminal_sequences(&JsString::join(
                    mode.chat_container.borrow_mut().render(200),
                    "\n",
                ));
                assert!(
                    rendered.contains("Anthropic subscription auth is active."),
                    "{rendered:?}"
                );
                assert!(mode.anthropic_subscription_warning_shown.get());

                mode.chat_container.borrow_mut().clear();
                mode.maybe_warn_about_anthropic_subscription_auth(Some(model)).await;
                let rendered = pi_tui::utils::strip_terminal_sequences(&JsString::join(
                    mode.chat_container.borrow_mut().render(200),
                    "\n",
                ));
                assert!(!rendered.contains("Anthropic subscription auth is active."));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn anthropic_subscription_auth_warning_respects_the_settings_opt_out() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let mut settings = serde_json::Map::new();
                settings.insert("warnings".to_owned(), serde_json::json!({"anthropicExtraUsage": false}));
                let mut credentials = serde_json::Map::new();
                credentials.insert(
                    "anthropic".to_owned(),
                    serde_json::json!({"type": "oauth", "access": "token", "refresh": "refresh", "expires": 0}),
                );
                let (mode, _terminal, _faux, _temp) = build_mode_with_extra_providers(
                    StaticResourceLoader::new(),
                    None,
                    SettingsManager::in_memory(
                        settings
                            .into_iter()
                            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                            .collect::<pi_js_compat::js_json::JsObject>(),
                        SettingsManagerCreateOptions::default(),
                    ),
                    vec![pi_ai::providers::anthropic::anthropic_provider()],
                    credentials,
                    false,
                    TestModeSetup::default(),
                )
                .await;
                let model = mode
                    .session()
                    .model_runtime()
                    .get_provider("anthropic")
                    .expect("anthropic provider")
                    .get_models()
                    .into_iter()
                    .next()
                    .expect("anthropic model");
                mode.maybe_warn_about_anthropic_subscription_auth(Some(model)).await;
                assert!(!mode.anthropic_subscription_warning_shown.get());
                assert!(mode.chat_container.borrow().children().is_empty());
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn share_command_creates_a_gist_and_reports_the_viewer_url() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let temp = tempfile::tempdir().expect("temp dir");
                let gh = temp.path().join("gh");
                std::fs::write(
                    &gh,
                    "#!/bin/sh\nif [ \"$1\" = \"auth\" ]; then exit 0; fi\nif [ \"$1\" = \"gist\" ]; then printf 'https://gist.github.com/alice/abc123\\n'; exit 0; fi\nexit 1\n",
                )
                .expect("gh script");
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755))
                        .expect("gh executable");
                }

                let (mode, terminal, _faux, _temp) = build_mode_with_persisted_session().await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;
                mode.session()
                    .prompt("share-me", PromptOptions::default())
                    .await
                    .expect("session message");

                mode.handle_share_command_with_program(&JsString::from(&gh.display().to_string()))
                    .await;

                let rendered = pi_tui::utils::strip_terminal_sequences(
                    &JsString::join(mode.chat_container.borrow_mut().render(200), "\n"),
                );
                assert!(
                    rendered.contains("Share URL: https://pi.dev/session/#abc123"),
                    "{rendered:?}"
                );
                assert!(
                    rendered.contains("Gist: https://gist.github.com/alice/abc123"),
                    "{rendered:?}"
                );
                assert!(!std::env::temp_dir().join("session.html").exists());
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn daxnuts_easter_egg_triggers_for_opencode_kimi_models() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let mut model = mode.session().model();
                model.provider = "opencode".to_owned();
                model.id = "kimi-k2.5-preview".to_owned();
                let before = mode.chat_container.borrow().children().len();
                mode.check_daxnuts_easter_egg(&model);
                let after = mode.chat_container.borrow().children().len();
                assert_eq!(after, before + 2, "spacer plus the daxnuts component");

                let mut other = model.clone();
                other.id = "kimi-k2".to_owned();
                mode.check_daxnuts_easter_egg(&other);
                assert_eq!(mode.chat_container.borrow().children().len(), after);
            })
            .await;
    }

    #[derive(Clone)]
    struct FakeClock(Rc<Cell<f64>>);

    impl FakeClock {
        fn new(millis: f64) -> Self {
            Self(Rc::new(Cell::new(millis)))
        }

        fn install(&self, mode: &Rc<InteractiveMode>) {
            let clock = self.clone();
            *mode.now_millis.borrow_mut() = Rc::new(move || clock.0.get());
        }

        fn advance(&self, millis: f64) {
            self.0.set(self.0.get() + millis);
        }
    }

    fn status_text(mode: &Rc<InteractiveMode>) -> JsString {
        let rendered = JsString::join(mode.status_container.borrow_mut().render(160), "\n");
        pi_tui::utils::strip_terminal_sequences(&rendered)
    }

    fn chat_text(mode: &Rc<InteractiveMode>) -> JsString {
        let rendered = JsString::join(mode.chat_container.borrow_mut().render(160), "\n");
        pi_tui::utils::strip_terminal_sequences(&rendered)
    }

    fn test_store() -> Arc<MultiAgentStore> {
        Arc::new(MultiAgentStore::new(
            crate::core::multi_agent_store::MultiAgentStoreOptions {
                now: Some(Arc::new(|| "2026-06-27T00:00:00.000Z".to_owned())),
            },
        ))
    }

    fn spawn_agent(
        store: &MultiAgentStore,
        name: &str,
        slot: Option<i64>,
        transcript_path: Option<&JsString>,
        model: Option<crate::core::multi_agent_store::AgentModel>,
    ) -> JsString {
        let agent = AgentSnapshot {
            id: store.allocate_agent_id_for_lifecycle_coordinator("agent").unwrap(),
            parent_id: None,
            display_name: JsString::from(name.to_owned()),
            agent_type: JsString::from("worker"),
            origin: None,
            lifecycle: AgentLifecycle::Running,
            revision: 1,
            created_at: JsString::from("2026-06-27T00:00:00.000Z"),
            updated_at: JsString::from("2026-06-27T00:00:00.000Z"),
            cwd: JsString::from("/repo"),
            worktree: None,
            model,
            account: None,
            permission: crate::core::multi_agent_store::AgentPermission {
                policy: JsString::from("on-request"),
                inherited_from: None,
                narrowed: true,
            },
            slot: slot.map(|index| crate::core::multi_agent_store::AgentSlot { index, pinned: true }),
            transcript: transcript_path.map(|path| crate::core::multi_agent_store::AgentTranscriptMetadata {
                session_id: JsString::from("child-session"),
                path: Some(JsString::from(path.to_owned())),
            }),
            event_stream: None,
            worker: None,
            detached: None,
            current_activity: None,
            last_activity: None,
            result: None,
            error: None,
            extra: serde_json::Map::new()
                .into_iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect::<pi_js_compat::js_json::JsObject>(),
        };
        let id = agent.id.clone();
        store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();
        id
    }

    async fn build_mode_with_store(
        store: Arc<MultiAgentStore>,
    ) -> (
        Rc<InteractiveMode>,
        Rc<RefCell<CaptureTerminal>>,
        Arc<pi_ai::providers::faux::FauxProviderHandle>,
        tempfile::TempDir,
    ) {
        build_mode_with_extra_providers(
            StaticResourceLoader::new(),
            None,
            SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default()),
            Vec::new(),
            serde_json::Map::new(),
            false,
            TestModeSetup {
                multi_agent_store: Some(store),
            },
        )
        .await
    }

    fn write_child_session(root: &Path, cwd: &JsString, messages: &[&str]) -> (JsString, JsString) {
        let session_dir = js_path(&root.join("child-sessions"));
        let mut manager = SessionManager::create(cwd, Some(&session_dir), None).expect("child session");
        manager
            .append_model_change(&JsString::from("faux"), &JsString::from("faux-2"))
            .expect("model change");
        manager
            .append_thinking_level_change(&JsString::from("high"))
            .expect("thinking change");
        for text in messages {
            manager
                .append_message(AgentMessage::User(pi_ai::types::UserMessage {
                    role: pi_ai::types::UserRole::Tag,
                    content: pi_ai::types::UserContent::Text((*text).into()),
                    input_source: None,
                    provider_native: None,
                    timestamp: 0,
                }))
                .expect("child message");
        }
        let mut assistant = faux_assistant_message(
            "child answer",
            pi_ai::providers::faux::FauxAssistantMessageOptions::default(),
        );
        assistant.provider = "faux".to_owned();
        assistant.model = "faux-2".to_owned();
        assistant.usage.input = 1_500.0;
        assistant.usage.output = 300.0;
        manager
            .append_message(AgentMessage::Assistant(Box::new(assistant)))
            .expect("child assistant");
        manager.persist_for_recovery().expect("persist child");
        let path = manager.get_session_file().expect("child file").clone();
        (path, session_dir)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_slot_keybindings_select_active_agents_and_main() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let store = test_store();
                let first = spawn_agent(&store, "First", Some(1), None, None);
                let second = spawn_agent(&store, "Second", Some(2), None, None);
                let (mode, terminal, _faux, _temp) = build_mode_with_store(Arc::clone(&store)).await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;

                terminal.borrow_mut().send_input("\x1b2");
                mode.ui.process_events();
                assert_eq!(store.get_selected_agent_id().as_ref(), Some(&first));
                let chat = chat_text(&mode);
                assert!(chat.contains("Viewing live agent: First"), "{chat:?}");

                terminal.borrow_mut().send_input("\x1b3");
                mode.ui.process_events();
                assert_eq!(store.get_selected_agent_id().as_ref(), Some(&second));
                let chat = chat_text(&mode);
                assert!(chat.contains("Viewing live agent: Second"), "{chat:?}");

                terminal.borrow_mut().send_input("\x1b1");
                mode.ui.process_events();
                assert_eq!(store.get_selected_agent_id(), None);
                let chat = chat_text(&mode);
                assert!(!chat.contains("Viewing live agent:"), "{chat:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn inactive_agent_slot_reports_status_without_switching() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let store = test_store();
                let done = spawn_agent(&store, "Done", Some(1), None, None);
                let mut snapshot = store.get_agent(&JsString::from(&done)).expect("agent");
                snapshot.lifecycle = AgentLifecycle::Completed;
                snapshot.revision += 1;
                store.publish_lifecycle_coordinator_snapshot(&snapshot).unwrap();
                let active = spawn_agent(&store, "Active", Some(2), None, None);
                let (mode, _terminal, _faux, _temp) = build_mode_with_store(Arc::clone(&store)).await;
                let _events = start(&mode).await;

                assert!(mode.select_agent_slot(1));
                assert_eq!(store.get_selected_agent_id().as_ref(), Some(&active));
                assert!(!status_text(&mode).contains("not active"));

                assert!(mode.select_agent_slot(2));
                assert_eq!(store.get_selected_agent_id().as_ref(), Some(&active));
                assert!(!mode.select_agent_slot(5));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn thinking_and_tool_waiting_timers_update_the_working_loader() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let clock = FakeClock::new(1_000_000.0);
                clock.install(&mode);
                let loader = mode.create_working_loader(mode.get_working_loader_indicator(WorkingLoaderView::Main));
                *mode.loading_animation.borrow_mut() = Some(Rc::clone(&loader));
                *mode.working_loader_view.borrow_mut() = Some(WorkingLoaderView::Main);
                mode.status_container.borrow_mut().add_child(loader);

                mode.handle_event(AgentSessionEvent::Agent(AgentEvent::ModelRequestStart))
                    .await;
                let status = status_text(&mode);
                assert!(status.contains("Thinking... 0s"), "{status:?}");

                clock.advance(2_000.0);
                mode.poll_timers();
                let status = status_text(&mode);
                assert!(status.contains("Thinking... 2s"), "{status:?}");

                mode.handle_event(AgentSessionEvent::Agent(AgentEvent::ModelRequestEnd))
                    .await;
                clock.advance(2_000.0);
                mode.poll_timers();
                let status = status_text(&mode);
                assert!(status.contains("Thinking... 2s"), "timer must stop: {status:?}");

                mode.handle_event(AgentSessionEvent::Agent(AgentEvent::ToolExecutionStart {
                    tool_call_id: "bash-1".to_owned(),
                    tool_name: "bash".to_owned(),
                    args: JsValue::from(serde_json::json!({"command": "sleep 10"})),
                    started_at: clock.0.get(),
                }))
                .await;
                let status = status_text(&mode);
                assert!(status.contains("Waiting for command..."), "{status:?}");
                mode.pending_tools.borrow_mut().clear();
                clock.advance(2_000.0);
                mode.poll_timers();
                let status = status_text(&mode);
                assert!(status.contains("Waiting for command... Elapsed: 2s"), "{status:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn retry_loader_counts_down_and_reports_final_failure() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                mode.handle_event(AgentSessionEvent::AutoRetryStart(
                    crate::core::agent_session::AutoRetryStartEvent {
                        event_type: "auto_retry_start",
                        attempt: 1,
                        max_attempts: 3,
                        delay_ms: 3_000.0,
                        error_message: "boom".into(),
                    },
                ))
                .await;
                let status = status_text(&mode);
                assert!(
                    status.contains("Retrying (1/3) in 3s... (escape to cancel)"),
                    "{status:?}"
                );

                tokio::time::sleep(Duration::from_millis(1_100)).await;
                mode.poll_timers();
                let status = status_text(&mode);
                assert!(
                    status.contains("Retrying (1/3) in 2s... (escape to cancel)"),
                    "{status:?}"
                );

                mode.handle_event(AgentSessionEvent::AutoRetryEnd(
                    crate::core::agent_session::AutoRetryEndEvent {
                        event_type: "auto_retry_end",
                        success: false,
                        attempt: 3,
                        final_error: None,
                    },
                ))
                .await;
                let status = status_text(&mode);
                assert!(!status.contains("Retrying ("), "{status:?}");
                let chat = chat_text(&mode);
                assert!(
                    chat.contains("Retry failed after 3 attempts: Unknown error"),
                    "{chat:?}"
                );
            })
            .await;
    }

    fn compaction_source(
        source_type: crate::core::compaction::CompactionSourceType,
        provider: &str,
        model: &str,
        endpoint: Option<&str>,
    ) -> crate::core::compaction::CompactionSourceInfo {
        crate::core::compaction::CompactionSourceInfo {
            r#type: source_type,
            provider: provider.to_owned(),
            model: model.to_owned(),
            endpoint: endpoint.map(str::to_owned).map(JsString::from),
        }
    }

    fn compaction_result(
        source: Option<crate::core::compaction::CompactionSourceInfo>,
    ) -> crate::core::compaction::CompactionResult {
        crate::core::compaction::CompactionResult {
            summary: "summary".into(),
            first_kept_entry_id: JsString::from("entry-1"),
            tokens_before: 123.0,
            duration_ms: None,
            estimated_tokens_after: None,
            kept_from_previous_context_tokens: None,
            compacted_result_tokens: None,
            compacted_result_bytes: None,
            source,
            usage: None,
            provider_native: None,
            details: None,
        }
    }

    #[test]
    fn compaction_labels_match_the_source() {
        use crate::core::compaction::CompactionSourceType;
        let remote = compaction_source(
            CompactionSourceType::OpenAiRemote,
            "openai-codex",
            "gpt-5.5",
            Some("https://chatgpt.com/backend-api/codex/responses/compact"),
        );
        assert_eq!(
            format_compaction_start_label(
                &JsString::from("manual"),
                Some(&remote),
                &JsString::from("(escape to cancel)")
            ),
            "Compacting context via OpenAI remote endpoint (openai-codex/gpt-5.5, https://chatgpt.com/backend-api/codex/responses/compact)... (escape to cancel)"
        );

        let local = compaction_source(CompactionSourceType::Local, "anthropic", "claude-sonnet-4-5", None);
        assert_eq!(
            format_compaction_start_label(
                &JsString::from("threshold"),
                Some(&local),
                &JsString::from("(escape to cancel)")
            ),
            "Auto-compacting locally... (escape to cancel)"
        );
        assert_eq!(
            format_compaction_start_label(&JsString::from("overflow"), None, &JsString::from("(escape to cancel)")),
            "Context overflow detected, Auto-compacting... (escape to cancel)"
        );
        assert_eq!(
            format_compaction_start_label(
                &JsString::from("overflow"),
                Some(&remote),
                &JsString::from("(escape to cancel)")
            ),
            "Context overflow detected, Auto-compacting via OpenAI remote endpoint (openai-codex/gpt-5.5, https://chatgpt.com/backend-api/codex/responses/compact)... (escape to cancel)"
        );

        assert_eq!(
            format_compaction_failure_message(
                "Context overflow recovery failed: Turn prefix summarization failed: Request timed out.",
                &JsString::from("overflow"),
                Some(&compaction_source(
                    CompactionSourceType::Local,
                    "ollama",
                    "qwen3:4b-instruct-128k",
                    None,
                )),
            ),
            [
                "Context overflow recovery failed after compaction timeout.",
                "Model: ollama/qwen3:4b-instruct-128k.",
                "The compaction request did not finish before the timeout.",
                "No compaction was saved; the previous context is still too large.",
                "Original error: Turn prefix summarization failed: Request timed out.",
            ]
            .join("\n")
        );
        assert_eq!(
            format_compaction_failure_message("Compaction failed: boom", &JsString::from("manual"), None),
            [
                "Compaction failed.",
                "No compaction was saved; context remains unchanged.",
                "Original error: boom.",
            ]
            .join("\n")
        );

        assert_eq!(
            format_compaction_source_log_message(Some(&remote)),
            "Compaction completed via OpenAI remote endpoint (openai-codex/gpt-5.5, https://chatgpt.com/backend-api/codex/responses/compact)"
        );
        assert_eq!(
            format_compaction_source_log_message(Some(&local)),
            "Compaction completed locally (anthropic/claude-sonnet-4-5)"
        );
        assert_eq!(
            format_compaction_source_log_message(None),
            "Compaction completed locally"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn compaction_start_renders_the_source_loader_and_binds_escape() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let mut settings = serde_json::Map::new();
                settings.insert(
                    "terminal".to_owned(),
                    serde_json::json!({"showTerminalProgress": true}),
                );
                let (mode, terminal, _faux, _temp) = build_mode_with_settings(
                    StaticResourceLoader::new(),
                    None,
                    SettingsManager::in_memory(settings.into_iter().map(|(key, value)| (JsString::from(key), JsValue::from(value))).collect::<pi_js_compat::js_json::JsObject>(), SettingsManagerCreateOptions::default()),
                )
                .await;
                let _events = start(&mode).await;
                let sentinel = Rc::new(Cell::new(false));
                let sentinel_flag = Rc::clone(&sentinel);
                mode.default_editor.borrow_mut().on_escape = Some(Rc::new(move || {
                    sentinel_flag.set(true);
                }));

                let source_hint = compaction_source(
                    crate::core::compaction::CompactionSourceType::OpenAiRemote,
                    "openai",
                    "gpt-4.1-mini",
                    Some("https://api.openai.com/v1/responses/compact"),
                );
                mode.handle_event(AgentSessionEvent::CompactionStart(
                    crate::core::agent_session::CompactionStartEvent {
                        event_type: "compaction_start",
                        reason: "manual".to_owned(),
                        source_hint: Some(source_hint.clone()),
                    },
                ))
                .await;

                let status = status_text(&mode);
                assert!(
                    status.contains(
                        "Compacting context via OpenAI remote endpoint (openai/gpt-4.1-mini, https://api.openai.com/v1/responses/compact)... (escape to cancel)"
                    ),
                    "{status:?}"
                );
                assert!(mode.auto_compaction_loader.borrow().is_some());
                assert_eq!(mode.auto_compaction_source_hint.borrow().as_ref(), Some(&source_hint));
                assert_eq!(*terminal.borrow().progress, vec![true]);

                let escape = mode.default_editor.borrow().on_escape.clone().expect("escape handler");
                escape();
                assert!(!sentinel.get(), "escape must abort compaction instead of the prior handler");

                mode.handle_event(AgentSessionEvent::CompactionEnd(
                    Box::new(crate::core::agent_session::CompactionEndEvent {
                        event_type: "compaction_end",
                        reason: "manual".to_owned(),
                        result: None,
                        aborted: true,
                        will_retry: false,
                        error_message: None,
                    }),
                ))
                .await;

                assert!(mode.auto_compaction_loader.borrow().is_none());
                assert!(status_text(&mode).is_empty());
                assert_eq!(*terminal.borrow().progress, vec![true, false]);
                assert!(chat_text(&mode).contains("Compaction cancelled"));
                let restored = mode.default_editor.borrow().on_escape.clone().expect("restored escape handler");
                restored();
                assert!(sentinel.get(), "compaction end must restore the previous escape handler");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn compaction_end_reports_the_source_and_reason_specific_failures() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let local_source = compaction_source(
                    crate::core::compaction::CompactionSourceType::Local,
                    "anthropic",
                    "claude-sonnet-4-5",
                    None,
                );
                mode.handle_event(AgentSessionEvent::CompactionStart(
                    crate::core::agent_session::CompactionStartEvent {
                        event_type: "compaction_start",
                        reason: "manual".to_owned(),
                        source_hint: Some(local_source.clone()),
                    },
                ))
                .await;
                mode.handle_event(AgentSessionEvent::CompactionEnd(Box::new(
                    crate::core::agent_session::CompactionEndEvent {
                        event_type: "compaction_end",
                        reason: "manual".to_owned(),
                        result: Some(compaction_result(Some(local_source.clone()))),
                        aborted: false,
                        will_retry: false,
                        error_message: None,
                    },
                )))
                .await;
                let chat = chat_text(&mode);
                assert!(
                    chat.contains("Compaction completed locally (anthropic/claude-sonnet-4-5)"),
                    "{chat:?}"
                );

                mode.handle_event(AgentSessionEvent::CompactionStart(
                    crate::core::agent_session::CompactionStartEvent {
                        event_type: "compaction_start",
                        reason: "threshold".to_owned(),
                        source_hint: Some(compaction_source(
                            crate::core::compaction::CompactionSourceType::Local,
                            "ollama",
                            "qwen3:4b-instruct-128k",
                            None,
                        )),
                    },
                ))
                .await;
                mode.handle_event(AgentSessionEvent::CompactionEnd(Box::new(
                    crate::core::agent_session::CompactionEndEvent {
                        event_type: "compaction_end",
                        reason: "threshold".to_owned(),
                        result: None,
                        aborted: false,
                        will_retry: false,
                        error_message: Some(
                            "Auto-compaction failed: Turn prefix summarization failed: Request timed out.".into(),
                        ),
                    },
                )))
                .await;
                let chat = chat_text(&mode);
                let normalized = JsString::join(chat.split('\n').map(|text| text.trim()), "\n");
                assert!(
                    normalized.contains(
                        &[
                            "Compaction failed after timeout.",
                            "Model: ollama/qwen3:4b-instruct-128k.",
                            "The compaction request did not finish before the timeout.",
                            "No compaction was saved; the previous context is still too large.",
                            "Original error: Turn prefix summarization failed: Request timed out.",
                        ]
                        .join("\n")
                    ),
                    "{chat:?}"
                );

                mode.handle_event(AgentSessionEvent::CompactionStart(
                    crate::core::agent_session::CompactionStartEvent {
                        event_type: "compaction_start",
                        reason: "manual".to_owned(),
                        source_hint: None,
                    },
                ))
                .await;
                mode.handle_event(AgentSessionEvent::CompactionEnd(Box::new(
                    crate::core::agent_session::CompactionEndEvent {
                        event_type: "compaction_end",
                        reason: "manual".to_owned(),
                        result: None,
                        aborted: false,
                        will_retry: false,
                        error_message: Some("Compaction failed: boom".into()),
                    },
                )))
                .await;
                let chat = chat_text(&mode);
                let normalized = JsString::join(chat.split('\n').map(|text| text.trim()), "\n");
                assert!(
                    normalized.contains(
                        &[
                            "Compaction failed.",
                            "No compaction was saved; context remains unchanged.",
                            "Original error: boom.",
                        ]
                        .join("\n")
                    ),
                    "{chat:?}"
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn child_view_overrides_the_footer_session() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let temp = tempfile::tempdir().expect("temp dir");
                let cwd = temp.path().join("repo");
                std::fs::create_dir_all(&cwd).expect("cwd");
                let (path, _session_dir) = write_child_session(temp.path(), &js_path(&cwd), &["child prompt"]);
                let store = test_store();
                spawn_agent(&store, "Child", Some(1), Some(&path), None);
                let (mode, _terminal, _faux, _temp) = build_mode_with_store(Arc::clone(&store)).await;
                let _events = start(&mode).await;
                let main_footer = pi_tui::utils::strip_terminal_sequences(&JsString::join(
                    mode.footer.borrow_mut().render(200),
                    "\n",
                ));
                assert!(main_footer.contains("faux-1"), "{main_footer:?}");

                assert!(mode.select_agent_slot(1));
                let footer = pi_tui::utils::strip_terminal_sequences(&JsString::join(
                    mode.footer.borrow_mut().render(200),
                    "\n",
                ));
                assert!(footer.contains("faux-2"), "{footer:?}");
                assert!(!footer.contains("faux-1"), "{footer:?}");
                assert!(footer.contains('↑'), "child usage must be shown: {footer:?}");
                let chat = chat_text(&mode);
                assert!(chat.contains("child prompt"), "{chat:?}");

                assert!(mode.select_agent_view_from_bridge(&JsString::from("main")));
                let footer = pi_tui::utils::strip_terminal_sequences(&JsString::join(
                    mode.footer.borrow_mut().render(200),
                    "\n",
                ));
                assert!(footer.contains("faux-1"), "{footer:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn message_end_moves_the_streaming_component_into_the_chat_container() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let message = AgentMessage::Assistant(Box::new(faux_assistant_message(
                    "streamed answer",
                    pi_ai::providers::faux::FauxAssistantMessageOptions::default(),
                )));
                mode.handle_event(AgentSessionEvent::Agent(AgentEvent::MessageStart {
                    message: message.clone(),
                }))
                .await;
                assert_eq!(mode.transcript_tail_container.borrow().children().len(), 1);
                assert!(mode.chat_container.borrow().children().is_empty());

                mode.handle_event(AgentSessionEvent::Agent(AgentEvent::MessageEnd { message }))
                    .await;
                assert!(mode.transcript_tail_container.borrow().children().is_empty());
                assert_eq!(mode.chat_container.borrow().children().len(), 1);
                assert!(chat_text(&mode).contains("streamed answer"));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn child_transcript_reloads_after_a_watched_file_change() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let temp = tempfile::tempdir().expect("temp dir");
                let cwd = temp.path().join("repo");
                std::fs::create_dir_all(&cwd).expect("cwd");
                let (path, session_dir) = write_child_session(temp.path(), &js_path(&cwd), &["first child message"]);
                let store = test_store();
                spawn_agent(&store, "Child", Some(1), Some(&path), None);
                let (mode, _terminal, _faux, _temp) = build_mode_with_store(Arc::clone(&store)).await;
                let _events = start(&mode).await;
                assert!(mode.select_agent_slot(1));
                assert!(chat_text(&mode).contains("first child message"));

                let mut manager =
                    SessionManager::open(&JsString::from(&path), Some(&JsString::from(&session_dir)), None)
                        .expect("open child");
                manager
                    .append_message(AgentMessage::User(pi_ai::types::UserMessage {
                        role: pi_ai::types::UserRole::Tag,
                        content: pi_ai::types::UserContent::Text("second child message".into()),
                        input_source: None,
                        provider_native: None,
                        timestamp: 0,
                    }))
                    .expect("append child message");
                manager.persist_for_recovery().expect("persist child");

                let mut rendered = JsString::default();
                for _ in 0..400 {
                    mode.poll_child_view_events();
                    rendered = chat_text(&mode);
                    if rendered.contains("second child message") {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                assert!(rendered.contains("second child message"), "{rendered:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_context_exposes_autocomplete_and_editor_factories() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let runner = mode.session().extension_runner();
                let context = runner.create_context();
                let ui = context.ui().expect("ui context");

                let wrapped = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let wrapped_by_factory = Arc::clone(&wrapped);
                ui.add_autocomplete_provider(Arc::new(move |provider| {
                    wrapped_by_factory.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    provider
                }));
                mode.handle_ui_requests().await;
                assert_eq!(wrapped.load(std::sync::atomic::Ordering::SeqCst), 1);
                assert_eq!(mode.autocomplete_provider_wrappers.borrow().len(), 1);
                assert!(mode.autocomplete_provider.borrow().is_some());

                let factory: ExtensionEditorFactory = Arc::new(|_tui, _theme, _keybindings| {
                    Rc::new(RefCell::new(TestEditorComponent)) as Rc<RefCell<dyn EditorComponent>>
                });
                ui.set_editor_component(Some(Arc::clone(&factory)));
                mode.handle_ui_requests().await;
                assert!(ui.get_editor_component().is_some());
                assert!(
                    !Rc::ptr_eq(
                        &mode.editor(),
                        &(mode.default_editor.clone() as Rc<RefCell<dyn EditorComponent>>)
                    ),
                    "custom editor must be mounted"
                );

                ui.set_editor_component(None);
                mode.handle_ui_requests().await;
                assert!(ui.get_editor_component().is_none());
                assert!(
                    Rc::ptr_eq(
                        &mode.editor(),
                        &(mode.default_editor.clone() as Rc<RefCell<dyn EditorComponent>>)
                    ),
                    "default editor must be restored"
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_factory_builds_real_custom_editor() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let ui = mode.session().extension_runner().create_context().ui().unwrap();
                ui.set_editor_component(Some(Arc::new(|tui, theme, keybindings| {
                    Rc::new(RefCell::new(
                        CustomEditor::new(Rc::new(tui), theme, keybindings, Default::default()).expect("custom editor"),
                    ))
                })));
                mode.handle_ui_requests().await;
                terminal.borrow_mut().send_input("real editor");
                mode.ui.process_events();
                assert_eq!(mode.editor().borrow().get_text(), "real editor");
                let lines = mode.editor().borrow_mut().render(40);
                assert!(lines.iter().any(|line| line.contains("real editor")));
                assert!(lines.iter().any(|line| line.contains(pi_tui::tui::CURSOR_MARKER)));
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                assert!(
                    !mode.actions.borrow().is_empty(),
                    "factory editor must inherit submit callback"
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_factory_uses_base_fixed_cells() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode_with_store(test_store()).await;
                let _events = start(&mode).await;
                let ui = mode.session().extension_runner().create_context().ui().unwrap();
                ui.set_editor_component(Some(Arc::new(|tui, theme, kb| {
                    let cell = tui.create_fixed_cell();
                    cell.place(0, 0);
                    assert!(
                        !cell.update(&JsString::from("x")),
                        "TS base cells request a render, not an immediate write"
                    );
                    cell.clear();
                    Rc::new(RefCell::new(
                        CustomEditor::new(Rc::new(tui), theme, kb, Default::default()).unwrap(),
                    ))
                })));
                mode.handle_ui_requests().await;
                terminal.borrow_mut().send_input("fixed-cell input");
                mode.ui.process_events();
                assert_eq!(mode.editor().borrow().get_text(), "fixed-cell input");
                assert!(
                    mode.editor()
                        .borrow_mut()
                        .render(30)
                        .iter()
                        .any(|line| line.contains(pi_tui::tui::CURSOR_MARKER))
                );
            })
            .await;
    }

    struct ReleaseInput {
        opted_in: bool,
        inputs: Arc<std::sync::Mutex<Vec<JsString>>>,
    }
    impl ExtensionUiComponent for ReleaseInput {
        fn render(&self, _: usize) -> Vec<JsString> {
            vec!["release fixture".into()]
        }
        fn wants_key_release(&self) -> bool {
            self.opted_in
        }
        fn handle_input(&self, data: &JsString) {
            self.inputs.lock().unwrap().push(data.clone());
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_release_opt_in_reaches_real_tui() {
        assert!(!StaticWidget.wants_key_release());
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, terminal, _, _temp) = build_mode().await;
                let _events = start(&mode).await;
                for opted_in in [false, true] {
                    let inputs = Arc::new(std::sync::Mutex::new(Vec::new()));
                    let captured = inputs.clone();
                    let (send, _receive) = tokio::sync::oneshot::channel();
                    mode.show_extension_custom(
                        Rc::new(move |_, _| {
                            crate::core::extensions::ready_custom_component(Box::new(ReleaseInput {
                                opted_in,
                                inputs: captured.clone(),
                            }))
                        }),
                        None,
                        send,
                    );
                    tokio::task::yield_now().await;
                    terminal.borrow_mut().send_input("a");
                    terminal.borrow_mut().send_input("\x1b[97;1:3u");
                    mode.ui.process_events();
                    assert_eq!(
                        *inputs.lock().unwrap(),
                        if opted_in { vec!["a", "\x1b[97;1:3u"] } else { vec!["a"] }
                    );
                    mode.restore_editor();
                }
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_tui_lifecycle_is_synchronous_and_force_redraws() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, terminal, _, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let tui = mode.extension_factory_context().tui;
                mode.set_extension_header(Some(Arc::new(|_| Box::new(StaticWidget))));
                mode.ui.render_now();
                terminal.borrow_mut().writes.clear();
                tui.request_render_force();
                mode.handle_ui_requests().await;
                mode.ui.flush_pending();
                assert!(
                    terminal.borrow().output().contains("custom widget"),
                    "forced redraw must repaint unchanged frame: {:?}",
                    terminal.borrow().writes
                );
                terminal.borrow_mut().writes.clear();
                tui.stop();
                assert!(
                    JsString::join(&terminal.borrow().writes, "").contains("\x1b[?1004l"),
                    "stop must release before returning"
                );
                terminal.borrow_mut().writes.clear();
                tui.start().unwrap();
                assert!(
                    JsString::join(&terminal.borrow().writes, "").contains("\x1b[?1004h"),
                    "start must reacquire before returning"
                );
                tui.stop();
                let error = TerminalRawModeError::new(true, "fixture raw-mode failure");
                terminal.borrow_mut().start_error = Some(error.clone());
                assert_eq!(tui.start(), Err(JsString::from(error.to_string())));
                terminal.borrow_mut().start_error = None;
            })
            .await;
    }

    struct FocusedExtensionInput(Arc<std::sync::atomic::AtomicBool>);

    impl ExtensionUiComponent for FocusedExtensionInput {
        fn focused(&self) -> Option<bool> {
            Some(self.0.load(std::sync::atomic::Ordering::SeqCst))
        }

        fn set_focused(&self, focused: bool) {
            self.0.store(focused, std::sync::atomic::Ordering::SeqCst);
        }

        fn render(&self, _width: usize) -> Vec<JsString> {
            vec![js_format!(
                "input{}",
                if self.focused() == Some(true) {
                    pi_tui::tui::CURSOR_MARKER
                } else {
                    ""
                }
            )]
        }
    }

    struct SizedExtensionInput {
        done: ExtensionUiDone,
        focus: std::sync::atomic::AtomicBool,
        rendered_width: Arc<std::sync::atomic::AtomicUsize>,
        disposed: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl ExtensionUiComponent for SizedExtensionInput {
        fn handle_input(&self, data: &JsString) {
            if data == "\r" {
                self.done
                    .call(Some(pi_js_compat::js_json!({"action":"Search","query":"x"})));
            }
        }
        fn width(&self) -> Option<pi_tui::tui::SizeValue> {
            Some(pi_tui::tui::SizeValue::Absolute(70.0))
        }
        fn focused(&self) -> Option<bool> {
            Some(self.focus.load(std::sync::atomic::Ordering::SeqCst))
        }
        fn set_focused(&self, focused: bool) {
            self.focus.store(focused, std::sync::atomic::Ordering::SeqCst);
        }
        fn render(&self, width: usize) -> Vec<JsString> {
            self.rendered_width.store(width, std::sync::atomic::Ordering::SeqCst);
            vec![js_format!(
                "overlay{}",
                if self.focused() == Some(true) {
                    pi_tui::tui::CURSOR_MARKER
                } else {
                    ""
                }
            )]
        }
        fn dispose(&self) {
            self.disposed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    struct LocalEditorForm {
        editor: RefCell<pi_tui::components::editor::Editor>,
        done: ExtensionUiDone,
    }
    impl ExtensionUiComponent for LocalEditorForm {
        fn render(&self, width: usize) -> Vec<JsString> {
            self.editor.borrow_mut().render(width)
        }
        fn handle_input(&self, data: &JsString) {
            if data == "\r" {
                self.done
                    .call(Some(pi_js_compat::js_json!(self.editor.borrow().get_text())));
            } else {
                self.editor.borrow_mut().handle_input(&JsString::from(data));
            }
        }
    }

    struct RetainedOverlay {
        handle: Rc<RefCell<Option<Rc<dyn crate::core::extensions::ExtensionUiOverlayHandle>>>>,
        events: Arc<std::sync::Mutex<Vec<&'static str>>>,
        done: ExtensionUiDone,
    }
    impl ExtensionUiComponent for RetainedOverlay {
        fn render(&self, _: usize) -> Vec<JsString> {
            vec!["toggle".into()]
        }
        fn on_overlay_handle(&self, handle: Rc<dyn crate::core::extensions::ExtensionUiOverlayHandle>) {
            assert!(handle.is_focused());
            self.events.lock().unwrap().push("hook");
            *self.handle.borrow_mut() = Some(handle);
            let retained = self.handle.clone();
            let events = self.events.clone();
            let done = self.done.clone();
            tokio::task::spawn_local(async move {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                let handle = retained.borrow().clone().unwrap();
                handle.set_hidden(true);
                assert!(handle.is_hidden());
                assert!(!handle.is_focused());
                events.lock().unwrap().push("hidden");
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                handle.set_hidden(false);
                assert!(!handle.is_hidden());
                assert!(handle.is_focused());
                events.lock().unwrap().push("shown");
                done.call(None);
            });
        }
        fn dispose(&self) {
            self.events.lock().unwrap().push("disposed");
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_local_on_handle_retains_handle_for_later_timer() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let events = Arc::new(std::sync::Mutex::new(Vec::new()));
                let captured = events.clone();
                let option_events = events.clone();
                let (send, receive) = tokio::sync::oneshot::channel();
                mode.show_extension_custom(
                    Rc::new(move |_, done| {
                        crate::core::extensions::ready_custom_component(Box::new(RetainedOverlay {
                            handle: Rc::new(RefCell::new(None)),
                            events: captured.clone(),
                            done,
                        }))
                    }),
                    Some(ExtensionUiCustomOptions {
                        overlay: Some(true),
                        on_handle: Some(Arc::new(move |_| option_events.lock().unwrap().push("option"))),
                        ..Default::default()
                    }),
                    send,
                );
                tokio::task::yield_now().await;
                assert_eq!(*events.lock().unwrap(), vec!["hook", "option"]);
                assert_eq!(
                    tokio::time::timeout(std::time::Duration::from_secs(2), receive)
                        .await
                        .unwrap()
                        .unwrap()
                        .unwrap(),
                    None
                );
                assert_eq!(
                    *events.lock().unwrap(),
                    vec!["hook", "option", "hidden", "shown", "disposed"]
                );
                assert!(!mode.ui.has_overlay_entries());
            })
            .await;
    }

    struct NestedInput {
        width: Arc<std::sync::atomic::AtomicUsize>,
        input: Arc<std::sync::Mutex<Vec<JsString>>>,
    }
    impl ExtensionUiComponent for NestedInput {
        fn render(&self, width: usize) -> Vec<JsString> {
            self.width.store(width, std::sync::atomic::Ordering::SeqCst);
            vec![JsString::from("nested")]
        }
        fn handle_input(&self, input: &JsString) {
            self.input.lock().unwrap().push(input.clone());
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_nested_overlay_options_input_and_passive_focus() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let tui = mode.extension_factory_context().tui;
                let width = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let input = Arc::new(std::sync::Mutex::new(Vec::new()));
                let active = tui.show_overlay(
                    Box::new(NestedInput {
                        width: width.clone(),
                        input: input.clone(),
                    }),
                    Some(crate::core::extensions::ExtensionUiOverlayOptions {
                        width: Some(pi_tui::tui::SizeValue::Absolute(23.0)),
                        ..Default::default()
                    }),
                );
                mode.ui.render_now();
                assert_eq!(width.load(std::sync::atomic::Ordering::SeqCst), 23);
                let passive = tui.show_overlay(
                    Box::new(StaticWidget),
                    Some(crate::core::extensions::ExtensionUiOverlayOptions {
                        non_capturing: true,
                        ..Default::default()
                    }),
                );
                assert!(active.is_focused());
                assert!(!passive.is_focused());
                terminal.borrow_mut().send_input("nested key");
                mode.ui.process_events();
                assert_eq!(*input.lock().unwrap(), vec!["nested key"]);
                passive.focus();
                assert!(passive.is_focused());
                passive.unfocus();
                assert!(active.is_focused());
                active.set_hidden(true);
                assert!(!active.is_focused());
                active.set_hidden(false);
                assert!(active.is_focused());
                active.hide();
                assert!(mode.ui.has_overlay_entries(), "hide removes only its own entry");
                passive.hide();
                assert!(!mode.ui.has_overlay_entries());
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_local_editor_nested_overlay_and_handle_callback() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
                let callback = observed.clone();
                let (send, receive) = tokio::sync::oneshot::channel();
                mode.show_extension_custom(
                    Rc::new(|context, done| {
                        let nested = context.tui.show_overlay(Box::new(StaticWidget), None);
                        assert!(nested.is_focused());
                        nested.set_hidden(true);
                        assert!(nested.is_hidden());
                        assert!(!nested.is_focused());
                        nested.set_hidden(false);
                        nested.focus();
                        assert!(nested.is_focused());
                        nested.unfocus();
                        assert!(!nested.is_focused());
                        nested.hide();
                        crate::core::extensions::ready_custom_component(Box::new(LocalEditorForm {
                            editor: RefCell::new(pi_tui::components::editor::Editor::new(
                                Rc::new(context.tui),
                                crate::modes::interactive::theme::get_editor_theme(),
                                pi_tui::components::editor::EditorOptions::default(),
                            )),
                            done,
                        }))
                    }),
                    Some(ExtensionUiCustomOptions {
                        overlay: Some(true),
                        on_handle: Some(Arc::new(move |handle| {
                            callback.lock().unwrap().push(handle.is_focused());
                            handle.set_hidden(true);
                            callback.lock().unwrap().push(handle.is_hidden());
                            handle.set_hidden(false);
                        })),
                        ..Default::default()
                    }),
                    send,
                );
                tokio::task::yield_now().await;
                assert_eq!(*observed.lock().unwrap(), vec![true, true]);
                terminal.borrow_mut().send_input("typed 🚀");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                assert_eq!(
                    receive.await.unwrap().unwrap(),
                    Some(pi_js_compat::js_json!("typed 🚀"))
                );
                assert!(!mode.ui.has_overlay_entries());
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_custom_restores_text_saved_before_construction_only_without_overlay() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                for overlay in [false, true] {
                    mode.editor().borrow_mut().set_text(&JsString::from("before"));
                    let editor = mode.editor();
                    let (send, receive) = tokio::sync::oneshot::channel();
                    mode.show_extension_custom(
                        Rc::new(move |context, done| {
                            editor.borrow_mut().set_text(&JsString::from("factory"));
                            crate::core::extensions::ready_custom_component(Box::new(LocalEditorForm {
                                editor: RefCell::new(pi_tui::components::editor::Editor::new(
                                    Rc::new(context.tui),
                                    crate::modes::interactive::theme::get_editor_theme(),
                                    pi_tui::components::editor::EditorOptions::default(),
                                )),
                                done,
                            }))
                        }),
                        Some(ExtensionUiCustomOptions {
                            overlay: Some(overlay),
                            ..Default::default()
                        }),
                        send,
                    );
                    tokio::task::yield_now().await;
                    let ui = mode.ui_context();
                    ui.set_editor_text(&JsString::from("during"));
                    mode.handle_ui_requests().await;
                    assert_eq!(mode.editor().borrow().get_text(), "during");
                    terminal.borrow_mut().send_input("\r");
                    mode.ui.process_events();
                    assert_eq!(receive.await.unwrap().unwrap(), Some(pi_js_compat::js_json!("")));
                    assert_eq!(
                        mode.editor().borrow().get_text(),
                        if overlay { "during" } else { "before" }
                    );
                }
            })
            .await;
    }

    struct AsyncEditorForm {
        form: LocalEditorForm,
        disposed: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl ExtensionUiComponent for AsyncEditorForm {
        fn render(&self, width: usize) -> Vec<JsString> {
            self.form.render(width)
        }
        fn handle_input(&self, data: &JsString) {
            self.form.handle_input(&JsString::from(data));
        }
        fn dispose(&self) {
            self.disposed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_async_factory_settlement_orders_mount_restore_and_early_done() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                for overlay in [false, true] {
                    for (early, reject) in [(false, false), (false, true), (true, false), (true, true)] {
                        mode.editor().borrow_mut().set_text(&JsString::from("before"));
                        let editor = mode.ui.get_focused_component().unwrap();
                        let (release, pending) = tokio::sync::oneshot::channel::<Result<(), JsString>>();
                        let pending = Arc::new(std::sync::Mutex::new(Some(pending)));
                        let handles = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                        let counted = handles.clone();
                        let disposed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                        let counted_disposal = disposed.clone();
                        let ui = mode.ui_context();
                        let mut result = Some(ui.custom(
                            Arc::new(move |context, done| {
                                let pending = pending.lock().unwrap().take().unwrap();
                                let disposed = counted_disposal.clone();
                                if early {
                                    done.call(Some(pi_js_compat::js_json!("early")));
                                }
                                Ok(Box::pin(async move {
                                    pending.await.unwrap()?;
                                    let component: Box<dyn ExtensionUiComponent> = Box::new(AsyncEditorForm {
                                        form: LocalEditorForm {
                                            editor: RefCell::new(pi_tui::components::editor::Editor::new(
                                                Rc::new(context.tui),
                                                crate::modes::interactive::theme::get_editor_theme(),
                                                pi_tui::components::editor::EditorOptions::default(),
                                            )),
                                            done,
                                        },
                                        disposed,
                                    });
                                    Ok(component)
                                }))
                            }),
                            Some(ExtensionUiCustomOptions {
                                overlay: Some(overlay),
                                on_handle: Some(Arc::new(move |_| {
                                    counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                                })),
                                ..Default::default()
                            }),
                        ));
                        mode.handle_ui_requests().await;
                        tokio::task::yield_now().await;
                        assert!(!mode.ui.has_overlay_entries(), "pending factory must not mount");
                        assert!(Rc::ptr_eq(&editor, &mode.ui.get_focused_component().unwrap()));
                        ui.set_editor_text(&JsString::from("during"));
                        mode.handle_ui_requests().await;
                        terminal.borrow_mut().writes.clear();
                        mode.ui.request_render_force();
                        mode.ui.render_now();
                        assert!(
                            JsString::join(&terminal.borrow().writes, "").contains("during"),
                            "pending factory leaves editor visible"
                        );
                        let early_result = if early {
                            Some(result.take().unwrap().await)
                        } else {
                            None
                        };
                        release
                            .send(if reject {
                                Err(JsString::from("async failure"))
                            } else {
                                Ok(())
                            })
                            .unwrap();
                        tokio::task::yield_now().await;
                        tokio::task::yield_now().await;
                        if early {
                            assert_eq!(early_result.unwrap().unwrap(), Some(pi_js_compat::js_json!("early")));
                            assert!(!mode.ui.has_overlay_entries());
                            assert!(Rc::ptr_eq(&editor, &mode.ui.get_focused_component().unwrap()));
                            assert_eq!(
                                mode.editor().borrow().get_text(),
                                "during",
                                "late settlement must not restore twice"
                            );
                        } else if reject {
                            assert_eq!(result.take().unwrap().await, Err(JsString::from("async failure")));
                            assert!(!mode.ui.has_overlay_entries());
                            assert!(Rc::ptr_eq(&editor, &mode.ui.get_focused_component().unwrap()));
                            assert_eq!(
                                mode.editor().borrow().get_text(),
                                if overlay { "during" } else { "before" }
                            );
                        } else {
                            assert_eq!(mode.ui.has_overlay_entries(), overlay);
                            assert!(!Rc::ptr_eq(&editor, &mode.ui.get_focused_component().unwrap()));
                            terminal.borrow_mut().writes.clear();
                            mode.ui.render_now();
                            terminal.borrow_mut().send_input("\r");
                            mode.ui.process_events();
                            assert_eq!(result.take().unwrap().await.unwrap(), Some(pi_js_compat::js_json!("")));
                            assert_eq!(
                                mode.editor().borrow().get_text(),
                                if overlay { "during" } else { "before" }
                            );
                        }
                        assert_eq!(
                            disposed.load(std::sync::atomic::Ordering::SeqCst),
                            usize::from(!early && !reject),
                            "unmounted components must not receive dispose"
                        );
                        assert_eq!(
                            handles.load(std::sync::atomic::Ordering::SeqCst),
                            usize::from(overlay && !early && !reject)
                        );
                        assert!(!mode.ui.has_overlay_entries());
                    }
                }
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_retained_done_after_rejection_still_closes_once() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                for overlay in [false, true] {
                    for synchronous in [false, true] {
                        mode.editor().borrow_mut().set_text(&JsString::from("before"));
                        let retained = Rc::new(RefCell::new(None::<ExtensionUiDone>));
                        let saved = retained.clone();
                        let (send, receive) = tokio::sync::oneshot::channel();
                        mode.show_extension_custom(
                            Rc::new(move |_, done| {
                                *saved.borrow_mut() = Some(done);
                                if synchronous {
                                    Err(JsString::from("rejected"))
                                } else {
                                    Ok(Box::pin(async { Err(JsString::from("rejected")) }))
                                }
                            }),
                            Some(ExtensionUiCustomOptions {
                                overlay: Some(overlay),
                                ..Default::default()
                            }),
                            send,
                        );
                        tokio::task::yield_now().await;
                        assert_eq!(receive.await.unwrap(), Err(JsString::from("rejected")));
                        mode.editor().borrow_mut().set_text(&JsString::from("after rejection"));
                        if overlay {
                            mode.extension_factory_context()
                                .tui
                                .show_overlay(Box::new(StaticWidget), None);
                        }
                        let done = retained.borrow().clone().unwrap();
                        done.call(None);
                        assert!(
                            !mode.ui.has_overlay_entries(),
                            "Done after rejection still hides the top overlay"
                        );
                        assert_eq!(
                            mode.editor().borrow().get_text(),
                            if overlay { "after rejection" } else { "before" }
                        );
                        mode.editor().borrow_mut().set_text(&JsString::from("after close"));
                        done.call(None);
                        assert_eq!(
                            mode.editor().borrow().get_text(),
                            "after close",
                            "only the first Done closes"
                        );
                    }
                }
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_unfocus_distinguishes_omitted_explicit_target_and_null() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let editor = mode.ui.get_focused_component().unwrap();
                let tui = mode.extension_factory_context().tui;
                let first = tui.show_overlay(Box::new(StaticWidget), None);
                let second = tui.show_overlay(Box::new(StaticWidget), None);
                assert!(second.is_focused());
                second.unfocus();
                assert!(first.is_focused(), "omitted target restores previous focus");
                second.focus();
                second.unfocus_to(Some(editor.clone()));
                assert!(Rc::ptr_eq(&editor, &mode.ui.get_focused_component().unwrap()));
                second.focus();
                second.unfocus_to(None);
                assert!(mode.ui.get_focused_component().is_none(), "explicit null clears focus");
                first.focus();
                second.unfocus_to(None);
                assert!(first.is_focused(), "unfocused handle must not change focus");
                first.hide();
                second.hide();
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_factory_error_rejects_custom_without_changing_focus_or_overlay() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                mode.editor().borrow_mut().set_text(&JsString::from("draft 🚀"));
                let focused = mode.ui.get_focused_component().unwrap();
                for overlay in [false, true] {
                    let ui = mode.ui_context();
                    let result = ui.custom(
                        Arc::new(|_, _| Err(JsString::from("fixture constructor failed"))),
                        Some(ExtensionUiCustomOptions {
                            overlay: Some(overlay),
                            overlay_options: Some(Arc::new(|| panic!("failed factory must not resolve options"))),
                            on_handle: Some(Arc::new(|_| panic!("failed factory must not mount"))),
                        }),
                    );
                    mode.handle_ui_requests().await;
                    assert_eq!(result.await, Err(JsString::from("fixture constructor failed")));
                    assert!(!mode.ui.has_overlay_entries());
                    assert!(Rc::ptr_eq(&focused, &mode.ui.get_focused_component().unwrap()));
                    assert_eq!(mode.editor().borrow().get_text(), "draft 🚀");
                }
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_factory_done_before_error_keeps_first_settlement() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let ui = mode.ui_context();
                let result = ui.custom(
                    Arc::new(|_, done| {
                        done.call(Some(pi_js_compat::js_json!("closed first")));
                        Err(JsString::from("too late"))
                    }),
                    None,
                );
                mode.handle_ui_requests().await;
                assert_eq!(result.await.unwrap(), Some(pi_js_compat::js_json!("closed first")));
                assert!(!mode.ui.has_overlay_entries());
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_overlay_uses_component_width_and_closes_once() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                mode.ui.set_show_hardware_cursor(true);
                let width = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let disposed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let done = Rc::new(RefCell::new(None::<ExtensionUiDone>));
                let (w, d, callback) = (width.clone(), disposed.clone(), done.clone());
                let (send, receive) = tokio::sync::oneshot::channel();
                mode.show_extension_custom(
                    Rc::new(move |_, done| {
                        *callback.borrow_mut() = Some(done.clone());
                        crate::core::extensions::ready_custom_component(Box::new(SizedExtensionInput {
                            done,
                            focus: std::sync::atomic::AtomicBool::new(false),
                            rendered_width: w.clone(),
                            disposed: d.clone(),
                        }))
                    }),
                    Some(ExtensionUiCustomOptions {
                        overlay: Some(true),
                        ..Default::default()
                    }),
                    send,
                );
                tokio::task::yield_now().await;
                terminal.borrow_mut().writes.clear();
                mode.ui.render_now();
                assert_eq!(width.load(std::sync::atomic::Ordering::SeqCst), 70);
                assert!(
                    JsString::join(&terminal.borrow().writes, "").contains("\x1b[13G"),
                    "centered 70-cell overlay plus seven input cells must position hardware cursor at column 13"
                );
                assert!(
                    !JsString::join(&terminal.borrow().writes, "").contains(pi_tui::tui::CURSOR_MARKER),
                    "renderer must consume cursor markers"
                );
                let done = done.borrow().clone().unwrap();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                assert_eq!(
                    receive.await.unwrap().unwrap(),
                    Some(pi_js_compat::js_json!({"action":"Search","query":"x"}))
                );
                assert!(!mode.ui.has_overlay_entries());
                assert_eq!(disposed.load(std::sync::atomic::Ordering::SeqCst), 1);
                done.call(None);
                assert_eq!(disposed.load(std::sync::atomic::Ordering::SeqCst), 1);
                assert!(
                    mode.editor()
                        .borrow_mut()
                        .render(40)
                        .iter()
                        .any(|line| line.contains(pi_tui::tui::CURSOR_MARKER))
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_overlay_explicit_options_and_synchronous_close() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let width = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let captured = width.clone();
                let done = Rc::new(RefCell::new(None::<ExtensionUiDone>));
                let callback = done.clone();
                let (send, receive) = tokio::sync::oneshot::channel();
                mode.show_extension_custom(
                    Rc::new(move |_, done| {
                        *callback.borrow_mut() = Some(done.clone());
                        crate::core::extensions::ready_custom_component(Box::new(SizedExtensionInput {
                            done,
                            focus: std::sync::atomic::AtomicBool::new(false),
                            rendered_width: captured.clone(),
                            disposed: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                        }))
                    }),
                    Some(ExtensionUiCustomOptions {
                        overlay: Some(true),
                        overlay_options: Some(Arc::new(|| crate::core::extensions::types::ExtensionUiOverlayOptions {
                            width: Some(pi_tui::tui::SizeValue::Absolute(47.0)),
                            ..Default::default()
                        })),
                        ..Default::default()
                    }),
                    send,
                );
                tokio::task::yield_now().await;
                mode.ui.render_now();
                assert_eq!(width.load(std::sync::atomic::Ordering::SeqCst), 47);
                done.borrow().clone().unwrap().call(None);
                assert_eq!(receive.await.unwrap().unwrap(), None);
                assert!(!mode.ui.has_overlay_entries());
                let (send, receive) = tokio::sync::oneshot::channel();
                mode.show_extension_custom(
                    Rc::new(|_, done| {
                        done.call(Some(pi_js_compat::js_json!("immediate")));
                        crate::core::extensions::ready_custom_component(Box::new(StaticWidget))
                    }),
                    Some(ExtensionUiCustomOptions {
                        overlay: Some(true),
                        ..Default::default()
                    }),
                    send,
                );
                tokio::task::yield_now().await;
                assert_eq!(
                    receive.await.unwrap().unwrap(),
                    Some(pi_js_compat::js_json!("immediate"))
                );
                assert!(
                    !mode.ui.has_overlay_entries(),
                    "factory completed synchronously: do not mount"
                );
                let (send, receive) = tokio::sync::oneshot::channel();
                let callback = Rc::new(RefCell::new(None::<ExtensionUiDone>));
                let captured = callback.clone();
                mode.show_extension_custom(
                    Rc::new(move |_, done| {
                        *captured.borrow_mut() = Some(done);
                        crate::core::extensions::ready_custom_component(Box::new(StaticWidget))
                    }),
                    Some(ExtensionUiCustomOptions {
                        overlay: Some(false),
                        overlay_options: Some(Arc::new(|| panic!("non-overlay UI must not resolve overlay options"))),
                        ..Default::default()
                    }),
                    send,
                );
                tokio::task::yield_now().await;
                callback.borrow().clone().unwrap().call(None);
                assert_eq!(receive.await.unwrap().unwrap(), None);
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_ui_overlay_focus_emits_cursor_and_restores_editor() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let focused = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let state = focused.clone();
                let (send, _receive) = tokio::sync::oneshot::channel();
                mode.show_extension_custom(
                    Rc::new(move |_, _| {
                        crate::core::extensions::ready_custom_component(Box::new(FocusedExtensionInput(state.clone())))
                    }),
                    Some(ExtensionUiCustomOptions {
                        overlay: Some(true),
                        ..Default::default()
                    }),
                    send,
                );
                tokio::task::yield_now().await;
                assert!(focused.load(std::sync::atomic::Ordering::SeqCst));
                let component = mode.ui.get_focused_component().unwrap();
                assert_eq!(
                    component.borrow_mut().render(40),
                    vec![format!("input{}", pi_tui::tui::CURSOR_MARKER)]
                );
                mode.ui.hide_overlay();
                assert!(!focused.load(std::sync::atomic::Ordering::SeqCst));
                assert!(
                    mode.editor()
                        .borrow_mut()
                        .render(40)
                        .iter()
                        .any(|line| line.contains(pi_tui::tui::CURSOR_MARKER))
                );
            })
            .await;
    }

    async fn build_mode_with_custom_entry_renderer() -> (
        Rc<InteractiveMode>,
        Rc<RefCell<CaptureTerminal>>,
        Arc<pi_ai::providers::faux::FauxProviderHandle>,
        tempfile::TempDir,
    ) {
        let runtime = crate::core::extensions::create_extension_runtime();
        let factory: crate::core::extensions::ExtensionFactory = Arc::new(|api| {
            api.register_entry_renderer(
                "test-entry",
                Arc::new(|entry, _options, _theme| {
                    let message = entry
                        .data
                        .as_ref()
                        .and_then(|data| data.get("message"))
                        .and_then(JsValue::as_string)
                        .cloned()
                        .unwrap_or_default()
                        .to_owned();
                    Some(component(Text::new(
                        encode_node_utf8(&(JsString::from("[Custom]\n") + message)),
                        0,
                        0,
                    )))
                }),
            )
            .expect("entry renderer");
            Box::pin(async { Ok(()) })
        });
        let extension = crate::core::extensions::load_extension_from_factory(
            factory,
            &JsString::from("/tmp"),
            Arc::new(crate::core::event_bus::EventBusController::new()),
            runtime.clone(),
            &JsString::from("/tmp/test-extension"),
        )
        .await
        .expect("extension");
        let loader = Arc::new(StaticResourceLoader {
            extensions: Some(crate::core::extensions::LoadExtensionsResult {
                extensions: vec![extension],
                errors: Vec::new(),
                runtime,
            }),
            ..Default::default()
        }) as Arc<dyn crate::core::resource_loader::ResourceLoader>;
        build_mode_with_loader(loader, None).await
    }

    async fn render_lossless_custom_role(role: JsString) -> JsString {
        let runtime = crate::core::extensions::create_extension_runtime();
        let factory: crate::core::extensions::ExtensionFactory = Arc::new(|api| {
            api.register_message_renderer(
                JsString::from_utf16(vec![0xd800]),
                Arc::new(|message, _, _| {
                    assert_eq!(message.custom_type.units(), &[0xd800]);
                    Some(component(Text::new("surrogate-handler", 0, 0)))
                }),
            )
            .unwrap();
            api.register_message_renderer(
                "�",
                Arc::new(|message, _, _| {
                    assert_eq!(message.custom_type.units(), &[0xfffd]);
                    Some(component(Text::new("replacement-handler", 0, 0)))
                }),
            )
            .unwrap();
            Box::pin(async { Ok(()) })
        });
        let extension = crate::core::extensions::load_extension_from_factory(
            factory,
            &JsString::from("/tmp"),
            Arc::new(crate::core::event_bus::EventBusController::new()),
            runtime.clone(),
            &JsString::from("/tmp/raw-role-extension"),
        )
        .await
        .unwrap();
        let loader = Arc::new(StaticResourceLoader {
            extensions: Some(crate::core::extensions::LoadExtensionsResult {
                extensions: vec![extension],
                errors: vec![],
                runtime,
            }),
            ..Default::default()
        }) as Arc<dyn crate::core::resource_loader::ResourceLoader>;
        let (mode, _, _, _temp) = build_mode_with_loader(loader, None).await;
        let _events = start(&mode).await;
        let value = pi_js_compat::js_json!({"role": role, "content": "raw-role-content", "display": true});
        mode.add_custom_message_to_chat(value.as_object().unwrap());
        JsString::join(mode.chat_container.borrow_mut().render(80), "\n")
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lossless_surrogate_role_selects_its_distinct_callback() {
        let output = tokio::task::LocalSet::new()
            .run_until(render_lossless_custom_role(JsString::from_utf16(vec![0xd800])))
            .await;
        assert!(output.contains("surrogate-handler"));
        assert!(!output.contains("replacement-handler"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lossless_replacement_role_selects_its_distinct_callback() {
        let output = tokio::task::LocalSet::new()
            .run_until(render_lossless_custom_role("�".into()))
            .await;
        assert!(output.contains("replacement-handler"));
        assert!(!output.contains("surrogate-handler"));
    }

    fn custom_entry(message: &str) -> crate::core::session_manager::CustomEntry {
        crate::core::session_manager::CustomEntry {
            custom_type: JsString::from("test-entry"),
            data: Some(pi_js_compat::js_json!({"message": message})),
            id: JsString::from("entry-1"),
            parent_id: None,
            timestamp: JsString::from("2026-06-27T00:00:00.000Z"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn entry_appended_renders_registered_custom_entries() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode_with_custom_entry_renderer().await;
                let _events = start(&mode).await;
                let resources = JsString::join(mode.loaded_resources_container.borrow_mut().render(80), "\n");
                assert!(resources.contains("test-extension"), "{resources:?}");
                mode.handle_event(AgentSessionEvent::EntryAppended(
                    crate::core::agent_session::EntryAppendedEvent {
                        event_type: "entry_appended",
                        entry: SessionEntry::Known(KnownSessionEntry::Custom(custom_entry("appended entry"))),
                    },
                ))
                .await;
                let chat = chat_text(&mode);
                assert!(chat.contains("appended entry"), "{chat:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn initial_render_adds_interactive_user_messages_to_editor_history() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let user = |text: &str, input_source| {
                    AgentMessage::User(pi_ai::types::UserMessage {
                        role: pi_ai::types::UserRole::Tag,
                        content: pi_ai::types::UserContent::Text(text.into()),
                        input_source,
                        provider_native: None,
                        timestamp: 1,
                    })
                };
                {
                    let session = mode.session();
                    let mut manager = session.session_manager();
                    manager.append_message(user("typed prompt", None)).expect("typed");
                    manager
                        .append_message(user("extension prompt", Some(pi_ai::types::InputSource::Extension)))
                        .expect("extension");
                    manager
                        .append_message(user(
                            "<skill name=\"demo\" location=\"/skills/demo/SKILL.md\">\nbody\n</skill>",
                            None,
                        ))
                        .expect("skill");
                }
                mode.render_initial_messages();
                let history = mode.default_editor.borrow().get_history();
                assert!(history.iter().any(|entry| entry == "typed prompt"), "{history:?}");
                assert!(!history.iter().any(|entry| entry == "extension prompt"), "{history:?}");
                assert!(!history.iter().any(|entry| entry.starts_with("<skill")), "{history:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn render_session_entries_renders_persisted_custom_entries() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode_with_custom_entry_renderer().await;
                let _events = start(&mode).await;
                mode.render_session_entries(&[SessionEntry::Known(KnownSessionEntry::Custom(custom_entry(
                    "persisted entry",
                )))]);
                let chat = chat_text(&mode);
                assert!(chat.contains("persisted entry"), "{chat:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn message_end_shows_a_cache_miss_notice() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                mode.session().settings_manager().set_show_cache_miss_notices(true);
                let previous = faux_assistant_message(
                    "previous",
                    pi_ai::providers::faux::FauxAssistantMessageOptions {
                        timestamp: Some(0),
                        ..Default::default()
                    },
                );
                let mut previous = previous;
                previous.provider = "test-provider".to_owned();
                previous.model = "test-model".to_owned();
                previous.usage.cache_write = 100_000.0;
                previous.usage.cost.cache_write = 0.375;
                previous.usage.cost.total = 0.375;
                mode.session()
                    .session_manager()
                    .append_message(AgentMessage::Assistant(Box::new(previous)))
                    .expect("previous assistant");

                let mut current = faux_assistant_message(
                    "current",
                    pi_ai::providers::faux::FauxAssistantMessageOptions {
                        timestamp: Some(600_000),
                        ..Default::default()
                    },
                );
                current.provider = "test-provider".to_owned();
                current.model = "test-model".to_owned();
                current.usage.cache_write = 110_000.0;
                current.usage.cost.cache_write = 0.4125;
                current.usage.cost.total = 0.4125;
                mode.maybe_show_cache_miss_notice(&current);

                let chat = chat_text(&mode);
                assert!(chat.contains("Cache miss after 10m idle"), "{chat:?}");
                assert!(chat.contains("100k tokens re-billed"), "{chat:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn render_session_entries_replays_tool_calls_and_results() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let mut assistant = faux_assistant_message(
                    pi_ai::providers::faux::FauxContent::Blocks(vec![pi_ai::providers::faux::faux_tool_call(
                        "bash",
                        JsObject::from_iter([("command".into(), JsValue::String("echo replayed".into()))]),
                        Some("call-1".to_owned()),
                    )]),
                    pi_ai::providers::faux::FauxAssistantMessageOptions::default(),
                );
                assistant.stop_reason = pi_ai::types::StopReason::ToolUse;
                let result = pi_ai::types::ToolResultMessage {
                    role: pi_ai::types::ToolResultRole::Tag,
                    tool_call_id: "call-1".to_owned(),
                    tool_name: "bash".to_owned(),
                    content: vec![pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new(
                        "replayed tool output",
                    ))],
                    details: None,
                    usage: None,
                    added_tool_names: None,
                    is_error: false,
                    timestamp: 0,
                };
                let entries = vec![
                    SessionEntry::Known(KnownSessionEntry::Message(
                        crate::core::session_manager::SessionMessageEntry {
                            mailbox_store_refs: None,
                            id: JsString::from("m1"),
                            parent_id: None,
                            timestamp: JsString::from("2026-06-27T00:00:00.000Z"),
                            message: Box::new(AgentMessage::Assistant(Box::new(assistant))),
                        },
                    )),
                    SessionEntry::Known(KnownSessionEntry::Message(
                        crate::core::session_manager::SessionMessageEntry {
                            mailbox_store_refs: None,
                            id: JsString::from("m2"),
                            parent_id: Some(JsString::from("m1")),
                            timestamp: JsString::from("2026-06-27T00:00:01.000Z"),
                            message: Box::new(AgentMessage::ToolResult(result)),
                        },
                    )),
                ];
                mode.render_session_entries(&entries);
                let chat = chat_text(&mode);
                assert!(chat.contains("replayed tool output"), "{chat:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn startup_renders_the_project_trust_warning() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let settings = SettingsManager::in_memory(
                    Default::default(),
                    SettingsManagerCreateOptions {
                        project_trusted: Some(false),
                    },
                );
                let (mode, _terminal, _faux, _temp) =
                    build_mode_with_settings(StaticResourceLoader::new(), None, settings).await;
                let cwd = mode.cwd();
                std::fs::create_dir_all(
                    Path::new(&encode_node_utf8(&cwd))
                        .join(crate::config::config_dir_name())
                        .join("skills"),
                )
                .expect("project resources");
                let _events = start(&mode).await;
                let chat = chat_text(&mode);
                assert!(chat.contains("This project is not trusted."), "{chat:?}");
                assert!(chat.contains("Use /trust to save a trust decision"), "{chat:?}");
            })
            .await;
    }

    #[derive(Default)]
    struct TestEditorComponent;

    impl Component for TestEditorComponent {
        fn render(&mut self, _width: usize) -> Vec<JsString> {
            vec![JsString::from("custom-editor")]
        }
    }

    impl EditorComponent for TestEditorComponent {
        fn get_text(&self) -> JsString {
            JsString::default()
        }

        fn set_text(&mut self, _text: &JsString) {}
    }

    struct TestDetachHandle {
        detached: Arc<std::sync::atomic::AtomicBool>,
    }

    impl crate::core::tool_detach_registry::ToolDetachHandle for TestDetachHandle {
        fn detach(&self) -> bool {
            self.detached.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn background_tool_keybinding_detaches_the_running_tool_and_shows_the_hint() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                let output = pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;
                assert!(output.contains("ctrl+b to background tools"), "output:\n{output:?}");

                let context = mode.session().extension_runner().create_context();
                let registry = context
                    .tool_detach_registry()
                    .expect("registry getter")
                    .expect("session registry");
                let detached = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let subscription = registry.register(Arc::new(TestDetachHandle {
                    detached: Arc::clone(&detached),
                }));
                assert!(mode.session().has_detachable_tool());

                terminal.borrow_mut().send_input("\u{2}");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Detached tool to background job")
                })
                .await;
                assert!(
                    detached.load(std::sync::atomic::Ordering::SeqCst),
                    "output:\n{output:?}"
                );
                drop(subscription);
            })
            .await;
    }

    type RestartOutcome = Arc<Mutex<Option<Result<(), JsString>>>>;
    type RestartCalls = Arc<Mutex<Vec<(Option<JsString>, JsString)>>>;

    async fn build_mode_with_restart_extension() -> (
        Rc<InteractiveMode>,
        Rc<RefCell<CaptureTerminal>>,
        tempfile::TempDir,
        RestartOutcome,
        RestartCalls,
    ) {
        let runtime = crate::core::extensions::create_extension_runtime();
        let outcome: RestartOutcome = Arc::new(Mutex::new(None));
        let command_outcome = Arc::clone(&outcome);
        let factory: crate::core::extensions::ExtensionFactory = Arc::new(move |api| {
            let command_outcome = Arc::clone(&command_outcome);
            api.register_command(
                "restart-test",
                None,
                None,
                Arc::new(
                    move |_args: JsString, ctx: crate::core::extensions::ExtensionCommandContext| {
                        let command_outcome = Arc::clone(&command_outcome);
                        Box::pin(async move {
                            let result = match ctx.restart(Some(crate::core::extensions::types::RestartOptions {
                                notice: None,
                                process: None,
                            })) {
                                Ok(future) => future.await.map_err(|error| error.message),
                                Err(error) => Err(error.message),
                            };
                            *command_outcome.lock().unwrap() = Some(result);
                            Ok(())
                        })
                            as crate::core::extensions::BoxFuture<
                                'static,
                                Result<(), crate::core::extensions::HandlerError>,
                            >
                    },
                ),
            )
            .expect("restart command");
            Box::pin(async { Ok(()) })
        });
        let extension = crate::core::extensions::load_extension_from_factory(
            factory,
            &JsString::from("/tmp"),
            Arc::new(crate::core::event_bus::EventBusController::new()),
            runtime.clone(),
            &JsString::from("/tmp/restart-extension"),
        )
        .await
        .expect("extension");
        let loader = Arc::new(StaticResourceLoader {
            extensions: Some(crate::core::extensions::LoadExtensionsResult {
                extensions: vec![extension],
                errors: Vec::new(),
                runtime,
            }),
            ..Default::default()
        }) as Arc<dyn crate::core::resource_loader::ResourceLoader>;
        let calls: RestartCalls = Arc::new(Mutex::new(Vec::new()));
        let (mode, terminal, _faux, temp) = build_mode_with_extra_providers(
            loader,
            None,
            SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default()),
            Vec::new(),
            serde_json::Map::new(),
            true,
            TestModeSetup::default(),
        )
        .await;
        (mode, terminal, temp, outcome, calls)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn command_restart_always_restarts_the_process_and_propagates_errors() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, terminal, _temp, outcome, calls) = build_mode_with_restart_extension().await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;
                let session_file = mode.session().session_file().expect("persisted session file");

                {
                    let calls = Arc::clone(&calls);
                    let mut runtime = mode.runtime.lock().await;
                    runtime.set_process_restarter(Arc::new(move |options| {
                        let calls = Arc::clone(&calls);
                        Box::pin(async move {
                            calls
                                .lock()
                                .unwrap()
                                .push((options.prompt.clone(), options.session_file.clone()));
                            Err(JsString::from("restart failed"))
                        })
                    }));
                }

                terminal.borrow_mut().send_input("/restart-test");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();

                let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
                while outcome.lock().unwrap().is_none() && tokio::time::Instant::now() < deadline {
                    mode.ui.process_events();
                    mode.ui.flush_pending();
                    mode.poll_timers();
                    while let Ok(event) = events.try_recv() {
                        mode.handle_event(event).await;
                    }
                    mode.handle_ui_requests().await;
                    mode.drain_actions().await;
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }

                assert_eq!(
                    outcome.lock().unwrap().clone(),
                    Some(Err(JsString::from("restart failed"))),
                    "the extension caller observes the restart failure"
                );
                let calls = calls.lock().unwrap().clone();
                assert_eq!(calls, vec![(None, session_file)], "restart must use the process path");
            })
            .await;
    }
    async fn build_mode_for_stop(
        settings: crate::core::settings_manager::Settings,
    ) -> (Rc<InteractiveMode>, Rc<RefCell<CaptureTerminal>>, tempfile::TempDir) {
        let (mode, terminal, _faux, temp) = build_mode_with_extra_providers(
            StaticResourceLoader::new(),
            None,
            SettingsManager::in_memory(settings, SettingsManagerCreateOptions::default()),
            Vec::new(),
            serde_json::Map::new(),
            false,
            TestModeSetup {
                multi_agent_store: None,
            },
        )
        .await;
        (mode, terminal, temp)
    }

    fn stop_settings(value: Value) -> crate::core::settings_manager::Settings {
        value
            .as_object()
            .expect("settings object")
            .clone()
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>()
    }

    struct DisposingFooter(std::sync::Arc<std::sync::atomic::AtomicBool>);

    impl crate::core::extensions::types::ExtensionUiComponent for DisposingFooter {
        fn render(&self, _width: usize) -> Vec<JsString> {
            Vec::new()
        }

        fn dispose(&self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn prepared_user_message_renders_at_submit_and_is_reused() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let message = AgentMessage::User(pi_ai::types::UserMessage {
                    role: pi_ai::types::UserRole::Tag,
                    content: pi_ai::types::UserContent::Text("prepared-bubble-marker".into()),
                    input_source: None,
                    provider_native: None,
                    timestamp: 1,
                });

                prepared_user_message_callback()(&message);

                let prepared: Vec<ComponentRef> = {
                    let entries = mode.provisional_user_messages.borrow();
                    assert_eq!(entries.len(), 1, "submit renders one provisional bubble");
                    entries[0].1.clone()
                };
                let chat = mode.chat_container.borrow().children().to_vec();
                assert!(
                    prepared
                        .iter()
                        .all(|component| chat.iter().any(|child| Rc::ptr_eq(child, component)))
                );

                assert!(mode.consume_prepared_user_message(&message));
                assert!(mode.provisional_user_messages.borrow().is_empty());
                assert!(!mode.consume_prepared_user_message(&message));
                let chat = mode.chat_container.borrow().children().to_vec();
                assert!(
                    prepared
                        .iter()
                        .all(|component| chat.iter().any(|child| Rc::ptr_eq(child, component))),
                    "message_start reuses the submit-time components"
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stop_clears_terminal_progress_and_disposes_footer_components() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let settings = stop_settings(serde_json::json!({
                    "terminal": {"showTerminalProgress": true}
                }));
                let (mode, terminal, _temp) = build_mode_for_stop(settings).await;
                let _events = start(&mode).await;
                let disposed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let footer_flag = std::sync::Arc::clone(&disposed);
                mode.set_extension_footer(Some(std::sync::Arc::new(move |_context| {
                    Box::new(DisposingFooter(std::sync::Arc::clone(&footer_flag)))
                        as Box<dyn crate::core::extensions::types::ExtensionUiComponent>
                })));
                mode.pending_tools
                    .borrow_mut()
                    .insert(JsString::from("tool-1"), component(Text::new("pending", 0, 0)));
                let listener = mode.ui.add_input_listener(Rc::new(|_data: &JsString| {
                    pi_tui::tui::TuiInputListenerResult::default()
                }));
                mode.terminal_input_listeners
                    .borrow_mut()
                    .push((listener, std::sync::Arc::new(|_data: &JsString| None)));
                terminal.borrow_mut().progress.clear();

                mode.stop();

                assert!(disposed.load(std::sync::atomic::Ordering::SeqCst));
                assert_eq!(terminal.borrow().progress.last(), Some(&false));
                assert!(mode.pending_tools.borrow().is_empty());
                assert!(mode.terminal_input_listeners.borrow().is_empty());
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn js_path(path: &Path) -> JsString {
        JsString::from(path.to_str().expect("utf-8 test path"))
    }

    fn init_dark_theme() {
        let theme_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("theme");
        let dark = crate::modes::interactive::theme::load_theme_from_path(&js_path(&theme_dir.join("dark.json")), None)
            .expect("dark theme");
        let light =
            crate::modes::interactive::theme::load_theme_from_path(&js_path(&theme_dir.join("light.json")), None)
                .expect("light theme");
        crate::modes::interactive::theme::set_registered_themes(vec![
            std::sync::Arc::new(dark),
            std::sync::Arc::new(light),
        ])
        .expect("register fixture themes");
        crate::modes::interactive::theme::init_theme(Some(&JsString::from("dark")), false).expect("dark theme");
    }

    fn source_info(path: &str, source: &str, scope: SourceScope, base_dir: Option<&str>) -> SourceInfo {
        SourceInfo {
            path: JsString::from(path.to_owned()),
            source: JsString::from(source.to_owned()),
            scope,
            origin: crate::core::source_info::SourceOrigin::TopLevel,
            base_dir: base_dir.map(str::to_owned).map(JsString::from),
        }
    }

    #[test]
    fn quote_if_needed_matches_shell_safe_rule() {
        assert_eq!(quote_if_needed(&JsString::from("simple-value_1.2")), "simple-value_1.2");
        assert_eq!(quote_if_needed(&JsString::from("")), "''");
        assert_eq!(quote_if_needed(&JsString::from("a b")), "'a b'");
        assert_eq!(quote_if_needed(&JsString::from("it's")), "'it'\\''s'");
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
            (JsString::from("/tmp/extensions/answer.ts"), None),
            (JsString::from("/tmp/extensions/btw.ts"), None),
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
                JsString::from("/tmp/alpha/one/index.ts"),
                Some(source_info(
                    "/tmp/alpha/one/index.ts",
                    "cli",
                    SourceScope::Temporary,
                    Some("/tmp/alpha"),
                )),
            ),
            (
                JsString::from("/tmp/beta/one/index.ts"),
                Some(source_info(
                    "/tmp/beta/one/index.ts",
                    "cli",
                    SourceScope::Temporary,
                    Some("/tmp/beta"),
                )),
            ),
            (
                JsString::from("/tmp/gamma/one/index.ts"),
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
                &JsString::from("/tmp/project/.pi/npm/node_modules/pi-markdown-preview/extensions/index.ts"),
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
                &JsString::from(
                    "/tmp/project/.pi/git/github.com/HazAT/pi-interactive-subagents/extensions/subagents/index.ts"
                ),
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
                label: JsString::from("project"),
                scope_label: None,
                color: ThemeColor::Muted,
            }
        );
        let temporary = source_info("/tmp/x.ts", "local", SourceScope::Temporary, None);
        assert_eq!(
            get_display_source_info(Some(&temporary)),
            DisplaySourceInfo {
                label: JsString::from("path"),
                scope_label: Some(JsString::from("temp")),
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
                path: JsString::from("/tmp/project/.pi/skills/b"),
                source_info: None,
            },
            ScopeGroupItem {
                path: JsString::from("/tmp/project/.pi/skills/a"),
                source_info: None,
            },
            ScopeGroupItem {
                path: JsString::from("/home/u/.pi/skills/c"),
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
            JsString::from("/tmp/project/.pi/npm/node_modules/pkg"),
            source_info(
                "/tmp/project/.pi/npm/node_modules/pkg",
                "npm:pkg",
                SourceScope::Project,
                None,
            ),
        );
        let found = find_source_info_for_path(
            &JsString::from("/tmp/project/.pi/npm/node_modules/pkg/extensions/a.ts"),
            &infos,
        );
        assert_eq!(found.map(|info| &info.source), Some(&JsString::from("npm:pkg")));
    }

    #[test]
    fn diagnostics_render_collision_groups_and_other_entries() {
        init_dark_theme();
        let diagnostics = vec![
            ResourceDiagnostic {
                diagnostic_type: ResourceDiagnosticType::Collision,
                message: JsString::from("collision"),
                path: None,
                collision: Some(crate::core::diagnostics::ResourceCollision {
                    resource_type: crate::core::diagnostics::ResourceType::Skill,
                    name: JsString::from("commit"),
                    winner_path: JsString::from("/a/winner"),
                    loser_path: JsString::from("/b/loser"),
                    winner_source: None,
                    loser_source: None,
                }),
            },
            ResourceDiagnostic {
                diagnostic_type: ResourceDiagnosticType::Warning,
                message: JsString::from("bad file"),
                path: Some(JsString::from("/c/file")),
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
            &[
                JsString::from("read"),
                JsString::from(" bash "),
                JsString::default(),
                JsString::from("edit"),
            ],
            true,
        );
        let plain = pi_tui::utils::strip_terminal_sequences(&output);
        assert_eq!(plain, "  bash, edit, read");
    }

    #[test]
    fn extension_display_path_strips_index() {
        assert_eq!(
            format_extension_display_path(&JsString::from("/tmp/extensions/plan-mode/index.ts")),
            "/tmp/extensions/plan-mode"
        );
        assert_eq!(
            format_extension_display_path(&JsString::from("/tmp/extensions/plan-mode/index.js")),
            "/tmp/extensions/plan-mode"
        );
        assert_eq!(
            format_extension_display_path(&JsString::from("/tmp/extensions/webfetch.ts")),
            "/tmp/extensions/webfetch.ts"
        );
    }

    #[test]
    fn expandable_text_switches_bodies() {
        use pi_tui::tui::Component;

        let mut text = ExpandableText::new(
            || JsString::from("collapsed"),
            || JsString::from("expanded"),
            false,
            0,
            0,
        );
        assert_eq!(text.render(40)[0].trim_end(), "collapsed");
        text.set_expanded(true);
        assert_eq!(text.render(40)[0].trim_end(), "expanded");
        text.set_expanded(false);
        assert_eq!(text.render(40)[0].trim_end(), "collapsed");
    }

    #[test]
    fn unix_relative_paths_match_posix_relative() {
        assert_eq!(
            posix_relative(&JsString::from("/a/b"), &JsString::from("/a/b/c/d")),
            "c/d"
        );
        assert_eq!(
            posix_relative(&JsString::from("/a/b/c"), &JsString::from("/a/d")),
            "../../d"
        );
        assert_eq!(posix_relative(&JsString::from("/a"), &JsString::from("/a")), "");
    }

    #[test]
    fn path_segments_ignore_root_and_home_markers() {
        assert_eq!(
            get_compact_display_path_segments(&JsString::from("/tmp/x/index.ts")),
            vec!["tmp".to_owned(), "x".to_owned(), "index.ts".to_owned()]
        );
    }

    #[test]
    fn parsed_posix_paths_match_node_shape() {
        assert_eq!(parse_posix_path(&JsString::from("index.ts")).dir, "");
        assert_eq!(parse_posix_path(&JsString::from("index.ts")).name, "index");
        assert_eq!(parse_posix_path(&JsString::from("subagents/index.ts")).dir, "subagents");
        assert_eq!(parse_posix_path(&JsString::from("subagents/index.ts")).name, "index");
        assert_eq!(parse_posix_path(&JsString::from("answer.ts")).name, "answer");
        for (path, dir) in [
            ("/a.ts", "/"),
            ("foo//", ""),
            ("//foo", "/"),
            ("a//b.ts", "a/"),
            ("///", "/"),
            ("", ""),
        ] {
            assert_eq!(parse_posix_path(&JsString::from(path)).dir, dir, "path {path}");
        }
    }

    #[test]
    fn uncaught_crash_exits_129_for_a_dead_terminal_before_checking_shutdown() {
        assert_eq!(crash_action(true, true), CrashAction::EmergencyExit);
        assert_eq!(crash_action(true, false), CrashAction::EmergencyExit);
    }

    #[test]
    fn uncaught_crash_exits_one_while_already_shutting_down() {
        assert_eq!(crash_action(false, true), CrashAction::ExitOne);
    }

    #[test]
    fn uncaught_crash_restores_the_terminal_otherwise() {
        assert_eq!(crash_action(false, false), CrashAction::RestoreAndReport);
    }

    #[test]
    fn terminal_errors_route_dead_raw_mode_and_other_errors_separately() {
        let eio = std::io::Error::from_raw_os_error(rustix::io::Errno::IO.raw_os_error());
        let enospc = std::io::Error::from_raw_os_error(rustix::io::Errno::NOSPC.raw_os_error());
        let bun_raw_mode = std::io::Error::other("setRawMode failed with errno: 5");

        assert_eq!(terminal_error_action(&eio), TerminalErrorAction::EmergencyExit);
        assert_eq!(terminal_error_action(&bun_raw_mode), TerminalErrorAction::Ignore);
        assert_eq!(terminal_error_action(&enospc), TerminalErrorAction::Rethrow);
        assert_eq!(
            terminal_error_action(&std::io::Error::other("write failed")),
            TerminalErrorAction::Rethrow
        );
    }

    #[test]
    fn only_io_error_payloads_can_be_dead_terminal_errors() {
        let eio = std::io::Error::from_raw_os_error(rustix::io::Errno::IO.raw_os_error());
        let plain = std::io::Error::other("write failed");

        assert!(payload_is_dead_terminal_error(&eio));
        assert!(!payload_is_dead_terminal_error(&plain));
        assert!(!payload_is_dead_terminal_error(&String::from("EIO")));
    }

    #[test]
    fn pathbuf_components_are_used_for_relative_paths() {
        assert_eq!(relative_path("/tmp/a", "/tmp/a/b"), Ok(JsString::from("b")));
        assert!(matches!(
            Path::new("/tmp/a").components().next(),
            Some(std::path::Component::RootDir)
        ));
    }
}

#[cfg(test)]
mod lossless_preview_tests {
    use super::*;
    mod oracle {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
    }
    #[test]
    fn followup_slices_raw_units_before_display_encoding() {
        let mut oracle = oracle::TsOracle::spawn(
            "interactive-followup-oracle.ts",
            "packages/coding-agent/src/modes/interactive/interactive-mode.ts",
        );
        for input in [
            JsString::from(format!("- agent: helper\nMessage:\n{}😀", "x".repeat(49))),
            JsString::from("Continue working toward the active goal."),
            JsString::from_utf16(vec![0xd800, 0x20, 0xdc00]),
        ] {
            let request = JsValue::Object(JsObject::from_iter([(
                JsString::from("message"),
                JsValue::String(input.clone()),
            )]));
            let expected = oracle.request(&request);
            let actual = JsValue::String(format_follow_up_preview(&input));
            assert_eq!(actual, expected, "input {input:?}");
        }
    }
}

#[cfg(test)]
mod lossless_ui_bridge_tests {
    use super::*;
    #[test]
    fn native_extension_ui_bridge_roundtrips_source_units() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let context = InteractiveUiContext {
            id: 0,
            sender,
            shared: Arc::new(UiShared::new()),
        };
        let key = JsString::from_utf16(vec![0xd800]);
        let text = JsString::from_utf16(vec![0xdc00]);
        context.set_status(&key, Some(&text));
        match receiver.recv().unwrap() {
            UiRequest::SetStatus {
                key: actual,
                text: Some(value),
            } => {
                assert_eq!(actual, key);
                assert_eq!(value, text);
            }
            _ => panic!("status not delivered"),
        }
        context.set_widget(&key, Some(&[text.clone()]), None);
        match receiver.recv().unwrap() {
            UiRequest::SetWidget {
                key: actual,
                content: Some(lines),
                ..
            } => {
                assert_eq!(actual, key);
                assert_eq!(lines, vec![text.clone()]);
            }
            _ => panic!("widget not delivered"),
        }
        context.set_title(&text);
        match receiver.recv().unwrap() {
            UiRequest::SetTitle(value) => assert_eq!(value, text),
            _ => panic!("title not delivered"),
        }
        let selected = context.select(&key, &[text.clone()], None);
        match receiver.recv().unwrap() {
            UiRequest::Select {
                title,
                options,
                respond,
                ..
            } => {
                assert_eq!(title, key);
                assert_eq!(options, vec![text.clone()]);
                respond.send(Some(text.clone())).unwrap();
            }
            _ => panic!("selection not delivered"),
        }
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        assert_eq!(runtime.block_on(selected).unwrap(), Some(text));
    }
}
