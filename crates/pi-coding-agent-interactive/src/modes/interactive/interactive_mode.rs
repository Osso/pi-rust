//! Port of `coding-agent/src/modes/interactive/interactive-mode.ts`.
//!
//! The TypeScript class is the composition root for the interactive TUI. This
//! module keeps the same structure: pure formatting helpers first, then the
//! mode itself. Component factories and the TUI event loop are Rust-native;
//! deviations are recorded in `docs/porting/traceability/interactive-mode.md`.

use crate::modes::interactive::login_options::LoginAuthType;
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
use pi_ai::auth::types::{AuthEvent, AuthInteraction, AuthPrompt, AuthPromptKind};
use pi_ai::request::{CallbackError, Fetch, HttpRequest};
use pi_ai::types::{AssistantMessage, ImageContent, Model};
use pi_js_compat::abort::AbortReason;
use pi_js_compat::date::js_now_millis;
use pi_js_compat::js_format;
use pi_js_compat::js_regex::JsRegex as Regex;
use pi_js_compat::number::to_locale_string;
use pi_tui::EditorComponent;
use pi_tui::ProcessTerminal;
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
    TuiExt, TuiInputListenerResult, component,
};

use crate::core::agent_session::{AgentSession, AgentSessionEvent, CycleDirection, PromptOptions, StreamingBehavior};
use crate::core::agent_session_runtime::AgentSessionRuntime;
use crate::core::diagnostics::{ResourceDiagnostic, ResourceDiagnosticType};
use crate::core::extensions::types::{
    BoxFuture, ExtensionAutocompleteProviderFactory, ExtensionEditorFactory, ExtensionUiComponent,
    ExtensionUiComponentFactory, ExtensionUiContext, ExtensionUiCustomFactory, ExtensionUiCustomOptions,
    ExtensionUiDialogOptions, ExtensionUiDone, ExtensionUiFooterFactory, ExtensionUiHeaderFactory, ExtensionUiTui,
    ExtensionUiTuiBackend, ExtensionWidgetOptions, HandlerError, HeaderExpansionHandler, NotifyType,
    SessionTransitionResult, TerminalInputHandler, ThemeInfo, ThemeSetResult, WidgetPlacement, WorkingIndicatorOptions,
};
use crate::core::footer_data_provider::{ContextUsage, ExtensionStatus, FooterDataProvider, FooterSessionOverride};
use crate::core::keybindings::KeybindingsManager;
use crate::core::model_resolver::{find_exact_model_reference_match, resolve_model_scope_from_models};
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
use crate::modes::interactive::components::footer::FooterComponent;
use crate::modes::interactive::components::keybinding_hints::{key_hint, key_text, raw_key_hint};
use crate::modes::interactive::components::login_dialog::LoginDialogComponent;
use crate::modes::interactive::components::model_selector::{ModelSelectorComponent, ModelSelectorUi};
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
use crate::modes::interactive::components::tool_execution::{
    ToolExecutionComponent, ToolExecutionOptions, ToolExecutionResult, ToolExecutionStartOptions,
};
use crate::modes::interactive::components::tree_selector::{FilterMode, TreeSelectorComponent};
use crate::modes::interactive::components::trust_selector::{TrustSelectorComponent, TrustSelectorOptions};
use crate::modes::interactive::components::user_message::UserMessageComponent;
use crate::modes::interactive::components::user_message_selector::{UserMessageItem, UserMessageSelectorComponent};
use crate::modes::interactive::interactive_root_compositor::{
    InteractiveRootCompositorOptions, create_interactive_root_compositor,
};
use crate::modes::interactive::login_options::get_login_provider_options;
use crate::modes::interactive::login_options::get_logout_provider_options;
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

const ANTHROPIC_SUBSCRIPTION_AUTH_WARNING: &str = "Anthropic subscription auth is active. Third-party harness usage draws from extra usage and is billed per token, not your Claude plan limits. Manage extra usage at https://claude.ai/settings/usage.";
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

/// Frozen `isDeadTerminalError` checks `.code`, not the thrown object's class.
fn payload_is_dead_terminal_error(payload: &(dyn Any + Send)) -> bool {
    use pi_js_compat::abort::AbortReason;
    if let Some(error) = payload.downcast_ref::<std::io::Error>() {
        return is_dead_terminal_error(error);
    }
    let value = if let Some(error) = payload.downcast_ref::<pi_js_compat::node_error::NativeNodeError>() {
        error.value()
    } else if let Some(error) = payload.downcast_ref::<CallbackError>() {
        match error.thrown_value() {
            AbortReason::Error(error) | AbortReason::DomException(error) => error.value(),
            AbortReason::Value(value) => value,
        }
    } else {
        return false;
    };
    value
        .get("code")
        .and_then(JsValue::as_string)
        .is_some_and(|code| code == "EIO" || code == "EPIPE" || code == "ENOTCONN")
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
    crate::parse_skill_block(crate::core::prompt_templates::expand_prompt_template(
        JsString::from(text),
        prompt_templates,
    ))
    .is_some()
}

/// `formatResumeCommand`: printable `pi --session ...` hint.
pub fn format_resume_command(
    session_manager: &crate::core::session_manager::SessionManager,
) -> Result<Option<JsString>, crate::core::session_manager::SessionManagerError> {
    use std::io::IsTerminal;

    if !std::io::stdout().is_terminal() {
        return Ok(None);
    }
    if !session_manager.is_persisted() {
        return Ok(None);
    }
    let Some(session_file) = session_manager.get_session_file() else {
        return Ok(None);
    };
    if !Path::new(&encode_node_utf8(session_file)).exists() {
        return Ok(None);
    }

    let mut args = vec![crate::config::package_config().app_name.clone()];
    if !session_manager.uses_default_session_dir()? {
        args.push(JsString::from("--session-dir"));
        args.push(quote_if_needed(session_manager.get_session_dir()));
    }
    args.push(JsString::from("--session"));
    args.push(session_manager.get_session_id().to_owned());
    Ok(Some(JsString::join(args, " ")))
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
    JsString::from("Follow-up from ") + sender + ": " + preview
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

fn format_session_info(stats: &crate::core::agent_session::SessionStats, name: Option<&JsString>) -> JsString {
    with_theme(|theme| {
        let mut info = js_format!("{}\n\n", theme.bold("Session Info"));
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            info.push_str(&js_format!("{} {}\n", theme.fg(ThemeColor::Dim, "Name:"), name));
        }
        info.push_str(&js_format!(
            "{} {}\n",
            theme.fg(ThemeColor::Dim, "File:"),
            stats.session_file.as_ref().unwrap_or(&JsString::from("In-memory"))
        ));
        info.push_str(&js_format!(
            "{} {}\n\n",
            theme.fg(ThemeColor::Dim, "ID:"),
            stats.session_id
        ));
        info.push_str(&js_format!("{}\n", theme.bold("Messages")));
        for (label, count) in [
            ("User:", stats.user_messages),
            ("Assistant:", stats.assistant_messages),
            ("Tool Calls:", stats.tool_calls),
            ("Tool Results:", stats.tool_results),
            ("Total:", stats.total_messages),
        ] {
            info.push_str(&js_format!("{} {}\n", theme.fg(ThemeColor::Dim, label), count));
        }
        info.push_str(&js_format!("\n{}\n", theme.bold("Tokens")));
        for (label, value) in [("Input:", stats.tokens.input), ("Output:", stats.tokens.output)] {
            info.push_str(&js_format!(
                "{} {}\n",
                theme.fg(ThemeColor::Dim, label),
                to_locale_string(value)
            ));
        }
        if stats.tokens.cache_read > 0.0 {
            info.push_str(&js_format!(
                "{} {}\n",
                theme.fg(ThemeColor::Dim, "Cache Read:"),
                to_locale_string(stats.tokens.cache_read)
            ));
        }
        if stats.tokens.cache_write > 0.0 {
            info.push_str(&js_format!(
                "{} {}\n",
                theme.fg(ThemeColor::Dim, "Cache Write:"),
                to_locale_string(stats.tokens.cache_write)
            ));
        }
        info.push_str(&js_format!(
            "{} {}\n",
            theme.fg(ThemeColor::Dim, "Total:"),
            to_locale_string(stats.tokens.total)
        ));
        if stats.cost > 0.0 {
            info.push_str(&js_format!(
                "\n{}\n{} {}",
                theme.bold("Cost"),
                theme.fg(ThemeColor::Dim, "Total:"),
                pi_js_compat::number::to_fixed(stats.cost, 4)
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

type ResourceDisplayResult<T> = Result<T, pi_js_compat::node_error::NativeNodeError>;

pub(crate) fn format_display_path(path: &JsString) -> ResourceDisplayResult<JsString> {
    let home = pi_js_compat::node_os::homedir()?;
    if path.starts_with(&home) {
        return Ok(js_format!("~{}", path.slice(home.len(), path.len())));
    }
    Ok(path.to_owned())
}

pub(crate) fn format_extension_display_path(path: &JsString) -> ResourceDisplayResult<JsString> {
    let display = format_display_path(path)?;
    Ok(display
        .strip_suffix("/index.ts")
        .or_else(|| display.strip_suffix("/index.js"))
        .unwrap_or(display.clone())
        .to_owned())
}

pub(crate) fn format_context_path(path: &JsString, cwd: &JsString) -> ResourceDisplayResult<JsString> {
    let cwd = resolve_path_string(cwd);
    let absolute = if node_is_absolute(path) {
        resolve_path_string(path)
    } else {
        resolve_path_string(&js_format!("{}/{}", cwd, path))
    };
    if let Ok(Some(relative)) = get_cwd_relative_path(&absolute, &cwd) {
        return Ok(relative);
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

pub(crate) fn get_short_path(
    full_path: &JsString,
    source_info: Option<&SourceInfo>,
) -> ResourceDisplayResult<JsString> {
    let normalized_full_path = normalize_slashes(full_path);
    if let Some(source_info) = source_info
        && let Some(base_dir) = source_info.base_dir.as_ref()
        && is_package_source(Some(source_info))
    {
        let normalized_base_dir = normalize_slashes(base_dir);
        if let Some(captures) = package_path_regex().captures(&normalized_base_dir) {
            let root = captures.get(1).map(|capture| capture.text()).unwrap_or_default();
            if !root.is_empty() && normalized_full_path.starts_with(&js_format!("{}/", root)) {
                return Ok(posix_relative(&normalized_base_dir, &normalized_full_path));
            }
        }

        let relative = relative_path(resolve_path_string(base_dir), resolve_path_string(full_path))
            .unwrap_or_else(|error| throw_uncaught(ERROR, error.message().clone()));
        if !relative.is_empty()
            && relative != "."
            && !relative.starts_with("..")
            && !relative.starts_with(&js_format!("..{}", std::path::MAIN_SEPARATOR))
            && !node_is_absolute(&relative)
        {
            return Ok(normalize_slashes(&relative));
        }
    }

    let source = source_info.map(|info| info.source.clone()).unwrap_or_default();
    if let Some(captures) = node_modules_path_regex().captures(&normalized_full_path)
        && source.starts_with("npm:")
        && let Some(path) = captures.get(2)
    {
        return Ok(path.text());
    }
    if let Some(captures) = git_path_regex().captures(&normalized_full_path)
        && source.starts_with("git:")
        && let Some(path) = captures.get(1)
    {
        return Ok(path.text());
    }
    format_display_path(full_path)
}

pub(crate) fn get_compact_path_label(
    resource_path: &JsString,
    source_info: Option<&SourceInfo>,
) -> ResourceDisplayResult<JsString> {
    let short_path = get_short_path(resource_path, source_info)?;
    let normalized_path = normalize_slashes(&short_path);
    let segments: Vec<JsString> = normalized_path
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "~")
        .collect();
    if let Some(last) = segments.last() {
        return Ok((*last).to_owned());
    }
    Ok(short_path)
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

pub(crate) fn get_compact_extension_label(
    resource_path: &JsString,
    source_info: Option<&SourceInfo>,
) -> ResourceDisplayResult<JsString> {
    if !is_package_source(source_info) {
        return get_compact_path_label(resource_path, source_info);
    }
    let source_label = get_compact_package_source_label(source_info);
    if source_label.is_empty() {
        return get_compact_path_label(resource_path, source_info);
    }
    let short_path = normalize_slashes(&get_short_path(resource_path, source_info)?);
    let package_path = short_path.strip_prefix("extensions/").unwrap_or(short_path.clone());
    let parsed = parse_posix_path(&package_path);
    if parsed.name == "index" {
        if parsed.dir.is_empty() || parsed.dir == "." {
            return Ok(source_label);
        }
        return Ok(js_format!("{}:{}", source_label, parsed.dir));
    }
    Ok(js_format!("{}:{}", source_label, package_path))
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

pub(crate) fn get_compact_display_path_segments(resource_path: &JsString) -> ResourceDisplayResult<Vec<JsString>> {
    Ok(normalize_slashes(&format_display_path(resource_path)?)
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != "~")
        .collect())
}

fn compact_non_package_extension_label(
    resource_path: &JsString,
    index: usize,
    all_paths: &[Vec<JsString>],
) -> ResourceDisplayResult<JsString> {
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
            return Ok(candidate);
        }
    }
    Ok(JsString::join(segments, "/"))
}

pub(crate) fn get_compact_extension_labels(
    extensions: &[(JsString, Option<SourceInfo>)],
) -> ResourceDisplayResult<Vec<JsString>> {
    let non_package: Vec<&(JsString, Option<SourceInfo>)> = extensions
        .iter()
        .filter(|(_, source_info)| !is_package_source(source_info.as_ref()))
        .collect();
    let non_package_segments: Vec<Vec<JsString>> = non_package
        .iter()
        .map(|(path, _)| {
            let mut segments = get_compact_display_path_segments(path)?;
            if segments.len() > 1
                && segments
                    .last()
                    .is_some_and(|last| last == "index.ts" || last == "index.js")
            {
                segments.pop();
            }
            Ok(segments)
        })
        .collect::<ResourceDisplayResult<_>>()?;

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
    format_path: impl Fn(&ScopeGroupItem) -> ResourceDisplayResult<JsString>,
    format_package_path: impl Fn(&ScopeGroupItem, &JsString) -> ResourceDisplayResult<JsString>,
) -> ResourceDisplayResult<JsString> {
    with_theme(|theme| {
        let mut lines: Vec<JsString> = Vec::new();
        for group in groups {
            lines.push(js_format!("  {}", theme.fg(ThemeColor::Accent, group.scope.as_str())));

            let mut sorted_paths: Vec<&ScopeGroupItem> = group.paths.iter().collect();
            sorted_paths.sort_by(|left, right| {
                pi_js_compat::collator::locale_compare_utf16(left.path.units(), right.path.units())
            });
            for item in sorted_paths {
                lines.push(theme.fg(ThemeColor::Dim, &js_format!("    {}", format_path(item)?)));
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
                        &js_format!("      {}", format_package_path(item, source)?),
                    ));
                }
            }
        }
        Ok(JsString::join(lines, "\n"))
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

pub(crate) fn format_path_with_source(
    path: &JsString,
    source_info: Option<&SourceInfo>,
) -> ResourceDisplayResult<JsString> {
    if let Some(source_info) = source_info {
        let short_path = get_short_path(path, Some(source_info))?;
        let display = get_display_source_info(Some(source_info));
        let label_text = match display.scope_label {
            Some(scope_label) => js_format!("{} ({})", display.label, scope_label),
            None => display.label,
        };
        return Ok(js_format!("{} {}", label_text, short_path));
    }
    format_display_path(path)
}

pub(crate) fn format_diagnostics(
    diagnostics: &[ResourceDiagnostic],
    source_infos: &HashMap<JsString, SourceInfo>,
) -> ResourceDisplayResult<JsString> {
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

        push_collision_lines(&mut lines, theme, &collisions, source_infos)?;
        push_other_diagnostic_lines(&mut lines, theme, &other, source_infos)?;
        Ok(JsString::join(lines, "\n"))
    })
}

fn push_collision_lines(
    lines: &mut Vec<JsString>,
    theme: &Theme,
    collisions: &IndexMap<JsString, Vec<&ResourceDiagnostic>>,
    source_infos: &HashMap<JsString, SourceInfo>,
) -> ResourceDisplayResult<()> {
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
                )?
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
                        )?
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn push_other_diagnostic_lines(
    lines: &mut Vec<JsString>,
    theme: &Theme,
    other: &[&ResourceDiagnostic],
    source_infos: &HashMap<JsString, SourceInfo>,
) -> ResourceDisplayResult<()> {
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
                        format_path_with_source(path, find_source_info_for_path(path, source_infos))?
                    ),
                ));
                lines.push(theme.fg(color, &js_format!("    {}", diagnostic.message)));
            }
            None => lines.push(theme.fg(color, &js_format!("  {}", diagnostic.message))),
        }
    }
    Ok(())
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

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
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

    pub fn terminal(&self) -> Rc<RefCell<dyn Terminal>> {
        match self {
            Self::Main(tui) => TuiExt::terminal(tui.as_ref()),
        }
    }

    pub fn start(&self) -> Result<(), CallbackError> {
        match self {
            Self::Main(tui) => TuiExt::start(tui.as_ref()),
        }
    }

    pub fn stop(&self) -> Result<(), CallbackError> {
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

impl ModelSelectorUi for InteractiveTuiHandle {
    fn request_render(&self) {
        InteractiveTuiHandle::request_render(self);
    }
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

    pub fn request_component_render(&self, component: &ComponentRef) -> Result<bool, CallbackError> {
        match self.renderer() {
            Renderer::Main(tui) => tui.request_component_render(component),
        }
    }

    pub fn render_now(&self) -> Result<(), CallbackError> {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::render_now(tui.as_ref(), true),
        }
    }

    pub fn invalidate(&self) {
        match self.renderer() {
            Renderer::Main(tui) => TuiExt::invalidate(tui.as_ref()),
        }
    }

    pub fn start(&self) -> Result<(), CallbackError> {
        self.renderer.borrow().clone_ref().start()
    }

    pub fn stop(&self) -> Result<(), CallbackError> {
        self.renderer.borrow().clone_ref().stop()
    }

    pub fn flush_pending(&self) -> Result<(), CallbackError> {
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
    fn suspend(&self) -> Result<(), CallbackError> {
        self.0.stop()
    }

    fn resume(&self) -> Result<(), CallbackError> {
        self.0.start()
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
        let on_abort = self.0.borrow().abort_for_input(data);
        if let Some(on_abort) = on_abort {
            on_abort();
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
        let input = self.0.borrow_mut().handle_input_deferred(data);
        input.run(|resume| resume(&mut *self.0.borrow_mut()));
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
    pub initial_images: Option<Vec<ImageContent>>,
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
    RefreshEditorBorder,
    CycleModel(CycleDirection),
    NewSession,
}

/// Shared, thread-safe counters used by the extension UI bridge.
pub struct UiShared {
    owner_thread: std::thread::ThreadId,
    rows: std::sync::atomic::AtomicUsize,
    columns: std::sync::atomic::AtomicUsize,
    editor_component_factory: std::sync::Mutex<Option<ExtensionEditorFactory>>,
}

impl UiShared {
    fn new() -> Self {
        Self {
            owner_thread: std::thread::current().id(),
            rows: std::sync::atomic::AtomicUsize::new(24),
            columns: std::sync::atomic::AtomicUsize::new(80),
            editor_component_factory: std::sync::Mutex::new(None),
        }
    }
}

/// Requests crossing from extension threads into the TUI thread.
enum UiRequest {
    LifecycleTransition(pi_ai::request::BoxFuture<'static, ()>),
    RendererCompletion(pi_ai::request::BoxFuture<'static, ()>),
    BashOutput(JsString),
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
    RequestComponentRender(usize),
    RequestRenderForce,
    SetStatus {
        key: JsString,
        text: Option<ExtensionStatus>,
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
    SetHeader {
        factory: Option<ExtensionUiHeaderFactory>,
        respond: std::sync::mpsc::Sender<Result<(), CallbackError>>,
    },
    Custom {
        factory: ExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
        respond: tokio::sync::oneshot::Sender<Result<Option<JsValue>, CallbackError>>,
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
        respond: tokio::sync::oneshot::Sender<Result<SessionTransitionResult, CallbackError>>,
    },
    RuntimeFork {
        entry_id: JsString,
        options: Option<crate::core::extensions::types::ForkOptions>,
        respond: tokio::sync::oneshot::Sender<Result<SessionTransitionResult, CallbackError>>,
    },
    RuntimeNavigateTree {
        entry_id: JsString,
        options: Option<crate::core::extensions::types::NavigateTreeOptions>,
        respond: tokio::sync::oneshot::Sender<Result<SessionTransitionResult, CallbackError>>,
    },
    RuntimeSwitchSession {
        path: JsString,
        options: Option<crate::core::extensions::types::SwitchSessionOptions>,
        respond: tokio::sync::oneshot::Sender<Result<SessionTransitionResult, CallbackError>>,
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
    pi_js_compat::array_sort::js_array_sort(&mut sessions, |a, b| {
        let a_named = session_has_name(a);
        let b_named = session_has_name(b);
        if a_named != b_named {
            return if a_named { -1.0 } else { 1.0 };
        }
        b.modified - a.modified
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
async fn await_ui_response<T, E: Into<HandlerError>>(
    receiver: tokio::sync::oneshot::Receiver<Result<T, E>>,
) -> Result<T, HandlerError> {
    receiver
        .await
        .map_err(|error| HandlerError::new(error.to_string()))?
        .map_err(Into::into)
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
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Select {
            title: title.to_owned(),
            options: options.to_vec(),
            timeout: opts.and_then(|opts| opts.timeout),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| CallbackError::new(error.to_string())) })
    }

    fn confirm<'a>(
        &'a self,
        title: &JsString,
        message: &JsString,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, CallbackError>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Confirm {
            title: title.to_owned(),
            message: message.to_owned(),
            timeout: opts.and_then(|opts| opts.timeout),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| CallbackError::new(error.to_string())) })
    }

    fn input<'a>(
        &'a self,
        title: &JsString,
        placeholder: Option<&JsString>,
        opts: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Input {
            title: title.to_owned(),
            placeholder: placeholder.cloned(),
            timeout: opts.and_then(|opts| opts.timeout),
            secret: opts.and_then(|opts| opts.secret).unwrap_or(false),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| CallbackError::new(error.to_string())) })
    }

    fn notify(
        &self,
        message: &JsString,
        notify_type: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.send(UiRequest::Notify {
            message: message.to_owned(),
            notify_type,
        });
        Ok(())
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

    fn set_status(&self, key: &JsString, text: Option<ExtensionStatus>) -> Result<(), CallbackError> {
        self.send(UiRequest::SetStatus {
            key: key.to_owned(),
            text,
        });
        Ok(())
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

    fn set_widget(
        &self,
        key: &JsString,
        content: Option<&[JsString]>,
        options: Option<&ExtensionWidgetOptions>,
    ) -> Result<(), CallbackError> {
        self.send(UiRequest::SetWidget {
            key: key.to_owned(),
            content: content.map(<[JsString]>::to_vec),
            placement: options
                .and_then(|options| options.placement)
                .unwrap_or(WidgetPlacement::AboveEditor),
        });
        Ok(())
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

    fn set_header(&self, factory: Option<ExtensionUiHeaderFactory>) -> Result<(), CallbackError> {
        if std::thread::current().id() == self.shared.owner_thread {
            return with_mode(self.id, |mode| mode.set_extension_header(factory))
                .ok_or_else(|| CallbackError::new("Interactive UI is not available"))?;
        }
        let (respond, receiver) = std::sync::mpsc::channel();
        self.sender
            .send(UiRequest::SetHeader { factory, respond })
            .map_err(|_| CallbackError::new("Interactive UI is not available"))?;
        receiver
            .recv()
            .map_err(|_| CallbackError::new("Interactive UI is not available"))?
    }

    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Custom {
            factory,
            options,
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| CallbackError::new(error.to_string()))? })
    }

    fn set_title(&self, title: &JsString) -> Result<(), CallbackError> {
        self.send(UiRequest::SetTitle(title.to_owned()));
        Ok(())
    }

    fn paste_to_editor(&self, text: &JsString) -> Result<(), CallbackError> {
        self.send(UiRequest::PasteToEditor(text.to_owned()));
        Ok(())
    }

    fn set_editor_text(&self, text: &JsString) -> Result<(), CallbackError> {
        self.send(UiRequest::SetEditorText(text.to_owned()));
        Ok(())
    }

    fn get_editor_text(&self) -> Result<JsString, CallbackError> {
        with_mode(self.id, |mode| mode.editor().borrow().get_expanded_text()).unwrap_or_else(|| Ok(JsString::default()))
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
    ) -> BoxFuture<'a, Result<Option<JsString>, CallbackError>> {
        let (respond, receiver) = tokio::sync::oneshot::channel();
        self.send(UiRequest::Editor {
            title: title.to_owned(),
            prefill: prefill.cloned(),
            respond,
        });
        Box::pin(async move { receiver.await.map_err(|error| CallbackError::new(error.to_string())) })
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

    fn theme(&self) -> Result<Arc<Theme>, CallbackError> {
        crate::modes::interactive::theme::try_current_theme()
            .map_err(|error| CallbackError::from_error(error.message(), error))
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

    fn request_component_render(&self, component_identity: usize) {
        let _ = self.sender.send(UiRequest::RequestComponentRender(component_identity));
    }

    fn request_render_force(&self) {
        let _ = self.sender.send(UiRequest::RequestRenderForce);
    }

    fn stop(&self) -> Result<(), CallbackError> {
        with_mode(self.mode_id, |mode| mode.ui.stop()).expect("extension TUI stop requires the owning live UI thread")
    }

    fn start(&self) -> Result<(), CallbackError> {
        with_mode(self.mode_id, |mode| mode.ui.start()).expect("extension TUI start requires the owning live UI thread")
    }

    fn terminal_rows(&self) -> usize {
        self.shared.rows.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn terminal_columns(&self) -> usize {
        self.shared.columns.load(std::sync::atomic::Ordering::Relaxed)
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

    fn dispose(&mut self) {
        self.inner.dispose();
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
    Prompt(
        AuthPrompt,
        tokio::sync::oneshot::Sender<Result<Option<JsString>, JsString>>,
    ),
    Notify(AuthEvent),
}

type ManualCodePromise = tokio::sync::watch::Sender<Option<Result<JsString, CallbackError>>>;

/// `AuthInteraction` backed by the mode's UI loop.
struct InteractiveAuthInteraction {
    sender: std::sync::Mutex<std::sync::mpsc::Sender<AuthUiEvent>>,
    signal: pi_js_compat::abort::AbortSignal,
    manual_code: ManualCodePromise,
}

impl InteractiveAuthInteraction {
    fn notify_ui(&self, event: AuthEvent) {
        let _ = self.send(AuthUiEvent::Notify(event));
    }

    fn send(&self, event: AuthUiEvent) -> Result<(), CallbackError> {
        self.sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .send(event)
            .map_err(|_| CallbackError::new("Login cancelled"))
    }

    async fn request_prompt(&self, prompt: AuthPrompt) -> Result<Option<JsString>, CallbackError> {
        let prompt_signal = prompt.signal.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.send(AuthUiEvent::Prompt(prompt, sender))?;
        let result = match prompt_signal {
            Some(signal) => tokio::select! {
                result = receiver => result,
                _ = signal.wait() => return Err(CallbackError::new("Login cancelled")),
            },
            None => receiver.await,
        };
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(message)) => Err(CallbackError::new(message)),
            Err(_) => Err(CallbackError::new("Login cancelled")),
        }
    }
}

impl AuthInteraction for InteractiveAuthInteraction {
    fn signal(&self) -> pi_js_compat::abort::AbortSignal {
        self.signal.clone()
    }

    fn prompt(&self, prompt: AuthPrompt) -> pi_ai::request::BoxFuture<'_, Result<JsString, CallbackError>> {
        Box::pin(async move {
            self.request_prompt(prompt)
                .await?
                .ok_or_else(|| CallbackError::new("Login cancelled"))
        })
    }

    fn notify(&self, event: AuthEvent) -> Result<(), CallbackError> {
        self.notify_ui(event);
        Ok(())
    }
}

impl pi_ai::oauth::OAuthLoginCallbacks for InteractiveAuthInteraction {
    fn on_auth(&self, info: pi_ai::oauth::OAuthAuthInfo) -> Result<(), CallbackError> {
        self.notify_ui(AuthEvent::AuthUrl {
            url: info.url,
            instructions: info.instructions,
        });
        Ok(())
    }
    fn on_device_code(&self, info: pi_ai::oauth::OAuthDeviceCodeInfo) -> Result<(), CallbackError> {
        self.notify_ui(AuthEvent::DeviceCode {
            user_code: info.user_code,
            verification_uri: info.verification_uri,
            interval_seconds: info.interval_seconds,
            expires_in_seconds: info.expires_in_seconds,
        });
        Ok(())
    }
    fn on_prompt(
        &self,
        prompt: pi_ai::oauth::OAuthPrompt,
    ) -> pi_ai::request::BoxFuture<'_, Result<JsString, CallbackError>> {
        self.prompt(AuthPrompt {
            signal: Some(self.signal.clone()),
            kind: AuthPromptKind::Text {
                message: prompt.message,
                placeholder: prompt.placeholder,
            },
        })
    }
    fn on_progress(&self, message: &JsString) -> Result<(), CallbackError> {
        self.notify_ui(AuthEvent::Progress {
            message: message.clone(),
        });
        Ok(())
    }
    fn on_manual_code_input(&self) -> Option<pi_ai::request::BoxFuture<'_, Result<JsString, CallbackError>>> {
        Some(Box::pin(async move {
            let mut receiver = self.manual_code.subscribe();
            loop {
                if let Some(result) = receiver.borrow_and_update().clone() {
                    return result;
                }
                receiver
                    .changed()
                    .await
                    .expect("manual promise sender lives with callbacks");
            }
        }))
    }
    fn on_select(
        &self,
        prompt: pi_ai::oauth::OAuthSelectPrompt,
    ) -> pi_ai::request::BoxFuture<'_, Result<Option<JsString>, CallbackError>> {
        Box::pin(async move {
            self.request_prompt(AuthPrompt {
                signal: Some(self.signal.clone()),
                kind: AuthPromptKind::Select {
                    message: prompt.message,
                    options: prompt
                        .options
                        .into_iter()
                        .map(|option| pi_ai::auth::types::AuthSelectOption {
                            id: option.id,
                            label: option.label,
                            description: None,
                        })
                        .collect(),
                },
            })
            .await
        })
    }
    fn signal(&self) -> Option<pi_js_compat::abort::AbortSignal> {
        Some(self.signal.clone())
    }
}

/// Active auth flow UI state polled by the mode loop.
struct AuthUiState {
    receiver: std::sync::mpsc::Receiver<AuthUiEvent>,
    dialog: Rc<RefCell<LoginDialogComponent>>,
    manual_code: ManualCodePromise,
    uses_callback_server: bool,
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
/// Frozen core-target shape guard, expressed without optional capability methods.
pub enum ResolvedMutationTarget {
    Core(Arc<dyn crate::core::extensions::types::SessionMutationTarget>),
    Interactive(Arc<dyn crate::core::extensions::types::ViewedSessionMutationTarget>),
}

pub type InteractiveSessionMutationTargetResolver =
    Arc<dyn Fn() -> Result<Option<ResolvedMutationTarget>, HandlerError> + Send + Sync>;

pub fn bind_interactive_mode_session_mutation_target_resolver(
    mode: &InteractiveMode,
    resolver: InteractiveSessionMutationTargetResolver,
) {
    *mode.session_mutation_target_resolver.borrow_mut() = Some(resolver);
}

#[derive(Clone)]
struct MountedHeader {
    component: ComponentRef,
    set_expanded: Option<HeaderExpansionHandler>,
}

#[derive(Clone)]
struct LiveCompactionDisplay {
    entry_id: JsString,
    result: crate::core::compaction::CompactionResult,
    timestamp: JsString,
}

pub struct InteractiveMode {
    runtime: AgentSessionRuntime,
    auto_trust_on_reload_cwd: RefCell<Option<JsString>>,
    options: InteractiveModeOptions,
    multi_agent_store: Option<Arc<MultiAgentStore>>,
    child_view: RefCell<Option<ChildAgentView>>,
    session_mutation_target_resolver: RefCell<Option<InteractiveSessionMutationTargetResolver>>,
    child_view_watch_events: RefCell<Option<std::sync::mpsc::Receiver<ChildViewWatchEvent>>>,
    child_view_watch_sender: std::sync::mpsc::Sender<ChildViewWatchEvent>,
    multi_agent_updates: RefCell<Option<tokio::sync::mpsc::UnboundedReceiver<AgentSnapshot>>>,
    multi_agent_subscription: RefCell<Option<crate::core::multi_agent_store::AgentSubscription>>,
    agent_slot_input_listener: Cell<Option<usize>>,
    interrupt_input_listener: Cell<Option<usize>>,
    ui: Rc<InteractiveTuiHandle>,
    header_container: Rc<RefCell<Container>>,
    loaded_resources_container: Rc<RefCell<Container>>,
    chat_container: Rc<RefCell<crate::modes::interactive::components::render_region_container::RenderRegionContainer>>,
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
    branch_change_subscription: RefCell<Option<crate::core::footer_data_provider::BranchChangeSubscription>>,
    keybindings: KeybindingsManager,
    version: JsString,
    is_initialized: Cell<bool>,
    actions: RefCell<std::collections::VecDeque<ModeAction>>,
    events: RefCell<Option<tokio::sync::mpsc::UnboundedSender<AgentSessionEvent>>>,
    ui_requests: RefCell<Option<std::sync::mpsc::Receiver<UiRequest>>>,
    ui_sender: std::sync::mpsc::Sender<UiRequest>,
    ui_shared: Arc<UiShared>,
    ui_id: Cell<usize>,
    unsubscribe: RefCell<Option<(AgentSession, u64)>>,
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
    built_in_header: RefCell<Option<MountedHeader>>,
    custom_header: RefCell<Option<MountedHeader>>,
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
    live_compaction_display: RefCell<Option<LiveCompactionDisplay>>,
    active_session_selector: RefCell<Option<Rc<tokio::sync::Mutex<SessionSelectorComponent>>>>,
    active_selector_dispose: RefCell<Option<Rc<dyn Fn()>>>,
    active_bash_components: RefCell<Vec<ComponentRef>>,
    pending_bash_components: RefCell<Vec<ComponentRef>>,
    provisional_user_messages: RefCell<Vec<(AgentMessage, Vec<ComponentRef>)>>,
    auth_ui: RefCell<Option<AuthUiState>>,
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

fn runtime_auth_status_callback(
    runtime: crate::core::model_registry::ModelRegistry,
) -> super::components::oauth_selector::GetAuthStatus {
    Rc::new(move |id| runtime.get_provider_auth_status(id))
}

impl InteractiveMode {
    /// Builds the interactive mode. Must run inside a tokio runtime context
    /// because [`FooterDataProvider`] captures the current runtime handle.
    pub async fn new(
        runtime: AgentSessionRuntime,
        options: InteractiveModeOptions,
        terminal: Option<Rc<RefCell<dyn Terminal>>>,
    ) -> Result<Rc<Self>, CallbackError> {
        let session = runtime.clone().session().clone();
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
        let renderer = create_interactive_tui(InteractiveTuiOptions {
            show_hardware_cursor,
            log_directory: agent_dir.clone(),
            terminal,
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
        .map_err(CallbackError::from)?;

        let (header_container, header_ref) = SharedContainer::create();
        let (loaded_resources_container, loaded_ref) = SharedContainer::create();
        use crate::modes::interactive::components::render_region_container::{
            RenderRegionContainer, SharedRenderRegionContainer,
        };
        let chat_ui = Rc::clone(&ui);
        let chat_container = Rc::new(RefCell::new(RenderRegionContainer::new(Rc::new(move |child| {
            chat_ui
                .create_render_region(child)
                .expect("interactive renderer supports chat regions")
        }))));
        let chat_ref = component(SharedRenderRegionContainer(Rc::clone(&chat_container)));
        let (transcript_tail_container, transcript_tail_ref) = SharedContainer::create();
        let (pending_messages_container, pending_ref) = SharedContainer::create();
        let (status_container, status_ref) = SharedContainer::create();
        let (widget_container_above, widget_above_ref) = SharedContainer::create();
        let (widget_container_below, widget_below_ref) = SharedContainer::create();
        let (editor_container, editor_ref) = SharedContainer::create();
        let (footer_container, footer_ref) = SharedContainer::create();

        let keybindings = KeybindingsManager::create(Some(&agent_dir)).map_err(CallbackError::from)?;
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
            .map_err(CallbackError::from)?,
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
            auto_trust_on_reload_cwd: RefCell::new(options.auto_trust_on_reload_cwd.clone()),
            options,
            multi_agent_store,
            child_view: RefCell::new(None),
            session_mutation_target_resolver: RefCell::new(None),
            child_view_watch_events: RefCell::new(Some(child_view_watch_events)),
            child_view_watch_sender,
            multi_agent_updates: RefCell::new(None),
            multi_agent_subscription: RefCell::new(None),
            agent_slot_input_listener: Cell::new(None),
            interrupt_input_listener: Cell::new(None),
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
            branch_change_subscription: RefCell::new(None),
            keybindings,
            version: crate::config::package_config().version.clone(),
            is_initialized: Cell::new(false),
            actions: RefCell::new(std::collections::VecDeque::new()),
            events: RefCell::new(None),
            ui_requests: RefCell::new(Some(ui_receiver)),
            ui_sender,
            ui_shared: shared,
            ui_id: Cell::new(0),
            unsubscribe: RefCell::new(None),
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
            built_in_header: RefCell::new(None),
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
            live_compaction_display: RefCell::new(None),
            active_session_selector: RefCell::new(None),
            active_selector_dispose: RefCell::new(None),
            active_bash_components: RefCell::new(Vec::new()),
            pending_bash_components: RefCell::new(Vec::new()),
            provisional_user_messages: RefCell::new(Vec::new()),
            auth_ui: RefCell::new(None),
        });

        *mode_slot.borrow_mut() = Some(Rc::downgrade(&mode));

        let id = register_mode(&mode);
        mode.ui_id.set(id);
        mode.install_runtime_hooks();

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

    fn create_project_trust_context(&self, cwd: &JsString) -> crate::core::extensions::types::ProjectTrustContext {
        crate::core::extensions::types::ProjectTrustContext {
            cwd: cwd.clone(),
            mode: crate::core::extensions::types::ExtensionMode::Tui,
            has_ui: true,
            ui: self.ui_context(),
        }
    }

    fn project_trust_context_factory(&self) -> crate::core::agent_session_runtime::ProjectTrustContextFactory {
        let ui = self.ui_context();
        Arc::new(move |cwd| {
            Ok(crate::core::extensions::types::ProjectTrustContext {
                cwd: cwd.clone(),
                mode: crate::core::extensions::types::ExtensionMode::Tui,
                has_ui: true,
                ui: ui.clone(),
            })
        })
    }

    fn session(&self) -> AgentSession {
        self.runtime.session()
    }

    fn install_runtime_hooks(self: &Rc<Self>) {
        let id = self.ui_id.get();
        let owner_thread = std::thread::current().id();
        let sender = self.ui_sender.clone();
        self.runtime
            .set_host_transition_executor(Some(Arc::new(move |transition| {
                sender
                    .send(UiRequest::LifecycleTransition(transition))
                    .map_err(|_| CallbackError::new("Agent session runtime is not available"))?;
                // Drain older worker submissions before scheduling this owner's submission.
                if std::thread::current().id() == owner_thread {
                    with_mode(id, |mode| mode.spawn_pending_ui_requests())
                        .ok_or_else(|| CallbackError::new("Agent session runtime is not available"))??;
                }
                Ok(())
            })));
        self.runtime.set_before_session_invalidate(Some(Arc::new(move || {
            with_mode(id, |mode| mode.reset_extension_ui())
                .ok_or_else(|| CallbackError::new("Agent session runtime is not available"))?
        })));
        self.runtime.set_before_process_restart(Some(Arc::new(move || {
            let result = with_mode(id, |mode| {
                mode.theme_controller.borrow_mut().disable_auto_sync();
                mode.ui.terminal().borrow_mut().drain_input(1000, 50);
                mode.stop()
            })
            .ok_or_else(|| CallbackError::new("Agent session runtime is not available"))
            .and_then(|result| result);
            Box::pin(async move { result })
        })));
        self.runtime.set_rebind_session(Some(Arc::new(move |_| {
            let (respond, receiver) = tokio::sync::oneshot::channel();
            with_mode(id, |mode| {
                tokio::task::spawn_local(async move {
                    let _ = respond.send(mode.rebind_current_session(true).await);
                });
            })
            .expect("runtime rebind requires its live UI owner");
            Box::pin(async move {
                receiver
                    .await
                    .map_err(|_| CallbackError::new("Agent session runtime is not available"))?
            })
        })));
    }

    fn spawn_pending_ui_requests(self: &Rc<Self>) -> Result<(), CallbackError> {
        loop {
            let request = self
                .ui_requests
                .borrow_mut()
                .as_mut()
                .ok_or_else(|| CallbackError::new("Agent session runtime is not available"))?
                .try_recv();
            let request = match request {
                Ok(request) => request,
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(CallbackError::new("Agent session runtime is not available"));
                }
            };
            match request {
                UiRequest::LifecycleTransition(transition) | UiRequest::RendererCompletion(transition) => {
                    tokio::task::spawn_local(transition);
                }
                request => {
                    let mode = Rc::clone(self);
                    tokio::task::spawn_local(async move {
                        mode.handle_ui_request(request).await;
                    });
                }
            }
        }
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
            }
            crate::core::multi_agent_store::AgentCurrentActivity::Tool {
                started_at, tool_name, ..
            } => {
                let started_at = pi_js_compat::date::date_parse_utf16(started_at.units());
                if started_at.is_nan() {
                    return "Working...".into();
                }
                self.get_tool_waiting_message(&tool_name, Some(started_at), true)
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

    /// `setWorkingMessageForActiveTools`.
    fn set_working_message_for_active_tools(&self) {
        let first_tool = self
            .executing_tool_names
            .borrow()
            .iter()
            .next()
            .map(|(tool_call_id, tool_name)| (tool_call_id.clone(), tool_name.clone()));
        let Some((tool_call_id, tool_name)) = first_tool else {
            self.set_default_working_message(self.get_thinking_working_message());
            return;
        };
        let started_at = self.executing_tool_started_at.borrow().get(&tool_call_id).copied();
        self.set_default_working_message(self.get_tool_waiting_message(&tool_name, started_at, true));
    }

    /// `updateThinkingWorkingMessage`.
    fn update_thinking_working_message(&self) {
        if !self.executing_tool_names.borrow().is_empty() {
            return;
        }
        self.set_default_working_message(self.get_thinking_working_message());
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
        f(&self.session().settings_manager())
    }

    fn cwd(&self) -> JsString {
        self.session().cwd().to_owned()
    }

    /// `selectAgentViewFromBridge`.
    pub fn select_agent_view_from_bridge(&self, agent_id: &JsString) -> bool {
        self.select_agent_view(agent_id)
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error))
    }

    /// `selectAgentView`.
    fn select_agent_view(&self, agent_id: &JsString) -> Result<bool, CallbackError> {
        let Some(store) = self.multi_agent_store.as_ref() else {
            return Ok(false);
        };
        if agent_id == "main" {
            store.clear_selected_agent_view();
            self.clear_child_agent_view();
            self.loaded_resources_container.borrow_mut().clear();
            self.show_loaded_resources(false, true)?;
            self.chat_container.borrow_mut().clear();
            self.render_initial_messages();
            self.sync_working_loader_visibility();
            self.update_selected_agent_selection_widgets();
            return Ok(true);
        }
        let Some(selected) = store.get_agent(agent_id) else {
            return Ok(false);
        };
        let previous = store.get_selected_agent_id();
        if !matches!(
            store.select_agent_view_with_status(agent_id),
            crate::core::multi_agent_store::AgentViewSelectionResult::Ok(_)
        ) {
            return Ok(false);
        }
        if !self.open_child_agent_view(&selected)? {
            self.restore_previous_agent_selection(previous.as_ref());
            return Ok(false);
        }
        self.loaded_resources_container.borrow_mut().clear();
        self.sync_working_loader_visibility();
        self.update_selected_agent_selection_widgets();
        Ok(true)
    }

    /// `selectAgentSlot`.
    fn select_agent_slot(&self, slot_index: i64) -> Result<bool, CallbackError> {
        if slot_index == 0 {
            return self.select_agent_view(&"main".into());
        }
        let Some(store) = self.multi_agent_store.as_ref() else {
            return Ok(false);
        };
        let previous_selected_agent_id = store.get_selected_agent_id();
        match store.select_active_agent_slot_target_with_status(slot_index) {
            crate::core::multi_agent_store::ActiveAgentTargetSelectionResult::NotFound { .. } => Ok(false),
            crate::core::multi_agent_store::ActiveAgentTargetSelectionResult::Inactive { agent } => {
                self.show_inactive_agent_selection_status(&agent);
                Ok(true)
            }
            crate::core::multi_agent_store::ActiveAgentTargetSelectionResult::Ok(agent) => {
                if !self.open_child_agent_view(&agent)? {
                    self.restore_previous_agent_selection(previous_selected_agent_id.as_ref());
                    return Ok(false);
                }
                self.loaded_resources_container.borrow_mut().clear();
                self.sync_working_loader_visibility();
                self.update_selected_agent_selection_widgets();
                Ok(true)
            }
        }
    }

    /// `showInactiveAgentSelectionStatus`.
    fn show_inactive_agent_selection_status(&self, agent: &AgentSnapshot) {
        self.show_status(crate::core::multi_agent_store::format_inactive_agent_selection_message(
            agent,
        ));
    }

    /// `registerAgentSlotKeyHandlers`.
    fn register_agent_slot_key_handlers(self: &Rc<Self>) {
        for (keybinding, slot_index) in AGENT_SLOT_KEYBINDINGS {
            let mode = Rc::clone(self);
            self.default_editor.borrow_mut().on_action(
                keybinding,
                Rc::new(move || {
                    mode.select_agent_slot(slot_index)
                        .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
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
                    mode.keybindings.matches(data, keybinding)
                        && mode
                            .select_agent_slot(slot_index)
                            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error))
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

    fn register_global_interrupt_input_handler(&self) {
        if let Some(id) = self.interrupt_input_listener.take() {
            self.ui.remove_input_listener(id);
        }
        let mode_id = self.ui_id.get();
        let listener = self.ui.add_input_listener(Rc::new(move |data: &JsString| {
            let consumed = with_mode(mode_id, |mode| {
                if pi_tui::keys::is_key_release(data) || !mode.keybindings.matches(data, "app.interrupt") {
                    return false;
                }
                let editor_focused = mode
                    .ui
                    .get_focused_component()
                    .is_some_and(|focused| Rc::ptr_eq(&focused, &mode.editor_handle.borrow()));
                if mode.keybindings.matches(data, "tui.select.cancel") && !editor_focused {
                    return false;
                }
                if mode.session().cancel_supervisor_review() {
                    mode.stop_working_loader();
                    mode.status_container.borrow_mut().clear();
                    mode.ui.request_render();
                    return true;
                }
                if !mode.session().is_streaming() {
                    return false;
                }
                tokio::task::spawn_local(async move {
                    if let Err(error) = mode.cancel_streaming_and_submit_queued_messages().await {
                        mode.show_error(&error.message);
                    }
                });
                true
            })
            .unwrap_or(false);
            TuiInputListenerResult {
                consume: consumed,
                data: None,
            }
        }));
        self.interrupt_input_listener.set(Some(listener));
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
    fn open_child_agent_view(&self, agent: &AgentSnapshot) -> Result<bool, CallbackError> {
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
            return Ok(true);
        };
        if !Path::new(&encode_node_utf8(path)).exists() {
            self.render_live_agent_placeholder(agent, Some(path));
            return Ok(true);
        }
        let session_dir = self.session().session_manager().get_session_dir().to_owned();
        match SessionManager::open(path, Some(&session_dir), None) {
            Ok(manager) => {
                if let Some(view) = self.child_view.borrow_mut().as_mut() {
                    view.session_manager = Some(Arc::new(std::sync::Mutex::new(manager)));
                }
            }
            Err(_) => {
                self.render_live_agent_placeholder(agent, Some(path));
                self.schedule_selected_agent_transcript_reload(false);
                return Ok(true);
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
            self.reload_selected_agent_transcript()
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
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
    fn reload_selected_agent_transcript(&self) -> Result<(), CallbackError> {
        if self.child_view.borrow().is_none() || !self.selected_child_view_matches() {
            return Ok(());
        }
        let Some(path) = self
            .child_view
            .borrow()
            .as_ref()
            .and_then(|view| view.transcript_path.clone())
        else {
            return Ok(());
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
            return Ok(());
        }
        if !is_complete_jsonl_file(&path).unwrap_or(false) {
            self.retry_selected_agent_transcript_reload();
            return Ok(());
        }
        let session_dir = self.session().session_manager().get_session_dir().to_owned();
        let Ok(manager) = SessionManager::open(&path, Some(&session_dir), None) else {
            self.retry_selected_agent_transcript_reload();
            return Ok(());
        };
        let watch_path = {
            let mut view = self.child_view.borrow_mut();
            let Some(view) = view.as_mut() else {
                return Ok(());
            };
            view.session_manager = Some(Arc::new(std::sync::Mutex::new(manager)));
            view.reload_retries = 0;
            view.watch_path.clone()
        };
        if watch_path.as_ref() != Some(&path) {
            self.watch_selected_agent_transcript(&path);
        }
        if self.render_selected_agent_view()? {
            self.ui.request_render();
        }
        Ok(())
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
    fn build_selected_agent_footer_override(&self) -> Result<Option<FooterSessionOverride>, CallbackError> {
        let Some(session_manager) = self
            .child_view
            .borrow()
            .as_ref()
            .and_then(|view| view.session_manager.clone())
        else {
            return Ok(None);
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
        let persisted_settings = manager.read_persisted_session_settings()?;
        let selected_agent = self
            .multi_agent_store
            .as_ref()
            .and_then(|store| store.get_agent(&agent_id));
        let thinking_level = persisted_settings
            .as_ref()
            .and_then(|settings| settings.thinking_level.clone())
            .or_else(|| {
                selected_agent
                    .as_ref()
                    .and_then(|agent| agent.model.as_ref())
                    .and_then(|model| model.thinking_level.clone().map(JsString::from))
            })
            .unwrap_or_else(|| session_context.thinking_level.clone());
        let selected_model = persisted_settings
            .and_then(|settings| settings.model)
            .or_else(|| {
                selected_agent
                    .as_ref()
                    .and_then(|agent| agent.model.as_ref())
                    .map(|model| crate::core::session_manager::PersistedSessionModel {
                        provider: model.provider_id.as_str().into(),
                        model_id: model.model_id.as_str().into(),
                    })
            })
            .or_else(|| {
                session_context
                    .model
                    .as_ref()
                    .map(|model| crate::core::session_manager::PersistedSessionModel {
                        provider: model.provider.clone(),
                        model_id: model.model_id.clone(),
                    })
            });
        let model =
            selected_model.and_then(|model| self.session().model_registry().find(&model.provider, &model.model_id));
        let context_window = model.as_ref().map_or(0.0, |model| model.context_window);
        let estimate = crate::core::compaction::estimate_context_tokens(&session_context.messages);
        let context_usage = (context_window > 0.0).then(|| ContextUsage {
            tokens: Some(estimate.tokens),
            context_window,
            percent: Some(estimate.tokens / context_window * 100.0),
        });
        Ok(Some(FooterSessionOverride {
            cwd: manager.get_cwd().to_owned(),
            session_manager: Some(crate::core::readonly_session_manager::create_readonly_session_manager(
                Arc::clone(&session_manager),
            )),
            model,
            thinking_level,
            context_usage,
        }))
    }

    fn refresh_selected_agent_footer_override(&self) -> Result<(), CallbackError> {
        if self.child_view.borrow().is_none() {
            return Ok(());
        }
        if let Some(footer_override) = self.build_selected_agent_footer_override()? {
            self.footer.borrow_mut().set_session_override(footer_override.clone());
            self.footer_data_provider.set_session_override(footer_override);
        }
        Ok(())
    }

    fn render_selected_agent_view(&self) -> Result<bool, CallbackError> {
        let Some(footer_override) = self.build_selected_agent_footer_override()? else {
            return Ok(false);
        };
        let manager_handle = footer_override
            .session_manager
            .as_ref()
            .expect("child footer has manager");
        let entries = manager_handle.build_context_entries();
        let source_cwd = manager_handle.get_cwd();
        self.footer.borrow_mut().set_session_override(footer_override.clone());
        self.footer_data_provider.set_session_override(footer_override);
        self.chat_container.borrow_mut().clear();
        self.render_session_entries_with_cwd(&entries, Some(&source_cwd), false);
        Ok(true)
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
            // readFileSync(logPath, "utf8")
            let bytes = std::fs::read(encode_node_utf8(log_path))?;
            return Ok(JsString::from(String::from_utf8_lossy(&bytes).as_ref()));
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
                theme.bold(theme.fg(
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
            Ok(())
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
                self.select_agent_view_from_bridge(&"main".into());
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
                self.open_child_agent_view(&current)
                    .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
            }
            self.sync_working_loader_visibility();
            self.update_selected_agent_selection_widgets();
            self.ui.request_render();
        }
    }

    async fn submit_selected_agent_steering(
        &self,
        message: &JsString,
        submitted_text: &JsString,
    ) -> Result<bool, CallbackError> {
        let Some(store) = self.multi_agent_store.as_ref() else {
            return Ok(false);
        };
        let Some(agent_id) = store.get_selected_agent_id() else {
            return Ok(false);
        };
        let Some(agent) = store.get_agent(&agent_id) else {
            return Ok(false);
        };
        if !is_steerable_agent(&agent) {
            self.show_error(JsString::from("Could not steer ") + &agent.display_name + ": agent is not steerable");
            self.editor().borrow_mut().set_text(submitted_text)?;
            return Ok(true);
        }
        let Some(steer) = self.options.steer_multi_agent.clone() else {
            self.show_error(
                JsString::from("Could not steer ") + &agent.display_name + ": lifecycle coordinator unavailable",
            );
            self.editor().borrow_mut().set_text(submitted_text)?;
            return Ok(true);
        };
        let session_id = self.session().session_manager().get_session_id().to_owned();
        match steer(agent_id, message.into(), session_id).await {
            Ok(()) => {
                let result = self.editor().borrow_mut().add_to_history(message);
                match result {
                    Ok(()) => {
                        self.clear_editor()?;
                        self.ui.request_render();
                    }
                    Err(error) => {
                        self.show_error(error.message);
                        self.editor().borrow_mut().set_text(submitted_text)?;
                    }
                }
            }
            Err(error) => {
                self.show_error(JsString::from("Could not steer ") + &agent.display_name + ": " + error);
                self.editor().borrow_mut().set_text(submitted_text)?;
            }
        }
        Ok(true)
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

    fn output_pad(&self) -> usize {
        // Frozen mode omits the component argument; the stored setting has no live effect.
        1
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
            chat: component(
                crate::modes::interactive::components::render_region_container::SharedRenderRegionContainer(Rc::clone(
                    &self.chat_container,
                )),
            ),
            transcript_tail: transcript_tail_ref,
            pending_messages: component(SharedContainer(Rc::clone(&self.pending_messages_container))),
            on_transcript_tail_layout,
            status: status_ref,
            widget_above: component(SharedContainer(Rc::clone(&self.widget_container_above))),
            editor: component(SharedContainer(Rc::clone(&self.editor_container))),
            widget_below: component(SharedContainer(Rc::clone(&self.widget_container_below))),
            footer: component(SharedContainer(Rc::clone(&self.footer_container))),
            on_status_layout,
            on_chat_layout: {
                let chat = Rc::clone(&self.chat_container);
                Rc::new(move |rect| chat.borrow_mut().place(rect))
            },
            on_widget_above_layout: None,
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
        let title = match self.session().session_name() {
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
        self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
        let text = component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Error, &js_format!("Error: {}", message))),
            1,
            0,
        ));
        self.chat_container.borrow_mut().add_child(text);
        self.ui.request_render();
    }

    fn show_warning(&self, message: impl Into<JsString>) {
        let message = message.into();
        self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
        let text = component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Warning, &js_format!("Warning: {}", message))),
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
        let coordination = crate::modes::interactive::coordination_visibility::is_coordination_message(message);
        let previous_child_count = self.chat_container.borrow().children().len();
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
                            )));
                    }
                } else {
                    self.chat_container
                        .borrow_mut()
                        .add_child(component(UserMessageComponent::new(
                            text.clone(),
                            Some(self.markdown_theme()),
                            self.output_pad(),
                        )));
                }
                if populate_history
                    && user.input_source != Some(pi_ai::types::InputSource::Extension)
                    && !is_skill_block
                {
                    self.add_rendered_message_to_editor_history(&text)
                        .unwrap_or_else(|error| throw_uncaught(ERROR, error.message));
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
                    )));
            }
            AgentMessage::ToolResult(_) => {}
            AgentMessage::Custom(object) => self.add_custom_message_to_chat(object),
        }
        if coordination && self.chat_container.borrow().children().len() > previous_child_count {
            use crate::modes::interactive::components::coordination_message::CoordinationMessageComponent;
            let mut chat = self.chat_container.borrow_mut();
            let mut retained = chat.children().to_vec();
            let components = retained.split_off(previous_child_count);
            let session = self.session();
            let hidden = Rc::new(move || session.settings_manager().get_hide_coordination_messages());
            chat.set_children(retained);
            chat.add_child(component(CoordinationMessageComponent::new(components, hidden)));
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
        Self::set_user_messages_in_transit(&components, true);
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
            Self::set_user_messages_in_transit(&components, false);
            return true;
        }
        self.remove_components_from_chat(&components);
        false
    }

    fn set_user_messages_in_transit(components: &[ComponentRef], in_transit: bool) {
        use crate::modes::interactive::components::coordination_message::CoordinationMessageComponent;
        for component in components {
            let mut component = component.borrow_mut();
            let Some(component) = component.as_any_mut() else {
                continue;
            };
            if let Some(user) = component.downcast_mut::<UserMessageComponent>() {
                user.set_in_transit(in_transit);
            } else if let Some(coordination) = component.downcast_mut::<CoordinationMessageComponent>() {
                coordination.set_in_transit(in_transit);
            }
        }
    }

    fn discard_prepared_user_message(&self, message: &AgentMessage) {
        let index = self
            .provisional_user_messages
            .borrow()
            .iter()
            .position(|(prepared, _)| prepared == message);
        if let Some(index) = index {
            let (_, components) = self.provisional_user_messages.borrow_mut().remove(index);
            self.remove_components_from_chat(&components);
        }
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
                    exit_code,
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
                        max_lines: 0 as f64,
                        max_bytes: 0 as f64,
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
                    from_id: json_string(object, &JsString::from("fromId")),
                    timestamp: object.get("timestamp").and_then(JsValue::as_f64).unwrap_or_default(),
                };
                self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
                let mut summary = BranchSummaryMessageComponent::new(message, Some(self.markdown_theme()));
                summary.set_expanded(self.tool_output_expanded.get());
                self.chat_container.borrow_mut().add_child(component(summary));
            }
            _ => {
                if object.get("display").and_then(JsValue::as_bool) == Some(true) {
                    let renderer = self.session().extension_runner().get_message_renderer(&role);
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
        let entries = self.session().session_manager().build_context_entries();
        self.render_session_entries_with_cwd(&entries, None, true);
        self.render_project_trust_warning_if_needed();
        let all_entries = self.session().session_manager().get_entries();
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
                component_value.mark_execution_started(
                    started_at,
                    ToolExecutionStartOptions {
                        show_live_elapsed: true,
                    },
                );
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
                                .filter(|error| !error.is_empty())
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
                                Some(assistant.timestamp),
                            );
                        }
                    } else {
                        rendered_pending_tools.insert((call.id.clone()).into(), component_ref);
                    }
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
                        component_value.mark_execution_started(started_at, ToolExecutionStartOptions::default());
                    }
                    component_value.update_result(
                        ToolExecutionResult {
                            content: result.content.clone(),
                            details: result.details.clone(),
                            is_error: result.is_error,
                        },
                        false,
                        Some(timing.map_or(result.timestamp, |(_, finished_at)| finished_at)),
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
        let component_slot = Rc::new(RefCell::new(None::<std::rc::Weak<RefCell<dyn Component>>>));
        let request_slot = Rc::clone(&component_slot);
        let chat = Rc::downgrade(&self.chat_container);
        let mut component_value = CustomEntryComponent::new(
            entry.clone(),
            renderer,
            crate::modes::interactive::components::custom_entry::CustomEntryComponentOptions {
                session_id: self.session().session_manager().get_session_id().to_owned(),
                request_render: Rc::new(move || {
                    let Some(child) = request_slot.borrow().as_ref().and_then(std::rc::Weak::upgrade) else {
                        return Ok(false);
                    };
                    let Some(chat) = chat.upgrade() else {
                        return Ok(false);
                    };
                    chat.borrow_mut().request_child_render(&child)
                }),
            },
        );
        component_value.set_expanded(self.tool_output_expanded.get());
        if !component_value.has_content() {
            component_value.dispose();
            return false;
        }
        let component_ref = component(component_value);
        *component_slot.borrow_mut() = Some(Rc::downgrade(&component_ref));
        self.chat_container
            .borrow_mut()
            .track_scoped_child(Rc::clone(&component_ref));
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

    fn show_loaded_resources(&self, force: bool, show_diagnostics_when_quiet: bool) -> ResourceDisplayResult<()> {
        self.loaded_resources_container.borrow_mut().clear();
        let quiet = self.settings(|settings| settings.get_quiet_startup());
        let show_listing = force || self.options.verbose || !quiet;
        let show_diagnostics = show_listing || show_diagnostics_when_quiet;
        if !show_listing && !show_diagnostics {
            return Ok(());
        }

        let session = self.session();
        let loader = session.resource_loader();
        let skills = loader.get_skills();
        let prompts = loader.get_prompts();
        let themes = loader.get_themes();
        let agents_files = loader.get_agents_files();
        let rules_files = loader.get_rules_files();
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
            let context_files: Vec<JsString> = agents_files.agents_files.iter().map(|file| file.path.clone()).collect();

            if !context_files.is_empty() || !rules_files.rules_files.is_empty() {
                self.loaded_resources_container
                    .borrow_mut()
                    .add_child(component(Spacer::new(1)));
            }
            if !context_files.is_empty() {
                let expanded_items = context_files
                    .iter()
                    .map(|path| {
                        let display = format_display_path(path)?;
                        Ok(with_theme(|theme| {
                            theme.fg(ThemeColor::Dim, &js_format!("  {}", display))
                        }))
                    })
                    .collect::<ResourceDisplayResult<Vec<_>>>()?;
                let expanded_body = JsString::join(expanded_items, "\n");
                let compact_items = context_files
                    .iter()
                    .map(|path| format_context_path(path, &cwd))
                    .collect::<ResourceDisplayResult<Vec<_>>>()?;
                self.add_loaded_section(
                    &JsString::from("Context"),
                    &format_compact_list(&compact_items, false),
                    &expanded_body,
                    ThemeColor::MdHeading,
                    expanded,
                );
            }
            if !rules_files.rules_files.is_empty() {
                let expanded_items = rules_files
                    .rules_files
                    .iter()
                    .map(|file| {
                        let display = format_display_path(&file.path)?;
                        Ok(with_theme(|theme| {
                            theme.fg(ThemeColor::Dim, &js_format!("  {}", display))
                        }))
                    })
                    .collect::<ResourceDisplayResult<Vec<_>>>()?;
                let expanded_body = JsString::join(expanded_items, "\n");
                let compact_items = rules_files
                    .rules_files
                    .iter()
                    .map(|file| format_display_path(&file.path))
                    .collect::<ResourceDisplayResult<Vec<_>>>()?;
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
                )?;
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
                        Some(name) => Ok(js_format!("/{}", name)),
                        None => format_display_path(&item.path),
                    },
                    |item, _source| match by_path.get(&item.path) {
                        Some(name) => Ok(js_format!("/{}", name)),
                        None => format_display_path(&item.path),
                    },
                )?;
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
                        format_extension_display_path(&get_short_path(&item.path, item.source_info.as_ref())?)
                    },
                )?;
                let compact_labels = get_compact_extension_labels(&listed_extensions)?;
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
                )?;
                let compact_items = custom_themes
                    .iter()
                    .map(|(path, source_info, name)| {
                        if name.is_empty() {
                            get_compact_path_label(path, source_info.as_ref())
                        } else {
                            Ok(name.clone())
                        }
                    })
                    .collect::<ResourceDisplayResult<Vec<_>>>()?;
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
            for (label, diagnostics) in [
                ("Skill conflicts", &skills.diagnostics),
                ("Prompt conflicts", &prompts.diagnostics),
            ] {
                self.append_resource_diagnostics(label, diagnostics, &source_infos)?;
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
            extension_diagnostics.extend(get_builtin_command_conflict_diagnostics(
                extension_runner.get_registered_commands(),
            ));
            extension_diagnostics.extend(extension_runner.get_shortcut_diagnostics());
            self.append_resource_diagnostics("Extension issues", &extension_diagnostics, &source_infos)?;
            self.append_resource_diagnostics("Theme conflicts", &themes.diagnostics, &source_infos)?;
        }
        self.ui.request_render();
        Ok(())
    }

    fn append_resource_diagnostics(
        &self,
        label: &str,
        diagnostics: &[ResourceDiagnostic],
        source_infos: &HashMap<JsString, SourceInfo>,
    ) -> ResourceDisplayResult<()> {
        if diagnostics.is_empty() {
            return Ok(());
        }
        let body = format_diagnostics(diagnostics, source_infos)?;
        let header = with_theme(|theme| theme.fg(ThemeColor::Warning, js_format!("[{}]", label)));
        let mut resources = self.loaded_resources_container.borrow_mut();
        resources.add_child(component(Text::new(js_format!("{}\n{}", header, body), 0, 0)));
        resources.add_child(component(Spacer::new(1)));
        Ok(())
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
        f(&mut self.session().settings_manager())
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
            slash.description = Some(command.description);
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
                slash.get_argument_completions = Some(Box::new(move |prefix: &JsString| {
                    let scoped = session.scoped_models();
                    let models: Vec<Model> = if scoped.is_empty() {
                        session
                            .model_registry()
                            .get_available()
                            .unwrap_or_else(|error| crate::utils::uncaught::throw_native_uncaught(error))
                    } else {
                        scoped.into_iter().map(|scoped| scoped.model).collect()
                    };
                    model_argument_completions(&models, prefix)
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
    fn set_custom_editor_component(&self, factory: Option<ExtensionEditorFactory>) -> Result<(), CallbackError> {
        *self.editor_component_factory.borrow_mut() = factory.clone();
        *self
            .ui_shared
            .editor_component_factory
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = factory.clone();

        let current_text = self.editor().borrow().get_text()?;
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
                    editor.set_text(&current_text)?;
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
        Ok(())
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

    fn request_extension_component_render(&self, identity: usize) -> Result<(), CallbackError> {
        let above = self.extension_widgets_above.borrow();
        let below = self.extension_widgets_below.borrow();
        let target = above.values().chain(below.values()).find_map(|widget| {
            let ExtensionWidget::Component(component) = widget else {
                return None;
            };
            let mut widget = component.borrow_mut();
            let adapter = widget.as_any_mut()?.downcast_mut::<ExtensionComponentAdapter>()?;
            (adapter.inner.render_identity() == identity).then(|| Rc::clone(component))
        });
        if let Some(target) = target {
            self.ui.request_component_render(&target)?;
        } else {
            self.ui.request_render();
        }
        Ok(())
    }

    fn remove_extension_widget(&self, key: &JsString) {
        for widgets in [&self.extension_widgets_above, &self.extension_widgets_below] {
            let component = match widgets.borrow().get(key) {
                Some(ExtensionWidget::Component(component)) => Some(Rc::clone(component)),
                _ => None,
            };
            if let Some(component) = component {
                component.borrow_mut().dispose();
            }
            widgets.borrow_mut().shift_remove(key);
        }
    }

    fn add_extension_widget(&self, key: &JsString, content: Option<ExtensionWidget>, placement: WidgetPlacement) {
        self.remove_extension_widget(key);
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
        for widgets in [&self.extension_widgets_above, &self.extension_widgets_below] {
            let components: Vec<_> = widgets
                .borrow()
                .values()
                .filter_map(|widget| match widget {
                    ExtensionWidget::Component(component) => Some(Rc::clone(component)),
                    ExtensionWidget::Lines(_) => None,
                })
                .collect();
            for component in components {
                component.borrow_mut().dispose();
            }
        }
        self.extension_widgets_above.borrow_mut().clear();
        self.extension_widgets_below.borrow_mut().clear();
        self.render_widgets();
    }

    fn get_all_queued_messages(&self, visible_only: bool) -> (Vec<JsString>, Vec<JsString>) {
        use crate::modes::interactive::coordination_visibility::is_coordination_preview;
        let hide = visible_only && self.settings(|settings| settings.get_hide_coordination_messages());
        let session = self.session();
        let (mut steering, mut follow_up) = if hide {
            let queued = session.get_queued_message_previews();
            let visible = |messages: Vec<crate::core::agent_session::QueuedMessagePreview>| {
                messages
                    .into_iter()
                    .filter(|message| !is_coordination_preview(message))
                    .map(|message| message.text)
                    .collect::<Vec<_>>()
            };
            (visible(queued.steering), visible(queued.follow_up))
        } else {
            (session.get_steering_messages(), session.get_follow_up_messages())
        };
        for message in self.compaction_queued_messages.borrow().iter() {
            if message.mode == "steer" {
                steering.push(message.text.clone());
            } else if message.mode == "followUp" {
                follow_up.push(message.text.clone());
            }
        }
        (steering, follow_up)
    }

    fn update_pending_messages_display(&self) {
        use crate::modes::interactive::components::keybinding_hints::key_display_text;
        use pi_tui::TruncatedText;
        let (steering, follow_up) = self.get_all_queued_messages(true);
        let mut container = self.pending_messages_container.borrow_mut();
        container.clear();
        if steering.is_empty() && follow_up.is_empty() {
            return;
        }
        container.add_child(component(Spacer::new(1)));
        for message in steering {
            let text = with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("Steering: {}", message)));
            container.add_child(component(TruncatedText::new(text, 1, 0)));
        }
        for message in follow_up {
            let text = with_theme(|theme| theme.fg(ThemeColor::Dim, format_follow_up_preview(&message)));
            container.add_child(component(TruncatedText::new(text, 1, 0)));
        }
        let hint = key_display_text("app.message.dequeue");
        let text = with_theme(|theme| theme.fg(ThemeColor::Dim, &js_format!("↳ {} to edit all queued messages", hint)));
        container.add_child(component(TruncatedText::new(text, 1, 0)));
    }

    /// `addRenderedMessageToEditorHistory`: the default editor skips persistence
    /// (the prompt is already in the control DB); custom editors use `addToHistory`.
    fn add_rendered_message_to_editor_history(
        &self,
        text: &JsString,
    ) -> Result<(), pi_tui::editor_component::EditorHistoryError> {
        let editor = self.editor();
        let default_editor: Rc<RefCell<dyn EditorComponent>> = self.default_editor.clone();
        if Rc::ptr_eq(&editor, &default_editor) {
            self.default_editor
                .borrow_mut()
                .add_to_history_without_persistence(text);
            return Ok(());
        }
        editor.borrow_mut().add_to_history(text)
    }

    fn add_submitted_text_to_history(
        &self,
        text: &JsString,
    ) -> Result<(), pi_tui::editor_component::EditorHistoryError> {
        if is_skill_invocation(text, &self.session().prompt_templates()) {
            return Ok(());
        }
        self.editor().borrow_mut().add_to_history(text)
    }

    fn clear_editor(&self) -> Result<(), CallbackError> {
        self.editor().borrow_mut().set_text(&JsString::from(""))?;
        self.ui.request_render();
        Ok(())
    }

    fn set_tools_expanded(&self, expanded: bool) -> Result<(), CallbackError> {
        self.tool_output_expanded.set(expanded);
        self.expand_active_header()?;
        let resources = self.loaded_resources_container.borrow().children().to_vec();
        let chat = self.chat_container.borrow().children().to_vec();
        for child in resources.iter().chain(&chat) {
            expand_component(child, expanded);
        }
        self.ui.request_render();
        Ok(())
    }

    fn toggle_tool_output_expansion(&self) -> Result<(), CallbackError> {
        self.set_tools_expanded(!self.tool_output_expanded.get())
    }

    fn detach_running_tool(&self) -> Result<bool, CallbackError> {
        // Frozen interactive-mode.ts:3795-3804 has no local callback-error catch.
        let session = self.session();
        if !session.has_detachable_tool() {
            return Ok(false);
        }
        if !session.detach_running_tool()? {
            return Ok(false);
        }
        self.show_status("Detached tool to background job");
        Ok(true)
    }

    fn toggle_thinking_block_visibility(&self) {
        let hidden = !self.hide_thinking_block.get();
        self.hide_thinking_block.set(hidden);
        self.settings_mut(|settings| settings.set_hide_thinking_block(hidden));
        self.chat_container.borrow_mut().clear();
        self.rebuild_chat_from_messages();
        let streaming = self.streaming_component.borrow().clone();
        let message = self.streaming_message.borrow().clone();
        if let (Some(streaming), Some(message)) = (streaming, message) {
            self.clear_transcript_tail();
            if let Some(component) = streaming
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<AssistantMessageComponent>())
            {
                component.set_hide_thinking_block(hidden);
                component.update_content(message);
            }
            self.transcript_tail_container.borrow_mut().add_child(streaming);
        }
        self.show_status(&js_format!(
            "Thinking blocks: {}",
            if hidden { "hidden" } else { "visible" }
        ));
    }

    fn cycle_thinking_level(&self) {
        let result = self.session().cycle_thinking_level();
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

    fn handle_ctrl_c(&self) -> Result<(), CallbackError> {
        let now = js_now_millis();
        if now - self.last_sigint_time.get() < 500.0 {
            self.actions.borrow_mut().push_back(ModeAction::Shutdown);
        } else {
            self.clear_editor()?;
            self.last_sigint_time.set(now);
        }
        Ok(())
    }

    fn handle_ctrl_d(&self) -> Result<(), CallbackError> {
        if self.editor().borrow().get_text()?.is_empty() {
            self.actions.borrow_mut().push_back(ModeAction::Shutdown);
        }
        Ok(())
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
                mode.ui
                    .start()
                    .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
                mode.ui.request_render_force();
            });

            self.ui
                .stop()
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
            let Some(pid) = rustix::process::Pid::from_raw(0) else {
                self.show_error("Could not suspend the process group");
                self.ui
                    .start()
                    .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
                return;
            };
            if let Err(error) = rustix::process::kill_process_group(pid, rustix::process::Signal::TSTP) {
                self.show_error(std::io::Error::from(error).to_string());
                self.ui
                    .start()
                    .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
            }
        }
        #[cfg(not(unix))]
        self.show_status("Suspend to background is not supported on Windows");
    }

    /// `handleClipboardImagePaste`: insert image paths only, never clipboard text.
    async fn handle_clipboard_paste(&self) {
        let environment = crate::utils::clipboard_image::ClipboardEnvironment::from_process();
        let native = crate::utils::clipboard_native::clipboard();
        let native = native.as_deref();
        if let Ok(Some(image)) = crate::utils::clipboard_image::read_clipboard_image(&environment, native).await {
            // Frozen interactive-mode.ts:3835-3854 catches clipboard UUID/write errors.
            let Ok(uuid) = pi_ai::utils::random_uuid::random_uuid() else {
                return;
            };
            let extension =
                crate::utils::clipboard_image::extension_for_image_mime_type(&image.mime_type).unwrap_or("png");
            let file_path = node_join(
                pi_js_compat::node_os::tmpdir(),
                js_format!("pi-clipboard-{}.{}", uuid, extension),
            );
            if std::fs::write(encode_node_utf8(&file_path), &image.bytes).is_ok() {
                self.clipboard_temp_files.borrow_mut().track(file_path.clone());
                self.editor().borrow_mut().insert_text_at_cursor(&file_path);
                self.ui.request_render();
            }
        }
    }

    fn handle_editor_escape(&self) -> Result<(), CallbackError> {
        if self.pending_compaction_summary_edit_id.borrow_mut().take().is_some() {
            self.editor().borrow_mut().set_text(&JsString::default())?;
            self.show_status("Compaction summary edit cancelled");
            self.ui.request_render();
            return Ok(());
        }
        if self.cancel_selected_agent_turn() {
            return Ok(());
        }
        let session = self.session();
        if session.cancel_supervisor_review() {
            self.stop_working_loader();
            self.status_container.borrow_mut().clear();
            self.ui.request_render();
            return Ok(());
        }
        if session.is_streaming() {
            if let Some(mode) = with_mode(self.ui_id.get(), |mode| mode) {
                tokio::task::spawn_local(async move {
                    if let Err(error) = mode.cancel_streaming_and_submit_queued_messages().await {
                        mode.show_error(&error.message);
                    }
                });
            }
            return Ok(());
        }
        if session.is_bash_running() {
            session.abort_bash();
            return Ok(());
        }
        if self.is_bash_mode.get() {
            self.editor().borrow_mut().set_text(&JsString::default())?;
            self.is_bash_mode.set(false);
            self.update_editor_border_color();
            return Ok(());
        }
        if self.editor().borrow().get_text()?.trim().is_empty() {
            self.handle_empty_editor_escape();
        }
        Ok(())
    }

    fn handle_empty_editor_escape(&self) {
        use crate::core::settings_manager::DoubleEscapeAction;
        let action = self.settings(|settings| settings.get_double_escape_action());
        if action == DoubleEscapeAction::None {
            return;
        }
        let now = js_now_millis();
        if now - self.last_escape_time.get() >= 500.0 {
            self.last_escape_time.set(now);
            return;
        }
        if action == DoubleEscapeAction::Tree {
            self.show_tree_selector(None, Some(TreeFilterMode::UserOnly));
        } else {
            self.show_user_message_selector();
        }
        self.last_escape_time.set(0.0);
    }

    /// `InteractiveMode.stop`: releases the session subscription and stops the TUI.
    pub fn stop(&self) -> Result<(), CallbackError> {
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
        if let Some(id) = self.interrupt_input_listener.take() {
            self.ui.remove_input_listener(id);
        }
        if let Some(id) = self.agent_slot_input_listener.take() {
            self.ui.remove_input_listener(id);
        }
        self.clear_extension_terminal_input_listeners();
        if let Some(component) = self.custom_footer.borrow_mut().take() {
            dispose_extension_component(&component);
        }
        if let Some(component) = self.default_extension_footer.borrow_mut().take() {
            dispose_extension_component(&component);
        }
        self.footer.borrow_mut().dispose();
        self.branch_change_subscription.borrow_mut().take();
        self.footer_data_provider.dispose();
        if let Some((session, id)) = self.unsubscribe.borrow_mut().take() {
            session.unsubscribe(id);
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
            self.ui.stop()?;
            self.is_initialized.set(false);
        }
        self.unregister_signal_handlers();
        Ok(())
    }

    /// `handleFatalRuntimeError`: report, stop the TUI on the transcript, and
    /// exit 1. The TypeScript signature returns `Promise<never>`.
    fn handle_fatal_runtime_error(&self, prefix: &JsString, error: impl Into<CallbackError>) -> ! {
        let error = error
            .into()
            .error_or_string_message()
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        self.show_error(&js_format!("{}: {}", prefix, error));
        crate::modes::interactive::theme::stop_theme_watcher();
        self.stop()
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
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
        let active = std::mem::take(&mut *self.active_bash_components.borrow_mut());
        let pending = std::mem::take(&mut *self.pending_bash_components.borrow_mut());
        for component in active.into_iter().chain(pending) {
            if let Some(bash) = component
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<BashExecutionComponent>())
            {
                bash.dispose();
            }
        }
    }

    fn flush_pending_bash_components(&self) {
        for component in std::mem::take(&mut *self.pending_bash_components.borrow_mut()) {
            self.pending_messages_container.borrow_mut().remove_child(&component);
            self.chat_container.borrow_mut().add_child(component);
        }
    }

    fn append_bash_output(&self, chunk: &JsString) {
        let active = self.active_bash_components.borrow().last().cloned();
        if let Some(active) = active {
            if let Some(bash) = active
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<BashExecutionComponent>())
            {
                bash.append_output(chunk);
            }
            self.ui.request_render();
        }
    }

    /// `clearPendingToolComponents`: drops components awaiting a tool result.
    fn clear_pending_tool_components(&self) {
        self.pending_tools.borrow_mut().clear();
    }

    fn close_response_complete_notification(&self) {
        let notification = self.response_complete_notification.borrow_mut().take();
        // Frozen interactive-mode.ts:3824-3831 logs close errors without changing completion.
        if let Some(notification) = notification
            && let Err(error) = notification.close()
        {
            crate::core::desktop_notification::report_callback_error(
                "Failed to close response-complete desktop notification:",
                &error,
            );
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

async fn spawn_external_editor(command: &JsString, path: &JsString) -> Result<bool, CallbackError> {
    let mut parts = command.split(' ');
    let executable = parts.next().unwrap_or_default();
    let mut arguments = parts.collect::<Vec<_>>();
    arguments.push(path.clone());
    pi_js_compat::node_child_process::validate_spawn_arguments(&executable, &arguments)
        .map_err(|error| CallbackError::from(*error))?;
    let child = tokio::process::Command::new(encode_node_utf8(&executable))
        .args(arguments.iter().map(encode_node_utf8))
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) if pi_js_compat::node_child_process::spawn_error_is_async(&error) => return Ok(false),
        Err(error) => {
            return Err(CallbackError::from(pi_js_compat::node_error::NativeNodeError::from(
                pi_js_compat::node_fs::node_spawn_error_record(&error, &executable),
            )));
        }
    };
    Ok(child.wait().await?.success())
}

fn get_builtin_command_conflict_diagnostics(
    commands: Vec<crate::core::extensions::ResolvedCommand>,
) -> Vec<ResourceDiagnostic> {
    let names: std::collections::HashSet<JsString> = crate::core::slash_commands::builtin_slash_commands()
        .iter()
        .map(|command| command.name.into())
        .collect();
    commands
        .into_iter()
        .filter(|command| names.contains(&command.name))
        .map(|command| {
            let message = if command.invocation_name == command.name {
                js_format!(
                    "Extension command '/{}' conflicts with built-in interactive command. Skipping in autocomplete.",
                    command.name
                )
            } else {
                js_format!(
                    "Extension command '/{}' conflicts with built-in interactive command. Available as '/{}'.",
                    command.name,
                    command.invocation_name
                )
            };
            ResourceDiagnostic {
                diagnostic_type: ResourceDiagnosticType::Warning,
                message,
                path: Some(command.source_info.path),
                collision: None,
            }
        })
        .collect()
}

fn read_hotkey_section(title: &str, rows: &[(&[&str], &str)]) -> JsString {
    use crate::modes::interactive::components::keybinding_hints::key_display_text;
    let mut text = js_format!("**{}**\n| Key | Action |\n|-----|--------|\n", title);
    for (actions, description) in rows {
        let keys = JsString::join(
            actions
                .iter()
                .map(|action| js_format!("`{}`", key_display_text(*action))),
            " / ",
        );
        text.push_str(js_format!("| {} | {} |\n", keys, description));
    }
    text
}

fn read_navigation_hotkeys() -> JsString {
    read_hotkey_section(
        "Navigation",
        &[
            (
                &[
                    "tui.editor.cursorUp",
                    "tui.editor.cursorDown",
                    "tui.editor.cursorLeft",
                    "tui.editor.cursorRight",
                ],
                "Move cursor / browse history",
            ),
            (
                &["tui.editor.cursorWordLeft", "tui.editor.cursorWordRight"],
                "Move by word",
            ),
            (&["tui.editor.cursorLineStart"], "Start of line"),
            (&["tui.editor.cursorLineEnd"], "End of line"),
            (&["tui.editor.jumpForward"], "Jump forward to character"),
            (&["tui.editor.jumpBackward"], "Jump backward to character"),
            (&["tui.editor.pageUp", "tui.editor.pageDown"], "Scroll by page"),
        ],
    )
}

fn read_editing_hotkeys() -> JsString {
    let newline = if cfg!(target_os = "windows") {
        "New line (Ctrl+Enter on Windows Terminal)"
    } else {
        "New line"
    };
    read_hotkey_section(
        "Editing",
        &[
            (&["tui.input.submit"], "Send message"),
            (&["tui.input.newLine"], newline),
            (&["tui.editor.deleteWordBackward"], "Delete word backwards"),
            (&["tui.editor.deleteWordForward"], "Delete word forwards"),
            (&["tui.editor.deleteToLineStart"], "Delete to start of line"),
            (&["tui.editor.deleteToLineEnd"], "Delete to end of line"),
            (&["tui.editor.yank"], "Paste the most-recently-deleted text"),
            (&["tui.editor.yankPop"], "Cycle through the deleted text after pasting"),
            (&["tui.editor.undo"], "Undo"),
        ],
    )
}

fn read_other_hotkeys() -> JsString {
    let mut text = read_hotkey_section(
        "Other",
        &[
            (&["tui.input.tab"], "Path completion / accept autocomplete"),
            (&["app.interrupt"], "Cancel autocomplete / abort streaming"),
            (&["app.clear"], "Clear editor (first) / exit (second)"),
            (&["app.exit"], "Exit (when editor is empty)"),
            (&["app.suspend"], "Suspend to background"),
            (&["app.thinking.cycle"], "Cycle thinking level"),
            (&["app.model.cycleForward", "app.model.cycleBackward"], "Cycle models"),
            (&["app.model.select"], "Open model selector"),
            (&["app.tools.expand"], "Toggle tool output expansion"),
            (&["app.thinking.toggle"], "Toggle thinking block visibility"),
            (&["app.editor.external"], "Edit message in external editor"),
            (&["app.message.followUp"], "Queue follow-up message"),
            (&["app.message.dequeue"], "Restore queued messages"),
            (&["app.clipboard.pasteImage"], "Paste image from clipboard"),
            (&["app.agent.select"], "Open agent switcher"),
        ],
    );
    text.push_str(
        "| `/` | Slash commands |\n| `!` | Run bash command |\n| `!!` | Run bash command (excluded from context) |\n",
    );
    text
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
        self.code_block_indent.clone()
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
                tokio::task::spawn_local(async move {
                    mode.shutdown_from_signal().await;
                });
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
                tokio::task::spawn_local(async move {
                    mode.restart_from_signal().await;
                });
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
    runtime: AgentSessionRuntime,
    options: InteractiveModeOptions,
    terminal: Option<Rc<RefCell<dyn Terminal>>>,
) -> Result<(), CallbackError> {
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
    let Some(last_version) = last_version.filter(|version| !version.is_empty()) else {
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

fn expand_component(child: &ComponentRef, expanded: bool) {
    let mut child = child.borrow_mut();
    let Some(view) = child.as_any_mut() else { return };
    macro_rules! expand {
        ($component:ty) => {
            if let Some(component) = view.downcast_mut::<$component>() {
                component.set_expanded(expanded);
                return;
            }
        };
    }
    expand!(ExpandableText);
    expand!(ToolExecutionComponent);
    expand!(BashExecutionComponent);
    expand!(SkillInvocationMessageComponent);
    expand!(CompactionSummaryMessageComponent);
    expand!(BranchSummaryMessageComponent);
    expand!(CustomMessageComponent);
    expand!(CustomEntryComponent);
    expand!(crate::modes::interactive::components::coordination_message::CoordinationMessageComponent);
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
            let bash = text.trim_start().starts_with('!');
            if mode.is_bash_mode.replace(bash) != bash {
                mode.actions.borrow_mut().push_back(ModeAction::RefreshEditorBorder);
            }
        }));

        let mode = Rc::clone(self);
        self.default_editor.borrow_mut().on_escape = Some(Rc::new(move || {
            mode.handle_editor_escape()
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        }));
        let mode = Rc::clone(self);
        self.default_editor.borrow_mut().on_ctrl_d = Some(Rc::new(move || {
            mode.handle_ctrl_d()
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        }));

        fn action(mode: &Rc<InteractiveMode>, handler: impl Fn(&InteractiveMode) + 'static) -> AppActionHandler {
            let mode = Rc::clone(mode);
            Rc::new(move || {
                handler(&mode);
                true
            })
        }

        self.default_editor.borrow_mut().on_action(
            "app.clear",
            action(self, |mode| {
                mode.handle_ctrl_c()
                    .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error))
            }),
        );
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
            "app.model.select",
            action(self, |mode| {
                mode.show_model_selector(None)
                    .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
            }),
        );
        self.default_editor.borrow_mut().on_action(
            "app.tools.expand",
            action(self, |mode| {
                mode.toggle_tool_output_expansion()
                    .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
            }),
        );
        self.default_editor.borrow_mut().on_action(
            "app.tool.background",
            action(self, |mode| {
                // Frozen :3767 invokes this directly from its synchronous editor action.
                mode.detach_running_tool()
                    .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
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
        self.register_global_interrupt_input_handler();

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
    pub async fn init(self: &Rc<Self>) -> Result<(), CallbackError> {
        if self.is_initialized.get() {
            return Ok(());
        }
        ACTIVE_INTERACTIVE_MODE.with(|slot| *slot.borrow_mut() = Some(Rc::downgrade(self)));

        self.register_signal_handlers();

        self.bind_callbacks();

        let http_idle_timeout_ms = self
            .settings(|settings| settings.get_http_idle_timeout_ms())
            .map_err(CallbackError::from)?;
        let fetch = crate::core::http_dispatcher::configure_http_dispatcher(http_idle_timeout_ms as f64)?;
        *self.http_fetch.borrow_mut() = Some(fetch);

        // Load changelog (only show new entries, skip for resumed sessions).
        *self.changelog_markdown.borrow_mut() = self.get_changelog_for_display().map_err(CallbackError::from)?;
        let (fd_path, _rg_path) = tokio::join!(
            crate::utils::tools_manager::ensure_tool(crate::utils::tools_manager::Tool::Fd, false),
            crate::utils::tools_manager::ensure_tool(crate::utils::tools_manager::Tool::Rg, false),
        );
        *self.fd_path.borrow_mut() = fd_path;
        self.print_model_scope()?;
        self.render_widgets();
        self.mount_interactive_tui();
        let editor_ref = Rc::clone(&self.editor_handle.borrow());
        self.ui.set_focus(Some(editor_ref));
        self.ui
            .start()
            .map_err(|error| CallbackError::from_error(error.to_string().into(), error))?;
        self.is_initialized.set(true);
        self.subscribe_to_multi_agent_store();
        self.update_terminal_current_directory();
        self.theme_controller.borrow_mut().apply_from_settings();
        self.render_widgets();
        self.build_header();
        self.rebind_current_session(false).await?;
        self.render_initial_messages();
        self.subscribe_to_ui_changes();
        self.update_available_provider_count()?;
        self.update_ui_shared();
        self.ui.request_render();
        Ok(())
    }

    fn subscribe_to_ui_changes(self: &Rc<Self>) {
        let mode = Rc::downgrade(self);
        crate::modes::interactive::theme::on_theme_change(Rc::new(move || {
            if let Some(mode) = mode.upgrade() {
                mode.ui.invalidate();
                mode.update_editor_border_color();
                mode.ui.request_render();
            }
        }));
        let sender = self.ui_sender.clone();
        *self.branch_change_subscription.borrow_mut() = Some(self.footer_data_provider.on_branch_change(move || {
            sender
                .send(UiRequest::RequestRender)
                .map_err(|_| CallbackError::new("Interactive UI is not available"))
        }));
    }

    fn print_model_scope(&self) -> Result<(), CallbackError> {
        use std::io::Write;

        let models = self.session().scoped_models();
        let quiet = self.settings(|settings| settings.get_quiet_startup());
        if models.is_empty() || (quiet && !self.options.verbose) {
            return Ok(());
        }
        let names = models.iter().map(|entry| match entry.thinking_level {
            Some(level) => js_format!("{}:{}", entry.model.id, level.as_str()),
            None => entry.model.id.as_str().into(),
        });
        let hint = self.model_cycle_hint();
        let line = with_theme(|theme| {
            theme.fg(
                ThemeColor::Dim,
                &js_format!("Model scope: {}{}", JsString::join(names, ", "), hint),
            )
        });
        std::io::stdout()
            .write_all(encode_node_utf8(&js_format!("{}\n", line)).as_bytes())
            .map_err(|error| {
                CallbackError::from(pi_js_compat::node_error::NativeNodeError::from(
                    pi_js_compat::node_fs::node_file_write_error_record(&error),
                ))
            })
    }

    fn model_cycle_hint(&self) -> JsString {
        use crate::modes::interactive::components::keybinding_hints::{KeyTextFormatOptions, format_key_text};
        let keys = self.keybindings.get_keys("app.model.cycleForward");
        if keys.is_empty() {
            return JsString::default();
        }
        let keys = format_key_text(JsString::join(keys, "/"), KeyTextFormatOptions { capitalize: true });
        with_theme(|theme| theme.fg(ThemeColor::Muted, &js_format!(" ({} to cycle)", keys)))
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
            let view = component(Text::new("", 0, 0));
            *self.built_in_header.borrow_mut() = Some(MountedHeader {
                component: view.clone(),
                set_expanded: None,
            });
            header.add_child(view);
            return;
        }
        let logo = with_theme(|theme| {
            theme.bold(theme.fg(ThemeColor::Accent, &crate::config::package_config().app_name))
                + theme.fg(ThemeColor::Dim, &js_format!(" v{}", self.version))
        });
        let separator = with_theme(|theme| theme.fg(ThemeColor::Muted, " · "));
        let compact_instructions = key_hint("app.interrupt", "interrupt")
            + &separator
            + raw_key_hint(
                js_format!("{}/{}", key_text("app.clear"), key_text("app.exit")),
                "clear/exit",
            )
            + &separator
            + raw_key_hint("/", "commands")
            + &separator
            + raw_key_hint("!", "bash")
            + &separator
            + key_hint("app.tools.expand", "more");
        let compact_onboarding = with_theme(|theme| {
            theme.fg(
                ThemeColor::Dim,
                &js_format!(
                    "Press {} to show full startup help and loaded resources.",
                    key_text("app.tools.expand")
                ),
            )
        });
        let expanded_instructions = self.expanded_startup_instructions();
        let onboarding = with_theme(|theme| {
            theme.fg(
                ThemeColor::Dim,
                "Pi can explain its own features and look up its docs. Ask it how to use or extend Pi.",
            )
        });
        let collapsed = js_format!(
            "{}\n{}\n{}\n\n{}",
            logo,
            compact_instructions,
            compact_onboarding,
            onboarding
        );
        let expanded = js_format!("{}\n{}\n\n{}", logo, expanded_instructions, onboarding);
        let view = component(ExpandableText::new(
            move || collapsed.clone(),
            move || expanded.clone(),
            self.startup_expansion_state(),
            1,
            0,
        ));
        let set_expanded = Rc::new({
            let view = view.clone();
            move |expanded| {
                let mut view = view.borrow_mut();
                let text = view
                    .as_any_mut()
                    .and_then(|view| view.downcast_mut::<ExpandableText>())
                    .expect("built-in expandable header retains its type");
                text.set_expanded(expanded);
                Ok(())
            }
        });
        *self.built_in_header.borrow_mut() = Some(MountedHeader {
            component: view.clone(),
            set_expanded: Some(set_expanded),
        });
        header.add_child(component(Spacer::new(1)));
        header.add_child(view);
        header.add_child(component(Spacer::new(1)));
    }

    fn expanded_startup_instructions(&self) -> JsString {
        JsString::join(
            [
                key_hint("app.interrupt", "to interrupt"),
                key_hint("app.clear", "to clear"),
                raw_key_hint(js_format!("{} twice", key_text("app.clear")), "to exit"),
                key_hint("app.exit", "to exit (empty)"),
                key_hint("app.suspend", "to suspend"),
                key_hint("tui.editor.deleteToLineEnd", "to delete to end"),
                key_hint("app.thinking.cycle", "to cycle thinking level"),
                raw_key_hint(
                    js_format!(
                        "{}/{}",
                        key_text("app.model.cycleForward"),
                        key_text("app.model.cycleBackward")
                    ),
                    "to cycle models",
                ),
                key_hint("app.model.select", "to select model"),
                key_hint("app.tools.expand", "to expand tools"),
                key_hint("app.tool.background", "to background tools"),
                key_hint("app.thinking.toggle", "to expand thinking"),
                key_hint("app.editor.external", "for external editor"),
                raw_key_hint("/", "for commands"),
                raw_key_hint("!", "to run bash"),
                raw_key_hint("!!", "to run bash (no context)"),
                key_hint("app.message.followUp", "to queue follow-up"),
                key_hint("app.message.dequeue", "to edit all queued messages"),
                key_hint("app.clipboard.pasteImage", "to paste image"),
                raw_key_hint("drop files", "to attach"),
            ],
            "\n",
        )
    }

    async fn bind_extensions(self: &Rc<Self>) -> Result<(), CallbackError> {
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
                Arc::new(move |path, options| {
                    let (respond, receiver) = tokio::sync::oneshot::channel();
                    let _ = sender.send(UiRequest::RuntimeSwitchSession { path, options, respond });
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
                        Ok(())
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
            .map_err(CallbackError::from)?;
        let registered = session
            .resource_loader()
            .get_themes()
            .themes
            .into_iter()
            .map(Arc::new)
            .collect();
        crate::modes::interactive::theme::set_registered_themes(registered)
            .map_err(|error| CallbackError::from_error(error.message(), error))?;
        self.setup_autocomplete_provider();
        self.setup_extension_shortcuts();
        self.show_loaded_resources(false, true)?;
        self.show_startup_notices_if_needed();
        self.footer.borrow_mut().invalidate();
        self.ui.request_render();
        Ok(())
    }

    fn setup_extension_shortcuts(&self) {
        let runner = self.session().extension_runner();
        let shortcuts = runner.get_shortcuts(&self.keybindings.get_effective_config());
        if shortcuts.is_empty() {
            return;
        }
        let mode_id = self.ui_id.get();
        self.default_editor.borrow_mut().on_extension_shortcut = Some(Rc::new(move |data| {
            let Some((_, shortcut)) = shortcuts.iter().find(|(key, _)| pi_tui::keys::matches_key(data, *key)) else {
                return false;
            };
            let handler = shortcut.handler.clone();
            let context = runner.create_context();
            tokio::task::spawn_local(async move {
                if let Err(error) = handler(context).await {
                    with_mode(mode_id, |mode| {
                        mode.show_error(&js_format!("Shortcut handler error: {}", error.message))
                    });
                }
            });
            true
        }));
    }

    fn subscribe_to_agent(self: &Rc<Self>) {
        if let Some((session, id)) = self.unsubscribe.borrow_mut().take() {
            session.unsubscribe(id);
        }
        let Some(sender) = self.events.borrow().clone() else {
            return;
        };
        let session = self.session();
        let id = session.subscribe(Arc::new(move |event: &AgentSessionEvent| {
            let _ = sender.send(event.clone());
        }));
        *self.unsubscribe.borrow_mut() = Some((session, id));
    }

    /// Runs the interactive loop until shutdown.
    pub async fn run(self: Rc<Self>) -> Result<(), CallbackError> {
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
            self.ui
                .flush_pending()
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
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
            crate::core::session_control_db::fail_incoming_message(control_db_path, message.id, &error_message)
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
                        mode.submit(text)
                            .await
                            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
                    });
                }
                ModeAction::Shutdown => {
                    self.shutdown_requested.set(true);
                }
                ModeAction::ExternalEditor => {
                    let mode = Rc::clone(self);
                    tokio::task::spawn_local(async move {
                        mode.handle_open_external_editor()
                            .await
                            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
                    });
                }
                ModeAction::FollowUp => {
                    let mode = Rc::clone(self);
                    tokio::task::spawn_local(async move {
                        mode.handle_follow_up()
                            .await
                            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
                    });
                }
                ModeAction::RefreshEditorBorder => self.update_editor_border_color(),
                ModeAction::Dequeue => self
                    .handle_dequeue()
                    .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error)),
                ModeAction::NewSession => {
                    let mode = Rc::clone(self);
                    tokio::task::spawn_local(async move {
                        mode.handle_clear_command().await;
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

    async fn submit(&self, text: JsString) -> Result<(), CallbackError> {
        let submitted_text = text.clone();
        let text = text.trim().to_owned();
        if text.is_empty() {
            return Ok(());
        }
        self.close_response_complete_notification();
        if text.starts_with('/') {
            self.add_submitted_text_to_history(&text)
                .unwrap_or_else(|error| throw_uncaught(ERROR, error.message));
        }

        if let Some(command) = text.strip_prefix('/') {
            let (name, args) = match command.find(' ') {
                Some(index) => (command.slice(0, index), command.slice(index + 1, command.len()).trim()),
                None => (command, JsString::default()),
            };
            let handled = self
                .handle_slash_command(&name, &args)
                .await
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
            if handled {
                self.clear_editor()?;
                return Ok(());
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
                if self.session().is_bash_running() {
                    self.show_warning("A bash command is already running. Press Esc to cancel it first.");
                    self.editor().borrow_mut().set_text(&text)?;
                    return Ok(());
                }
                self.add_submitted_text_to_history(&text)
                    .unwrap_or_else(|error| throw_uncaught(ERROR, error.message));
                self.handle_bash_command(command.to_owned(), exclude_from_context).await;
                self.is_bash_mode.set(false);
                self.update_editor_border_color();
                return Ok(());
            }
        }

        if self.submit_pending_compaction_summary_edit(&text)? {
            return Ok(());
        }
        if self.submit_selected_agent_steering(&text, &submitted_text).await? {
            return Ok(());
        }

        let session = self.session();
        if session.is_compacting() {
            if self.is_extension_command(&text) {
                self.add_submitted_text_to_history(&text)
                    .unwrap_or_else(|error| throw_uncaught(ERROR, error.message));
                self.editor().borrow_mut().set_text(&JsString::default())?;
                session
                    .prompt(&text, PromptOptions::default())
                    .await
                    .map_err(CallbackError::from)?;
            } else {
                self.queue_compaction_message(text, "steer")?;
            }
            return Ok(());
        }

        self.add_submitted_text_to_history(&text)
            .unwrap_or_else(|error| throw_uncaught(ERROR, error.message));
        self.clear_editor()?;
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
            return Ok(());
        }

        self.flush_pending_bash_components();
        if let Some(callback) = self.on_input_callback.borrow().clone() {
            callback(text);
            return Ok(());
        }
        self.submit_main_loop_input(&text).await;
        Ok(())
    }

    async fn submit_main_loop_input(&self, text: &JsString) {
        let direct_command = text
            .strip_prefix('/')
            .and_then(|command| command.split(' ').next())
            .is_some_and(|command| self.session().extension_runner().get_prompt_command(&command).is_some());
        let submitted = (!direct_command).then(|| {
            AgentMessage::User(pi_ai::types::UserMessage {
                role: Default::default(),
                content: pi_ai::types::UserContent::Blocks(vec![pi_ai::types::TextOrImageContent::Text(
                    pi_ai::types::TextContent::new(text.clone()),
                )]),
                input_source: None,
                provider_native: None,
                timestamp: js_now_millis(),
            })
        });
        if let Some(message) = &submitted {
            self.render_prepared_user_message(message);
        }
        let pending = Arc::new(std::sync::Mutex::new(submitted));
        let mode_id = self.ui_id.get();
        let prepared: crate::core::agent_session::UserMessagePreparedCallback = Arc::new(move |message| {
            if let Some(mode) = with_mode(mode_id, |mode| mode) {
                let previous = pending
                    .lock()
                    .unwrap_or_else(|_| throw_uncaught(ERROR, "Provisional prompt lock poisoned"))
                    .take();
                if let Some(previous) = previous {
                    mode.discard_prepared_user_message(&previous);
                }
                mode.render_prepared_user_message(message);
            }
        });
        match self
            .session()
            .prompt(
                text,
                PromptOptions {
                    streaming_behavior: Some(StreamingBehavior::Steer),
                    on_user_message_prepared: Some(prepared),
                    ..PromptOptions::default()
                },
            )
            .await
        {
            Ok(()) => self.clipboard_temp_files.borrow_mut().cleanup_referenced_in(text),
            Err(error) => self.show_error(error.message()),
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
        self.session().extension_runner().get_command(name).is_some()
    }

    async fn handle_slash_command(&self, name: &JsString, args: &JsString) -> Result<bool, CallbackError> {
        let accepts_arguments = ["model", "export", "import", "cd", "name", "compact", "login"]
            .iter()
            .any(|command| name == *command);
        if !args.is_empty() && !accepts_arguments {
            return Ok(false);
        }
        match name {
            value if value == "hotkeys" => self.handle_hotkeys_command(),
            value if value == "settings" => {
                self.show_settings_selector();
            }
            value if value == "scoped-models" => {
                self.show_models_selector().await?;
            }
            value if value == "model" => {
                let search = (!args.is_empty()).then(|| args.to_owned());
                self.handle_model_command(search).await?;
            }
            value if value == "export" => {
                self.handle_export_command(js_format!("/export {}", args)).await;
            }
            value if value == "import" => {
                self.handle_import_command(js_format!("/import {}", args)).await?;
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
            value if value == "name" => self.handle_name_command(args)?,
            value if value == "unname" => self.handle_unname_command()?,
            value if value == "session" => self.handle_session_command(),
            value if value == "tools" => self.handle_tools_command(),
            value if value == "extensions" => self.handle_extensions_command(),
            value if value == "changelog" => self.handle_changelog_command()?,
            value if value == "fork" => self.show_user_message_selector(),
            value if value == "clone" => self.handle_clone_command().await?,
            value if value == "tree" => self.show_tree_selector(None, None),
            value if value == "trust" => self.show_trust_selector(),
            value if value == "login" => {
                if args.is_empty() {
                    self.show_oauth_selector(false).await;
                } else {
                    self.handle_login_command(args).await;
                }
            }
            value if value == "logout" => self.show_oauth_selector(true).await,
            value if value == "new" => self.handle_clear_command().await,
            value if value == "continue" => {
                if self.session().is_streaming() {
                    self.cancel_streaming_and_submit_queued_messages().await?;
                } else {
                    self.clear_editor()?;
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
            _ => {
                if self.session().extension_runner().get_command(name).is_some() {
                    let text = if args.is_empty() {
                        js_format!("/{}", name)
                    } else {
                        js_format!("/{} {}", name, args)
                    };
                    self.clear_editor()?;
                    if let Err(error) = self.session().prompt(&text, PromptOptions::default()).await {
                        self.show_error(error.message());
                    }
                    return Ok(true);
                }
                let templates = self.session().prompt_templates();
                if let Some(template) = templates.into_iter().find(|template| &template.name == name) {
                    let expanded = crate::core::prompt_templates::expand_prompt_template(
                        &js_format!("/{} {}", name, args),
                        &[template],
                    );
                    self.clear_editor()?;
                    if let Err(error) = self.session().prompt(&expanded, PromptOptions::default()).await {
                        self.show_error(error.message());
                    }
                    return Ok(true);
                }
                return Ok(false);
            }
        }
        Ok(true)
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
        let entries = self.session().session_manager().build_context_entries();
        self.render_session_entries(&entries);
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
            image_width_cells: self.settings(|settings| settings.get_image_width_cells()),
            auto_resize_images: self.settings(|settings| settings.get_image_auto_resize()),
            block_images: self.settings(|settings| settings.get_block_images()),
            enable_skill_commands: self.settings(|settings| settings.get_enable_skill_commands()),
            steering_mode: self.steering_mode(),
            follow_up_mode: self.follow_up_mode(),
            transport: self.settings(|settings| settings.get_transport()),
            http_idle_timeout_ms: http_idle_timeout_ms as f64,
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
            hide_tool_output: self.settings(|settings| settings.get_hide_tool_output()),
            hide_coordination_messages: self.settings(|settings| settings.get_hide_coordination_messages()),
            collapse_changelog: self.settings(|settings| settings.get_collapse_changelog()),
            enable_install_telemetry: self.settings(|settings| settings.get_enable_install_telemetry()),
            double_escape_action: self.settings(|settings| settings.get_double_escape_action()),
            tree_filter_mode: self.settings(|settings| settings.get_tree_filter_mode()),
            show_hardware_cursor: self.settings(|settings| settings.get_show_hardware_cursor()),
            editor_padding_x: self.settings(|settings| settings.get_editor_padding_x()),
            output_pad: self.settings(|settings| settings.get_output_pad()),
            autocomplete_max_visible: self.settings(|settings| settings.get_autocomplete_max_visible()),
            quiet_startup: self.settings(|settings| settings.get_quiet_startup()),
            default_project_trust: self.settings(|settings| settings.get_default_project_trust()),
            clear_on_shrink: self.settings(|settings| settings.get_clear_on_shrink()),
            show_terminal_progress: self.settings(|settings| settings.get_show_terminal_progress()),
            warnings: self.settings(|settings| settings.get_warnings()),
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
            on_image_width_cells_change: callback!(|mode: Rc<InteractiveMode>, width: f64| {
                mode.settings_mut(|settings| settings.set_image_width_cells(width));
                for child in mode.chat_container.borrow().children().to_vec() {
                    if let Some(component) = child
                        .borrow_mut()
                        .as_any_mut()
                        .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                    {
                        component.set_image_width_cells(width);
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
            on_http_idle_timeout_ms_change: callback!(|mode: Rc<InteractiveMode>, timeout_ms: f64| {
                if let Err(error) = mode.settings_mut(|settings| settings.set_http_idle_timeout_ms(timeout_ms)) {
                    mode.show_error(error.message());
                    return;
                }
                let _ = crate::core::http_dispatcher::configure_http_dispatcher(timeout_ms);
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
            on_mermaid_rendering_mode_change: callback!(|mode: Rc<InteractiveMode>, value| {
                mode.settings_mut(|settings| settings.set_mermaid_rendering_mode(value));
            }),
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
            on_hide_tool_output_change: callback!(|mode: Rc<InteractiveMode>, hidden: bool| {
                mode.settings_mut(|settings| settings.set_hide_tool_output(hidden));
                for child in mode.chat_container.borrow().children().to_vec() {
                    if let Some(component) = child
                        .borrow_mut()
                        .as_any_mut()
                        .and_then(|any| any.downcast_mut::<ToolExecutionComponent>())
                    {
                        component.set_hide_output(hidden);
                    }
                }
            }),
            on_hide_coordination_messages_change: callback!(|mode: Rc<InteractiveMode>, hidden: bool| {
                mode.settings_mut(|settings| settings.set_hide_coordination_messages(hidden));
                mode.update_pending_messages_display();
                mode.ui.request_render();
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
            on_editor_padding_x_change: callback!(|mode: Rc<InteractiveMode>, padding: f64| {
                mode.settings_mut(|settings| settings.set_editor_padding_x(padding));
                mode.default_editor
                    .borrow_mut()
                    .set_padding_x(padding.max(0.0) as usize);
                let current = mode.editor();
                let default: Rc<RefCell<dyn EditorComponent>> = mode.default_editor.clone();
                if !Rc::ptr_eq(&current, &default) {
                    current.borrow_mut().set_padding_x(padding.max(0.0) as usize);
                }
            }),
            on_output_pad_change: callback!(|mode: Rc<InteractiveMode>, padding: u8| {
                mode.settings_mut(|settings| settings.set_output_pad(padding));
            }),
            on_autocomplete_max_visible_change: callback!(|mode: Rc<InteractiveMode>, max_visible: f64| {
                mode.settings_mut(|settings| settings.set_autocomplete_max_visible(max_visible));
                mode.default_editor
                    .borrow_mut()
                    .set_autocomplete_max_visible(max_visible.max(0.0) as usize);
                let current = mode.editor();
                let default: Rc<RefCell<dyn EditorComponent>> = mode.default_editor.clone();
                if !Rc::ptr_eq(&current, &default) {
                    current
                        .borrow_mut()
                        .set_autocomplete_max_visible(max_visible.max(0.0) as usize);
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
                |mode: Rc<InteractiveMode>, warnings: crate::core::settings_manager::Settings| {
                    mode.settings_mut(|settings| settings.set_warnings(&warnings));
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

    fn update_available_provider_count(&self) -> Result<(), CallbackError> {
        let models = self.get_model_candidates()?;
        let unique_providers: HashSet<&str> = models.iter().map(|model| model.provider.as_str()).collect();
        self.footer_data_provider
            .set_available_provider_count(unique_providers.len() as f64);
        Ok(())
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

    async fn show_models_selector(&self) -> Result<(), CallbackError> {
        let session = self.session();
        let runtime = session.model_registry().clone();
        // Frozen interactive-mode.ts:6154–6156 propagates refresh before discovery.
        runtime.refresh()?;
        let available_models = runtime.get_available()?;
        if available_models.is_empty() {
            self.show_status("No models available");
            return Ok(());
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
                    mode.update_available_provider_count()
                        .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
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
        Ok(())
    }

    fn resolve_viewed_session_target(
        &self,
    ) -> Result<Arc<dyn crate::core::extensions::types::ViewedSessionMutationTarget>, CallbackError> {
        let Some(agent_id) = self.child_view.borrow().as_ref().map(|view| view.agent_id.clone()) else {
            return Ok(Arc::new(
                crate::bundled_extensions::agents_core::session_factories::AgentSessionMutationTarget::new(
                    self.session(),
                ),
            ));
        };
        let resolver = self.session_mutation_target_resolver.borrow().clone();
        let target = match resolver {
            Some(resolve) => resolve().map_err(CallbackError::from)?,
            None => None,
        };
        match target {
            Some(ResolvedMutationTarget::Interactive(target)) => Ok(target),
            Some(ResolvedMutationTarget::Core(_)) => Err(CallbackError::new(js_format!(
                "Agent {} does not support interactive session mutation",
                agent_id
            ))),
            None => Err(CallbackError::new(js_format!(
                "Agent {} is not a live session mutation target",
                agent_id
            ))),
        }
    }

    fn get_model_candidates(&self) -> Result<Vec<Model>, CallbackError> {
        let session = self.resolve_viewed_session_target()?;
        let scoped = session.scoped_models();
        if !scoped.is_empty() {
            return Ok(scoped.into_iter().map(|scoped| scoped.model).collect());
        }
        // Frozen5965 is outside the getter-only catch at5966–5969.
        session.model_registry().refresh()?;
        Ok(session.model_registry().get_available().unwrap_or_default()) // Frozen registry-only catch.
    }

    fn show_model_selector(&self, initial_search: Option<&JsString>) -> Result<(), CallbackError> {
        let session = self.resolve_viewed_session_target()?;
        let mode_id = self.ui_id.get();
        let on_select: Rc<dyn Fn(&Model)> = Rc::new(move |model| {
            let model = model.clone();
            with_mode(mode_id, |mode| {
                tokio::task::spawn_local(async move {
                    let result = match mode.resolve_viewed_session_target() {
                        Ok(target) => target.set_model(model.clone()).await.map_err(CallbackError::from),
                        Err(error) => Err(error),
                    };
                    mode.restore_editor();
                    match result {
                        Ok(_) => {
                            if let Err(error) = mode.finish_model_selection(&model) {
                                mode.show_error(error.message);
                            }
                        }
                        Err(error) => mode.show_error(error.message),
                    }
                });
            });
        });
        let on_cancel = Rc::new(move || {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                mode.ui.request_render();
            });
        });
        let selector = ModelSelectorComponent::new(
            self.ui.clone(),
            session.model(),
            (*session.model_registry()).clone(),
            session.scoped_models(),
            on_select,
            on_cancel,
            initial_search,
        );
        self.show_selector(selector);
        Ok(())
    }

    fn finish_model_selection(&self, model: &Model) -> Result<(), CallbackError> {
        self.refresh_selected_agent_footer_override()?;
        self.footer.borrow_mut().invalidate();
        self.update_editor_border_color();
        self.show_status(js_format!("Model: {}", model.id));
        let mode_id = self.ui_id.get();
        let warning_model = model.clone();
        tokio::task::spawn_local(async move {
            if let Some(mode) = with_mode(mode_id, |mode| mode) {
                mode.maybe_warn_about_anthropic_subscription_auth(Some(warning_model))
                    .await;
            }
        });
        self.check_daxnuts_easter_egg(model);
        Ok(())
    }

    async fn handle_model_command(&self, search_term: Option<JsString>) -> Result<(), CallbackError> {
        let Some(search) = search_term.filter(|term| !term.is_empty()) else {
            return self.show_model_selector(None);
        };
        let model = find_exact_model_reference_match(&search, &self.get_model_candidates()?);
        let Some(model) = model else {
            return self.show_model_selector(Some(&search));
        };
        let result = match self.resolve_viewed_session_target() {
            Ok(target) => target.set_model(model.clone()).await.map_err(CallbackError::from),
            Err(error) => Err(error),
        };
        match result {
            Ok(_) => {
                if let Err(error) = self.finish_model_selection(&model) {
                    self.show_error(error.message);
                }
            }
            Err(error) => self.show_error(error.message),
        }
        Ok(())
    }

    async fn cycle_model(&self, direction: CycleDirection) {
        let target = match self.resolve_viewed_session_target() {
            Ok(target) => target,
            Err(error) => {
                self.show_error(error.message);
                return;
            }
        };
        match target.cycle_model(direction).await {
            Ok(Some(result)) => {
                if let Err(error) = self.refresh_selected_agent_footer_override() {
                    self.show_error(error.message);
                    return;
                }
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
                    JsString::from(&result.model.id)
                } else {
                    result.model.name.clone()
                };
                self.show_status(&js_format!("Switched to {}{}", name, thinking));
                self.maybe_warn_about_anthropic_subscription_auth(Some(result.model))
                    .await;
            }
            Ok(None) => {
                let message = if target.scoped_models().is_empty() {
                    "Only one model available"
                } else {
                    "Only one model in scope"
                };
                self.show_status(message);
            }
            Err(error) => self.show_error(error.message),
        }
    }

    async fn handle_open_external_editor(&self) -> Result<(), CallbackError> {
        let command = self.settings(|settings| settings.get_external_editor_command());
        if command.is_empty() {
            self.show_warning("No editor configured. Set externalEditor in settings.json or $VISUAL/$EDITOR.");
            return Ok(());
        }
        let text = self.editor().borrow().get_expanded_text()?;
        let directory = pi_js_compat::node_os::tmpdir();
        let timestamp = pi_js_compat::number::js_number_to_string(js_now_millis());
        let path = node_join(directory, js_format!("pi-editor-{}.pi.md", timestamp));
        let operation = self.edit_external_editor_file(&command, &text, &path).await;
        let _ = std::fs::remove_file(encode_node_utf8(&path));
        self.ui.start()?;
        self.ui.request_render_force();
        operation
    }

    async fn edit_external_editor_file(
        &self,
        command: &JsString,
        text: &JsString,
        path: &JsString,
    ) -> Result<(), CallbackError> {
        use pi_js_compat::node_error::NativeNodeError;
        use pi_js_compat::node_fs::{
            NodeReadApi, node_file_write_error_record, node_fs_error_record, node_path_argument_error,
            node_read_error_record,
        };
        use std::io::Write as _;
        if let Some(error) = node_path_argument_error(path) {
            return Err(CallbackError::from(NativeNodeError::from(error)));
        }
        std::fs::write(encode_node_utf8(path), encode_node_utf8(text))
            .map_err(|error| CallbackError::from(NativeNodeError::from(node_fs_error_record(&error, "open", path))))?;
        self.ui.stop()?;
        let banner = js_format!(
            "Launching external editor: {}\nPi will resume when the editor exits.\n",
            command
        );
        std::io::stdout()
            .write_all(encode_node_utf8(&banner).as_bytes())
            .map_err(|error| CallbackError::from(NativeNodeError::from(node_file_write_error_record(&error))))?;
        if spawn_external_editor(command, path).await? {
            let bytes = std::fs::read(encode_node_utf8(path)).map_err(|error| {
                CallbackError::from(NativeNodeError::from(node_read_error_record(
                    &error,
                    path,
                    NodeReadApi::ReadFileSync,
                )))
            })?;
            let content = JsString::from(String::from_utf8_lossy(&bytes).as_ref());
            let content = content.strip_suffix('\n').unwrap_or(content);
            self.editor().borrow_mut().set_text(&content)?;
        }
        Ok(())
    }

    async fn handle_follow_up(&self) -> Result<(), CallbackError> {
        let text = self.editor().borrow().get_expanded_text()?.trim().to_owned();
        if text.is_empty() {
            return Ok(());
        }
        let session = self.session();
        if session.is_compacting() {
            if self.is_extension_command(&text) {
                self.add_submitted_text_to_history(&text)
                    .unwrap_or_else(|error| throw_uncaught(ERROR, error.message));
                self.editor().borrow_mut().set_text(&JsString::default())?;
                session
                    .prompt(&text, PromptOptions::default())
                    .await
                    .map_err(CallbackError::from)?;
            } else {
                self.queue_compaction_message(text, "followUp")?;
            }
            return Ok(());
        }
        if session.is_streaming() {
            self.submit_streaming_prompt(&text, StreamingBehavior::FollowUp).await?;
        } else {
            // Frozen setEditorComponent assigns this same callback to custom editors (:3045).
            let on_submit = self.default_editor.borrow_mut().editor_mut().on_submit.clone();
            if let Some(on_submit) = on_submit {
                self.editor().borrow_mut().set_text(&JsString::default())?;
                on_submit(&text);
            }
        }
        Ok(())
    }

    fn queue_compaction_message(&self, text: JsString, mode: &'static str) -> Result<(), CallbackError> {
        self.compaction_queued_messages
            .borrow_mut()
            .push(CompactionQueuedMessage {
                text: text.clone(),
                mode,
            });
        self.add_submitted_text_to_history(&text)
            .unwrap_or_else(|error| throw_uncaught(ERROR, error.message));
        self.editor().borrow_mut().set_text(&JsString::default())?;
        self.update_pending_messages_display();
        self.show_status("Queued message for after compaction");
        Ok(())
    }

    async fn submit_streaming_prompt(&self, text: &JsString, behavior: StreamingBehavior) -> Result<(), CallbackError> {
        self.add_submitted_text_to_history(text)
            .unwrap_or_else(|error| throw_uncaught(ERROR, error.message));
        self.editor().borrow_mut().set_text(&JsString::default())?;
        if let Err(error) = self
            .session()
            .prompt(
                text,
                PromptOptions {
                    streaming_behavior: Some(behavior),
                    ..PromptOptions::default()
                },
            )
            .await
        {
            self.show_error(error.message());
        }
        self.update_pending_messages_display();
        self.ui.request_render();
        Ok(())
    }

    fn handle_dequeue(&self) -> Result<(), CallbackError> {
        let restored = self.restore_queued_messages_to_editor(false)?;
        if restored == 0 {
            self.show_status("No queued messages to restore");
        } else {
            self.show_status(&js_format!(
                "Restored {} queued message{} to editor",
                restored,
                if restored > 1 { "s" } else { "" }
            ));
        }
        Ok(())
    }

    fn clear_all_queues(&self) -> (Vec<JsString>, Vec<JsString>) {
        let (mut steering, mut follow_up) = self.session().clear_queue();
        let compaction = std::mem::take(&mut *self.compaction_queued_messages.borrow_mut());
        steering.extend(
            compaction
                .iter()
                .filter(|message| message.mode == "steer")
                .map(|message| message.text.clone()),
        );
        follow_up.extend(
            compaction
                .into_iter()
                .filter(|message| message.mode == "followUp")
                .map(|message| message.text),
        );
        (steering, follow_up)
    }

    fn restore_queued_messages_to_editor(&self, abort: bool) -> Result<usize, CallbackError> {
        let (steering, follow_up) = self.clear_all_queues();
        let queued: Vec<_> = steering.into_iter().chain(follow_up).collect();
        if queued.is_empty() {
            self.update_pending_messages_display();
            if abort {
                self.session().agent().abort();
            }
            return Ok(0);
        }
        let queued_text = JsString::join(queued.iter(), "\n\n");
        let current_text = self.editor().borrow().get_text()?;
        let text = JsString::join(
            [queued_text, current_text]
                .into_iter()
                .filter(|text| !text.trim().is_empty()),
            "\n\n",
        );
        self.editor().borrow_mut().set_text(&text)?;
        self.update_pending_messages_display();
        if abort {
            self.session().agent().abort();
        }
        Ok(queued.len())
    }

    async fn cancel_streaming_and_submit_queued_messages(&self) -> Result<(), CallbackError> {
        let (steering, follow_up) = self.clear_all_queues();
        let queued: Vec<_> = steering.into_iter().chain(follow_up).collect();
        if queued.is_empty() {
            self.update_pending_messages_display();
            return self.session().abort().await.map_err(CallbackError::from);
        }
        let queued_text = JsString::join(queued, "\n\n");
        let current = self.editor().borrow().get_text()?;
        let combined = JsString::join(
            [queued_text, current]
                .into_iter()
                .filter(|text| !text.trim().is_empty()),
            "\n\n",
        );
        self.editor().borrow_mut().set_text(&JsString::default())?;
        self.update_pending_messages_display();
        self.session()
            .interrupt(&combined, pi_ai::types::InputSource::Interactive)
            .await
            .map_err(CallbackError::from)
    }

    async fn handle_bash_command(&self, command: JsString, exclude_from_context: bool) {
        use crate::core::extensions::types::UserBashEvent;
        let session = self.session();
        let cwd = session.session_manager().get_cwd().clone();
        let interception = session
            .extension_runner()
            .emit_user_bash(&UserBashEvent {
                command: command.clone(),
                exclude_from_context,
                cwd,
            })
            .await;
        let complete = interception
            .as_ref()
            .and_then(|event| event.result.as_ref())
            .map(|value| {
                pi_js_compat::js_json::from_value::<crate::core::bash_executor::BashResult>(value)
                    .unwrap_or_else(|error| throw_uncaught(ERROR, format!("Invalid user_bash result: {error}")))
            });
        let render: Rc<dyn Fn()> = Rc::new({
            let ui = Rc::clone(&self.ui);
            move || ui.request_render()
        });
        let bash = component(BashExecutionComponent::new(
            command.clone(),
            render,
            exclude_from_context,
            BashExecutionOptions {
                show_elapsed: complete.as_ref().map(|_| false),
            },
        ));
        if session.is_streaming() {
            self.pending_messages_container.borrow_mut().add_child(Rc::clone(&bash));
            self.pending_bash_components.borrow_mut().push(Rc::clone(&bash));
        } else {
            self.chat_container.borrow_mut().add_child(Rc::clone(&bash));
        }
        self.active_bash_components.borrow_mut().push(Rc::clone(&bash));
        if let Some(result) = complete {
            self.append_bash_output(&result.output);
            Self::complete_bash_view(&bash, &result);
            session.record_bash_result(&command, &result, Some(exclude_from_context));
            self.invalidate_footer_after_bash_result(exclude_from_context);
            self.active_bash_components
                .borrow_mut()
                .retain(|active| !Rc::ptr_eq(active, &bash));
            self.ui.request_render();
            return;
        }
        self.set_prompt_activity(PromptActivitySource::Bash, true);
        self.ui.request_render();
        let sender = self.ui_sender.clone();
        let on_chunk: crate::core::bash_executor::BashChunkCallback = Arc::new(move |chunk| {
            sender
                .send(UiRequest::BashOutput(chunk.clone()))
                .map_err(|_| CallbackError::new("Interactive UI is not available"))
        });
        let result = session
            .execute_bash(
                &command,
                Some(on_chunk),
                crate::core::agent_session::ExecuteBashOptions {
                    exclude_from_context: Some(exclude_from_context),
                    operations: interception.and_then(|event| event.operations),
                },
            )
            .await;
        // Flush the final queued chunk before dropping the active output target.
        if let Some(mode) = with_mode(self.ui_id.get(), |mode| mode) {
            mode.handle_ui_requests().await;
        }
        match result {
            Ok(result) => {
                self.invalidate_footer_after_bash_result(exclude_from_context);
                Self::complete_bash_view(&bash, &result);
            }
            Err(error) => {
                if let Some(view) = bash
                    .borrow_mut()
                    .as_any_mut()
                    .and_then(|any| any.downcast_mut::<BashExecutionComponent>())
                {
                    view.set_complete(None, false, None, None);
                }
                self.show_error(&js_format!("Bash command failed: {}", error.message()));
            }
        }
        self.active_bash_components
            .borrow_mut()
            .retain(|active| !Rc::ptr_eq(active, &bash));
        self.set_prompt_activity(PromptActivitySource::Bash, false);
        self.ui.request_render();
    }

    fn invalidate_footer_after_bash_result(&self, excluded: bool) {
        if !self.session().is_streaming() && !excluded {
            self.footer.borrow_mut().invalidate();
        }
    }

    fn complete_bash_view(component: &ComponentRef, result: &crate::core::bash_executor::BashResult) {
        if let Some(view) = component
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<BashExecutionComponent>())
        {
            view.set_complete(
                result.exit_code,
                result.cancelled,
                result
                    .truncated
                    .then(|| crate::core::tools::truncate::TruncationResult {
                        content: result.output.clone(),
                        truncated: true,
                        truncated_by: None,
                        total_lines: 0,
                        total_bytes: 0,
                        output_lines: 0,
                        output_bytes: 0,
                        last_line_partial: false,
                        first_line_exceeds_limit: false,
                        max_lines: 0.0,
                        max_bytes: 0.0,
                    }),
                result.full_output_path.clone(),
            );
        }
    }

    async fn handle_compact_command(&self, custom_instructions: Option<JsString>) {
        self.stop_working_loader();
        self.status_container.borrow_mut().clear();
        let session = self.session();
        let saved = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = Arc::clone(&saved);
        let subscription = session.subscribe(Arc::new(move |event| {
            if matches!(event,AgentSessionEvent::CompactionEnd(event) if event.result.is_some()) {
                observed.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }));
        let result = session.compact(custom_instructions.as_ref()).await;
        session.unsubscribe(subscription);
        if let Err(error) = result
            && saved.load(std::sync::atomic::Ordering::SeqCst)
        {
            self.show_error(error.message());
        }
    }

    fn rebuild_chat_after_compaction(&self, result: &crate::core::compaction::CompactionResult) {
        let timestamp = pi_js_compat::date::to_iso_string(self.now_millis() as i64).into();
        self.rebuild_chat_after_compaction_at(result, timestamp);
    }

    fn rebuild_chat_after_compaction_at(
        &self,
        result: &crate::core::compaction::CompactionResult,
        timestamp: JsString,
    ) {
        let entries = self.session().session_manager().build_context_entries();
        *self.live_compaction_display.borrow_mut() =
            crate::core::session_manager::get_latest_compaction_entry(&entries).map(|entry| LiveCompactionDisplay {
                entry_id: entry.id().expect("compaction entries have IDs").clone(),
                result: result.clone(),
                timestamp: timestamp.clone(),
            });
        self.chat_container.borrow_mut().clear();
        let message = crate::core::messages::create_compaction_summary_message(
            result.summary.clone(),
            result.tokens_before,
            timestamp,
            &crate::core::messages::CompactionSummaryOptions {
                duration_ms: result.duration_ms,
                tokens_after: result.estimated_tokens_after,
                kept_from_previous_context_tokens: result.kept_from_previous_context_tokens,
                compacted_result_tokens: result.compacted_result_tokens,
                ..Default::default()
            },
        );
        self.add_message_to_chat(&message, false);
        let non_compaction_entries: Vec<_> = entries
            .into_iter()
            .filter(|entry| entry.entry_type() != "compaction")
            .collect();
        self.render_session_entries(&non_compaction_entries);
    }

    fn render_edited_compaction_summary(&self, entry_id: &JsString, summary: &JsString) {
        let live_display = self.live_compaction_display.borrow().clone();
        if let Some(mut live_display) = live_display.filter(|display| &display.entry_id == entry_id) {
            live_display.result.summary = summary.clone();
            self.rebuild_chat_after_compaction_at(&live_display.result, live_display.timestamp);
            return;
        }
        self.chat_container.borrow_mut().clear();
        self.render_initial_messages();
    }

    async fn deliver_compaction_message(
        &self,
        message: &CompactionQueuedMessage,
    ) -> Result<(), crate::core::agent_session::AgentSessionError> {
        let session = self.session();
        if self.is_extension_command(&message.text) {
            session.prompt(&message.text, PromptOptions::default()).await
        } else if message.mode == "followUp" {
            session.follow_up(&message.text, &[]).await
        } else {
            session.steer(&message.text, &[]).await
        }
    }

    fn restore_compaction_queue(&self, queued: Vec<CompactionQueuedMessage>, error: &JsString) {
        self.session().clear_queue();
        let count = queued.len();
        *self.compaction_queued_messages.borrow_mut() = queued;
        self.update_pending_messages_display();
        self.show_error(&js_format!(
            "Failed to send queued message{}: {}",
            if count > 1 { "s" } else { "" },
            error
        ));
    }

    async fn flush_compaction_queue(&self, will_retry: bool) {
        let queued = std::mem::take(&mut *self.compaction_queued_messages.borrow_mut());
        if queued.is_empty() {
            return;
        }
        self.update_pending_messages_display();
        let result: Result<(), crate::core::agent_session::AgentSessionError> = async {
            if will_retry {
                for message in &queued {
                    self.deliver_compaction_message(message).await?;
                }
                self.update_pending_messages_display();
                return Ok(());
            }
            let first = queued
                .iter()
                .position(|message| !self.is_extension_command(&message.text));
            let Some(first) = first else {
                for message in &queued {
                    self.session().prompt(&message.text, PromptOptions::default()).await?;
                }
                return Ok(());
            };
            for message in &queued[..first] {
                self.session().prompt(&message.text, PromptOptions::default()).await?;
            }
            let session = self.session();
            let prompt = queued[first].clone();
            let restore = queued.clone();
            let mode_id = self.ui_id.get();
            tokio::task::spawn_local(async move {
                let options = PromptOptions {
                    streaming_behavior: Some(if prompt.mode == "steer" {
                        StreamingBehavior::Steer
                    } else {
                        StreamingBehavior::FollowUp
                    }),
                    ..PromptOptions::default()
                };
                if let Err(error) = session.prompt(&prompt.text, options).await
                    && let Some(mode) = with_mode(mode_id, |mode| mode)
                {
                    mode.restore_compaction_queue(restore, error.message());
                }
            });
            tokio::task::yield_now().await;
            for message in &queued[first + 1..] {
                self.deliver_compaction_message(message).await?;
            }
            self.update_pending_messages_display();
            Ok(())
        }
        .await;
        if let Err(error) = result {
            self.restore_compaction_queue(queued, error.message());
        }
    }

    async fn handle_clear_command(&self) {
        let runtime = self.runtime.clone();
        match runtime
            .new_session(crate::core::agent_session_runtime::NewSessionOptions {
                parent_session: None,
                setup: None,
                with_session: None,
            })
            .await
        {
            Ok(result) if !result.cancelled => {
                self.chat_container.borrow_mut().clear();
                self.loaded_resources_container.borrow_mut().clear();
                self.render_widgets();
                self.show_loaded_resources(false, false)
                    .unwrap_or_else(|error| crate::utils::uncaught::throw_native_uncaught(error));
                self.ui.request_render();
            }
            Ok(_) => {}
            Err(error) => self.handle_fatal_runtime_error(&JsString::from("Failed to create session"), error),
        }
    }

    fn reset_extension_ui(&self) -> Result<(), CallbackError> {
        self.restore_editor();
        self.ui.hide_overlay();
        self.clear_extension_terminal_input_listeners();
        self.set_extension_footer(None);
        self.set_default_extension_footer(None);
        self.set_extension_header(None)?;
        self.clear_extension_widgets();
        self.footer_data_provider.clear_extension_statuses();
        self.footer.borrow_mut().invalidate();
        self.autocomplete_provider_wrappers.borrow_mut().clear();
        self.set_custom_editor_component(None)?;
        self.setup_autocomplete_provider();
        self.default_editor.borrow_mut().on_extension_shortcut = None;
        self.update_terminal_title();
        *self.working_message.borrow_mut() = None;
        self.working_visible.set(true);
        self.set_working_indicator(None);
        *self.hidden_thinking_label.borrow_mut() = "Thinking...".into();
        Ok(())
    }

    async fn rebind_current_session(self: &Rc<Self>, render_before_bind: bool) -> Result<(), CallbackError> {
        self.clear_child_agent_view();
        if let Some(store) = &self.multi_agent_store {
            store.clear_selected_agent_view();
        }
        if let Some((session, id)) = self.unsubscribe.borrow_mut().take() {
            session.unsubscribe(id);
        }
        self.cancel_partial_update_render();
        self.apply_runtime_settings()?;
        if render_before_bind {
            self.render_current_session_state();
            self.subscribe_to_agent();
            self.bind_extensions().await?;
        } else {
            self.bind_extensions().await?;
            self.subscribe_to_agent();
        }
        self.update_available_provider_count()?;
        self.update_editor_border_color();
        self.update_terminal_title();
        self.update_terminal_current_directory();
        Ok(())
    }

    fn apply_runtime_settings(&self) -> Result<(), CallbackError> {
        let timeout = self
            .settings(|settings| settings.get_http_idle_timeout_ms())
            .map_err(CallbackError::from)?;
        *self.http_fetch.borrow_mut() = Some(crate::core::http_dispatcher::configure_http_dispatcher(timeout as f64)?);
        self.footer.borrow_mut().set_session(Arc::new(self.session()));
        self.footer
            .borrow_mut()
            .set_auto_compact_enabled(self.session().auto_compaction_enabled());
        self.footer_data_provider.set_cwd(self.cwd())?;
        self.hide_thinking_block
            .set(self.settings(|settings| settings.get_hide_thinking_block()));
        self.ui
            .set_show_hardware_cursor(self.settings(|settings| settings.get_show_hardware_cursor()));
        self.ui
            .set_clear_on_shrink(self.settings(|settings| settings.get_clear_on_shrink()));
        self.apply_editor_runtime_settings();
        Ok(())
    }

    fn apply_editor_runtime_settings(&self) {
        let padding = self.settings(|settings| settings.get_editor_padding_x()).max(0.0) as usize;
        let visible = self
            .settings(|settings| settings.get_autocomplete_max_visible())
            .max(0.0) as usize;
        self.default_editor.borrow_mut().set_padding_x(padding);
        self.default_editor.borrow_mut().set_autocomplete_max_visible(visible);
        let editor = self.editor();
        if !Rc::ptr_eq(
            &editor,
            &(self.default_editor.clone() as Rc<RefCell<dyn EditorComponent>>),
        ) {
            editor.borrow_mut().set_padding_x(padding);
            editor.borrow_mut().set_autocomplete_max_visible(visible);
        }
    }

    fn render_current_session_state(&self) {
        self.pending_compaction_summary_edit_id.borrow_mut().take();
        self.live_compaction_display.borrow_mut().take();
        self.dispose_active_bash_components();
        self.loaded_resources_container.borrow_mut().clear();
        self.chat_container.borrow_mut().clear();
        if let Some(region) = self.transcript_tail_region.borrow().as_ref() {
            region.clear();
        }
        self.transcript_tail_container.borrow_mut().clear();
        self.pending_messages_container.borrow_mut().clear();
        self.compaction_queued_messages.borrow_mut().clear();
        self.streaming_component.borrow_mut().take();
        self.streaming_message.borrow_mut().take();
        self.clear_pending_tool_components();
        self.render_initial_messages();
    }

    async fn handle_reload_command(&self) {
        if self.session().is_streaming() {
            self.show_warning("Wait for the current response to finish before reloading.");
            return;
        }
        if self.session().is_compacting() {
            self.show_warning("Wait for compaction to finish before reloading.");
            return;
        }
        self.reset_extension_ui()
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        let reload_box = self.build_reload_box();
        let previous_editor = self.editor_handle.borrow().clone();
        self.mount_reload_editor(reload_box);
        self.ui.request_render_force();
        tokio::task::yield_now().await;

        let restored = Arc::new(AtomicBool::new(false));
        let mode_id = self.ui_id.get();
        let before_session_start = Arc::new({
            let restored = restored.clone();
            move || {
                with_mode(mode_id, |mode| mode.restore_chat_before_reload_start(&restored))
                    .expect("reload requires its live UI owner");
                Box::pin(async {}) as pi_ai::request::BoxFuture<'static, ()>
            }
        });
        let result = async {
            self.runtime.reload(Some(before_session_start)).await?;
            self.restore_chat_before_reload_start(&restored);
            let timeout = self
                .settings(|settings| settings.get_http_idle_timeout_ms())
                .map_err(CallbackError::from)?;
            *self.http_fetch.borrow_mut() =
                Some(crate::core::http_dispatcher::configure_http_dispatcher(timeout as f64)?);
            self.keybindings.clone().reload();
            self.expand_active_header()?;
            let registered = self
                .session()
                .resource_loader()
                .get_themes()
                .themes
                .into_iter()
                .map(Arc::new)
                .collect();
            crate::modes::interactive::theme::set_registered_themes(registered)
                .map_err(|error| CallbackError::from_error(error.message(), error))?;
            self.theme_controller.borrow_mut().apply_from_settings();
            self.apply_editor_runtime_settings();
            self.ui
                .set_show_hardware_cursor(self.settings(|settings| settings.get_show_hardware_cursor()));
            self.ui
                .set_clear_on_shrink(self.settings(|settings| settings.get_clear_on_shrink()));
            self.setup_autocomplete_provider();
            self.setup_extension_shortcuts();
            self.show_loaded_resources(false, true)?;
            let saved_project_trust = self.maybe_save_implicit_project_trust_after_reload()?;
            if let Some(error) = self.session().model_registry().get_error() {
                self.show_error(&js_format!("models.json error: {}", error));
            }
            self.show_status(if saved_project_trust {
                "Reloaded keybindings, extensions, skills, prompts, themes; saved project trust"
            } else {
                "Reloaded keybindings, extensions, skills, prompts, themes"
            });
            Ok::<(), CallbackError>(())
        }
        .await;
        match result {
            Ok(()) => self.mount_reload_editor(self.editor_handle.borrow().clone()),
            Err(error) => {
                self.mount_reload_editor(previous_editor);
                self.show_error(&js_format!("Reload failed: {}", error.message));
            }
        }
    }

    fn maybe_save_implicit_project_trust_after_reload(&self) -> Result<bool, CallbackError> {
        use crate::core::trust_manager::{ProjectTrustDecision, has_trust_requiring_project_resources};
        let cwd = self.cwd();
        if self.auto_trust_on_reload_cwd.borrow().as_ref() != Some(&cwd) {
            return Ok(false);
        }
        if !self.settings(|settings| settings.is_project_trusted())
            || !has_trust_requiring_project_resources(&cwd).map_err(CallbackError::from)?
        {
            return Ok(false);
        }
        let store = ProjectTrustStore::new(&self.runtime.services().agent_dir).map_err(CallbackError::from)?;
        let save = || -> Result<bool, crate::core::trust_manager::TrustError> {
            if store.get(&cwd)? != ProjectTrustDecision::Unknown {
                self.auto_trust_on_reload_cwd.borrow_mut().take();
                return Ok(false);
            }
            store.set(&cwd, ProjectTrustDecision::Trusted)?;
            self.auto_trust_on_reload_cwd.borrow_mut().take();
            Ok(true)
        };
        match save() {
            Ok(saved) => Ok(saved),
            Err(error) => {
                self.show_warning(&js_format!(
                    "Could not save project trust after reload: {}",
                    error.message()
                ));
                Ok(false)
            }
        }
    }

    fn restore_chat_before_reload_start(&self, restored: &AtomicBool) {
        if restored.load(Ordering::SeqCst) {
            return;
        }
        self.hide_thinking_block
            .set(self.settings(|settings| settings.get_hide_thinking_block()));
        self.rebuild_chat_from_messages();
        restored.store(true, Ordering::SeqCst);
    }

    fn mount_reload_editor(&self, editor: ComponentRef) {
        self.editor_container.borrow_mut().clear();
        self.editor_container.borrow_mut().add_child(editor.clone());
        self.ui.set_focus(Some(editor));
        self.ui.request_render();
    }

    fn build_reload_box(&self) -> ComponentRef {
        let border_color = Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Border, text)));
        let mut reload_box = Container::new();
        reload_box.add_child(component(pi_tui::components::dynamic_border::DynamicBorder::new(
            border_color.clone(),
        )));
        reload_box.add_child(component(Spacer::new(1)));
        reload_box.add_child(component(Text::new(
            with_theme(|theme| {
                theme.fg(
                    ThemeColor::Muted,
                    "Reloading keybindings, extensions, skills, prompts, themes...",
                )
            }),
            1,
            0,
        )));
        reload_box.add_child(component(Spacer::new(1)));
        reload_box.add_child(component(pi_tui::components::dynamic_border::DynamicBorder::new(
            border_color,
        )));
        component(reload_box)
    }

    async fn handle_export_command(&self, text: JsString) {
        let path = Self::path_command_argument(&text, &JsString::from("/export"));
        let result = if path.as_ref().is_some_and(|path| path.ends_with(".jsonl")) {
            self.session().export_to_jsonl(path.as_ref())
        } else {
            self.session().export_to_html(path.as_ref()).await
        };
        match result {
            Ok(path) => self.show_status(&js_format!("Session exported to: {}", path)),
            Err(error) => {
                let message = match CallbackError::from(error).thrown_value() {
                    AbortReason::Error(error) | AbortReason::DomException(error) => error.message.clone(),
                    AbortReason::Value(_) => "Unknown error".into(),
                };
                self.show_error(&js_format!("Failed to export session: {}", message));
            }
        }
    }

    async fn handle_import_command(&self, text: JsString) -> Result<(), CallbackError> {
        let path = Self::path_command_argument(&text, &JsString::from("/import"));
        let Some(path) = path.filter(|path| !path.is_empty()) else {
            self.show_error("Usage: /import <path.jsonl>");
            return Ok(());
        };
        let confirmed = self
            .show_extension_confirm(
                &"Import session".into(),
                &js_format!("Replace current session with {}?", path),
            )
            .await;
        if !confirmed {
            self.show_status("Import cancelled");
            return Ok(());
        }
        self.clear_import_status();
        match self.runtime.import_from_jsonl(&path, None).await {
            Ok(result) => self.show_import_result(&path, result.cancelled),
            Err(error) => self.handle_import_rejection(&path, error).await?,
        }
        Ok(())
    }

    fn clear_import_status(&self) {
        if let Some(loader) = self.loading_animation.borrow_mut().take()
            && let Some(loader) = loader
                .borrow_mut()
                .as_any_mut()
                .and_then(|view| view.downcast_mut::<Loader>())
        {
            loader.stop();
        }
        self.status_container.borrow_mut().clear();
    }

    fn show_import_result(&self, path: &JsString, cancelled: bool) {
        if cancelled {
            self.show_status("Import cancelled");
        } else {
            self.show_status(&js_format!("Session imported from: {}", path));
        }
    }

    async fn handle_import_rejection(&self, path: &JsString, error: CallbackError) -> Result<(), CallbackError> {
        let issue = error
            .original
            .as_ref()
            .and_then(|source| source.downcast_ref::<crate::core::session_cwd::MissingSessionCwdError>())
            .map(|error| error.issue.clone());
        if let Some(issue) = issue {
            self.retry_import_in_current_cwd(path, &issue).await?;
        } else if error
            .original
            .as_ref()
            .is_some_and(|source| source.is::<crate::core::agent_session_runtime::SessionImportFileNotFoundError>())
        {
            self.show_error(&js_format!("Failed to import session: {}", error.message));
        } else {
            self.handle_fatal_runtime_error(&"Failed to import session".into(), error);
        }
        Ok(())
    }

    async fn retry_import_in_current_cwd(
        &self,
        path: &JsString,
        issue: &crate::core::session_cwd::SessionCwdIssue,
    ) -> Result<(), CallbackError> {
        let confirmed = self
            .show_extension_confirm(
                &"Session cwd not found".into(),
                &crate::core::session_cwd::format_missing_session_cwd_prompt(issue),
            )
            .await;
        if !confirmed || issue.fallback_cwd.is_empty() {
            self.show_status("Import cancelled");
            return Ok(());
        }
        let result = self.runtime.import_from_jsonl(path, Some(&issue.fallback_cwd)).await?;
        self.show_import_result(path, result.cancelled);
        Ok(())
    }

    fn path_command_argument(text: &JsString, command: &JsString) -> Option<JsString> {
        let rest = text.strip_prefix(js_format!("{} ", command))?.trim_start();
        let first = *rest.units().first()?;
        if first == u16::from(b'\"') || first == u16::from(b'\'') {
            let closing = rest.units()[1..].iter().position(|unit| *unit == first)? + 1;
            return Some(rest.slice(1, closing));
        }
        let end = rest
            .units()
            .iter()
            .position(|unit| pi_js_compat::string::is_js_whitespace_unit(*unit))
            .unwrap_or(rest.len());
        Some(rest.slice(0, end))
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
        if !auth.is_ok_and(|status| status.success()) {
            self.show_error("GitHub CLI is not logged in. Run 'gh auth login' first.");
            return;
        }

        let tmp_path = node_join(pi_js_compat::node_os::tmpdir(), "session.html");
        if let Err(error) = self.session().export_to_html(Some(&tmp_path)).await {
            self.show_error(&(JsString::from("Failed to export session: ") + error.message()));
            return;
        }

        let request_render: Rc<dyn Fn()> = Rc::new({
            let ui = Rc::clone(&self.ui);
            move || ui.request_render()
        });
        let loader = Rc::new(RefCell::new(BorderedLoader::new(
            request_render,
            crate::modes::interactive::theme::current_theme(),
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
                let _ = rustix::process::kill_process(child_pid, rustix::process::Signal::TERM);
            }
            if let Some(loader) = abort_loader.upgrade() {
                loader.borrow_mut().dispose();
            }
            let _ = std::fs::remove_file(encode_node_utf8(&abort_tmp));
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                mode.show_status("Share cancelled");
            });
        })));

        let run = match run_gh_gist(gh_program, &tmp_path, &pid).await {
            Ok(run) => run,
            Err(message) => {
                if !signal.aborted() {
                    self.finish_share_loader(&loader, &tmp_path);
                    self.show_error(&js_format!("Failed to create gist: {}", message));
                }
                return;
            }
        };
        if signal.aborted() {
            return;
        }
        self.finish_share_loader(&loader, &tmp_path);

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
        let preview_url = crate::config::get_share_viewer_url(encode_node_utf8(&gist_id));
        self.show_status(&js_format!("Share URL: {}\nGist: {}", preview_url, gist_url));
    }

    fn finish_share_loader(&self, loader: &Rc<RefCell<BorderedLoader>>, tmp_path: &JsString) {
        self.restore_editor();
        loader.borrow_mut().dispose();
        let _ = std::fs::remove_file(encode_node_utf8(tmp_path));
    }

    async fn handle_copy_command(&self) {
        let Some(text) = self.session().get_last_assistant_text() else {
            self.show_error("No agent messages to copy yet.");
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
            Err(error) => self.show_error(error.message()),
        }
    }

    fn handle_name_command(&self, args: &JsString) -> Result<(), CallbackError> {
        let name = args.trim();
        let session = self.session();
        if name.is_empty() {
            if let Some(current) = session
                .session_manager()
                .get_session_name()
                .filter(|name| !name.is_empty())
            {
                self.append_command_output(with_theme(|theme| {
                    theme.fg(ThemeColor::Dim, &js_format!("Session name: {}", current))
                }));
            } else {
                self.show_warning("Usage: /name <name>");
            }
            self.ui.request_render();
            return Ok(());
        }
        session.set_session_name(&name).map_err(CallbackError::from)?;
        let stored = session.session_manager().get_session_name();
        self.warn_if_name_normalized(&name, stored.as_ref());
        self.append_command_output(with_theme(|theme| {
            theme.fg(
                ThemeColor::Dim,
                &js_format!("Session name set: {}", stored.as_ref().unwrap_or(&name)),
            )
        }));
        self.ui.request_render();
        Ok(())
    }

    fn warn_if_name_normalized(&self, requested: &JsString, stored: Option<&JsString>) {
        if stored == Some(requested) {
            return;
        }
        let requested_json = pi_js_compat::js_json::stringify_lossless(&JsValue::String(requested.clone()));
        let stored_json = stored
            .map(|name| pi_js_compat::js_json::stringify_lossless(&JsValue::String(name.clone())))
            .unwrap_or_else(|| "undefined".into());
        self.show_warning(&js_format!(
            "Session name was normalized from {} to {}",
            requested_json,
            stored_json
        ));
    }

    fn handle_unname_command(&self) -> Result<(), CallbackError> {
        self.session().clear_session_name().map_err(CallbackError::from)?;
        self.append_command_output(with_theme(|theme| theme.fg(ThemeColor::Dim, "Session name removed")));
        self.ui.request_render();
        Ok(())
    }

    fn append_command_output(&self, text: JsString) {
        self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
        self.chat_container
            .borrow_mut()
            .add_child(component(Text::new(text, 1, 0)));
    }

    fn handle_session_command(&self) {
        let stats = self.session().get_session_stats();
        let name = self.session().session_manager().get_session_name();
        let info = format_session_info(&stats, name.as_ref());
        self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
        self.chat_container
            .borrow_mut()
            .add_child(component(Text::new(info, 1, 0)));
        self.ui.request_render();
    }

    fn handle_tools_command(&self) {
        let session = self.session();
        let tools = session
            .get_all_tools()
            .into_iter()
            .map(|tool| crate::core::extensions::types::ToolInfo {
                name: tool.name,
                description: tool.description,
                parameters: tool.parameters,
                prompt_guidelines: Some(tool.prompt_guidelines),
                source_info: tool.source_info,
            })
            .collect::<Vec<_>>();
        let info = crate::cli::list_tools::format_tool_inventory(&tools, &session.get_active_tool_names());
        self.append_command_output(info);
        self.ui.request_render();
    }

    fn handle_extensions_command(&self) {
        let info = crate::cli::list_extensions::format_extension_inventory(
            &self.session().resource_loader().get_extensions().extensions,
        );
        self.append_command_output(info);
        self.ui.request_render();
    }

    fn get_changelog_for_display(&self) -> Result<Option<JsString>, crate::config::ConfigError> {
        if !self.session().messages().is_empty() {
            return Ok(None);
        }
        let last_version = self.settings(|settings| settings.get_last_changelog_version().cloned());
        let changelog_path = crate::config::get_changelog_path()?;
        let version = self.version.clone();
        let display = changelog_display_for(
            last_version.as_ref(),
            Path::new(&encode_node_utf8(&changelog_path)),
            !self.session().messages().is_empty(),
        );
        if display.update_version {
            self.settings_mut(|settings| settings.set_last_changelog_version(&version));
            self.report_install_telemetry(&version);
        }
        Ok(display.markdown)
    }

    fn report_install_telemetry(&self, version: &JsString) {
        if pi_js_compat::process_env::get("PI_OFFLINE").is_some_and(|value| !value.is_empty()) {
            return;
        }
        if !crate::core::telemetry::is_install_telemetry_enabled(&self.session().settings_manager(), None) {
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
            let signal = pi_js_compat::abort::AbortSignal::timeout(5000.0).expect("valid constant timeout");
            let request = HttpRequest {
                method: "GET".into(),
                url: encode_node_utf8(&url),
                headers: vec![("User-Agent".into(), encode_node_utf8(&user_agent))],
                body: None,
            };
            let _ = fetch.fetch(request, Some(signal)).await;
        });
    }

    fn show_startup_notices_if_needed(&self) {
        if self.startup_notices_shown.get() {
            return;
        }
        self.startup_notices_shown.set(true);

        let Some(markdown) = self
            .changelog_markdown
            .borrow()
            .clone()
            .filter(|markdown| !markdown.is_empty())
        else {
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
                with_theme(|theme| theme.bold(theme.fg(ThemeColor::Accent, "What's New"))),
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

    fn read_hotkeys_markdown(&self) -> JsString {
        use crate::modes::interactive::components::keybinding_hints::{KeyTextFormatOptions, format_key_text};
        let mut hotkeys = JsString::join(
            [read_navigation_hotkeys(), read_editing_hotkeys(), read_other_hotkeys()],
            "\n",
        );
        let shortcuts = self
            .session()
            .extension_runner()
            .get_shortcuts(&self.keybindings.get_effective_config());
        if !shortcuts.is_empty() {
            hotkeys.push_str("\n**Extensions**\n| Key | Action |\n|-----|--------|\n");
            for (key, shortcut) in shortcuts {
                let description = shortcut.description.as_ref().unwrap_or(&shortcut.extension_path);
                let key = format_key_text(key, KeyTextFormatOptions { capitalize: true });
                hotkeys.push_str(js_format!("| `{}` | {} |\n", key, description));
            }
        }
        hotkeys
    }

    fn handle_hotkeys_command(&self) {
        self.append_markdown_command("Keyboard Shortcuts", self.read_hotkeys_markdown().trim());
    }

    fn append_markdown_command(&self, title: &str, markdown: JsString) {
        let border_color = Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Border, text)));
        let mut chat = self.chat_container.borrow_mut();
        chat.add_child(component(Spacer::new(1)));
        chat.add_child(component(pi_tui::components::dynamic_border::DynamicBorder::new(
            border_color.clone(),
        )));
        chat.add_child(component(Text::new(
            with_theme(|theme| theme.bold(theme.fg(ThemeColor::Accent, title))),
            1,
            0,
        )));
        chat.add_child(component(Spacer::new(1)));
        chat.add_child(component(pi_tui::components::markdown::Markdown::with_shared_theme(
            markdown,
            1,
            1,
            self.markdown_theme(),
            None,
            pi_tui::components::markdown::MarkdownOptions::default(),
        )));
        chat.add_child(component(pi_tui::components::dynamic_border::DynamicBorder::new(
            border_color,
        )));
        drop(chat);
        self.ui.request_render();
    }

    fn handle_changelog_command(&self) -> Result<(), CallbackError> {
        let path = crate::config::get_changelog_path().map_err(CallbackError::from)?;
        let mut entries = crate::utils::changelog::parse_changelog(&encode_node_utf8(&path));
        entries.reverse();
        let markdown = if entries.is_empty() {
            "No changelog entries found.".into()
        } else {
            JsString::join(
                entries.iter().map(|entry| {
                    let version = format!("{}.{}.{}", entry.major, entry.minor, entry.patch);
                    JsString::from(crate::utils::changelog::normalize_changelog_links_for_entry(
                        &entry.content,
                        &version,
                    ))
                }),
                "\n\n",
            )
        };
        self.append_markdown_command("What's New", markdown);
        Ok(())
    }

    async fn handle_cd_command(&self, text: JsString) {
        let Some(path) = Self::path_command_argument(&text, &JsString::from("/cd")) else {
            self.show_error("Usage: /cd <path>");
            return;
        };
        let path = match resolve_path(&path, self.cwd(), &PathInputOptions::default()) {
            Ok(path) => path,
            Err(error) => {
                self.show_error(error.message());
                return;
            }
        };
        let runtime = self.runtime.clone();
        match runtime
            .relocate(&path, Some(self.create_project_trust_context(&path)))
            .await
        {
            Ok(()) => {
                self.show_status(&js_format!("Changed directory to {}", path));
            }
            Err(error) => self.show_error(&error.message),
        }
    }

    async fn handle_clone_command(&self) -> Result<(), CallbackError> {
        let leaf_id = self.session().session_manager().get_leaf_id().cloned();
        let Some(leaf_id) = leaf_id else {
            self.show_status("Nothing to clone yet");
            return Ok(());
        };
        let runtime = self.runtime.clone();
        match runtime
            .fork(&leaf_id, crate::core::agent_session_runtime::ForkPosition::At, None)
            .await
        {
            Ok(result) if !result.cancelled => {
                self.editor().borrow_mut().set_text(&JsString::from(""))?;
                self.show_status("Cloned to new session");
            }
            Ok(_) => {
                drop(runtime);
                self.ui.request_render();
            }
            Err(error) => self.show_error(&error.message),
        }
        Ok(())
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
                    mode.fork_from_entry(&entry_id)
                        .await
                        .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
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

    async fn fork_from_entry(&self, entry_id: &JsString) -> Result<(), CallbackError> {
        let runtime = self.runtime.clone();
        match runtime
            .fork(entry_id, crate::core::agent_session_runtime::ForkPosition::Before, None)
            .await
        {
            Ok(result) if !result.cancelled => {
                self.pending_compaction_summary_edit_id.borrow_mut().take();
                self.live_compaction_display.borrow_mut().take();
                self.editor()
                    .borrow_mut()
                    .set_text(&result.selected_text.unwrap_or_default())?;
                self.show_status("Forked to new session");
            }
            Ok(_) => {
                drop(runtime);
                self.ui.request_render();
            }
            Err(error) => self.show_error(&error.message),
        }
        Ok(())
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
                    mode.handle_tree_selection(entry_id, leaf, initial_filter_mode)
                        .await
                        .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
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
        let selector = TreeSelectorComponent::new(
            tree,
            real_leaf_id,
            self.ui.terminal().borrow().rows() as f64,
            on_select,
            on_cancel,
            Some(on_label_change),
            crate::modes::interactive::components::tree_selector::TreeSelectorOptions {
                initial_selected_id,
                initial_filter_mode: Some(tree_filter_mode(initial_filter_mode)),
            },
        );
        self.show_selector(component(selector));
    }

    async fn handle_tree_selection(
        &self,
        entry_id: JsString,
        real_leaf_id: Option<JsString>,
        initial_filter_mode: TreeFilterMode,
    ) -> Result<(), CallbackError> {
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
                self.start_compaction_summary_edit(&entry, entry.summary.clone())?;
            }
            return Ok(());
        }
        if real_leaf_id.as_ref() == Some(&entry_id) {
            self.restore_editor();
            self.show_status("Already at this point");
            return Ok(());
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
                    return Ok(());
                };
                wants_summary = choice != "No summary";
                if choice == "Summarize with custom prompt" {
                    match self
                        .show_extension_editor(&JsString::from("Custom summarization instructions"), None)
                        .await
                    {
                        Some(text) => custom_instructions = Some(text),
                        None => continue,
                    }
                }
                break;
            }
        }

        if self.session().is_streaming() {
            self.restore_queued_messages_to_editor(true)?;
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
                self.live_compaction_display.borrow_mut().take();
                self.chat_container.borrow_mut().clear();
                self.render_initial_messages();
                if let Some(editor_text) = &result.editor_text
                    && self.editor().borrow().get_text()?.trim().is_empty()
                {
                    self.editor()
                        .borrow_mut()
                        .set_text(&JsString::from(&(editor_text).clone()))?;
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
        Ok(())
    }

    fn start_compaction_summary_edit(&self, entry: &CompactionEntry, summary: JsString) -> Result<(), CallbackError> {
        if !self.editor().borrow().get_text()?.trim().is_empty() {
            self.show_warning("Clear the editor before editing a compaction summary");
            return Ok(());
        }
        *self.pending_compaction_summary_edit_id.borrow_mut() = Some(entry.id.clone());
        self.editor().borrow_mut().set_text(&summary)?;
        self.show_status("Editing compaction summary");
        self.ui.request_render();
        Ok(())
    }

    fn submit_pending_compaction_summary_edit(&self, summary: &JsString) -> Result<bool, CallbackError> {
        let Some(entry_id) = self.pending_compaction_summary_edit_id.borrow().clone() else {
            return Ok(false);
        };
        if let Err(error) = self.session().update_compaction_summary(&entry_id, &summary.into()) {
            self.show_error(error.message());
            return Ok(true);
        }
        *self.pending_compaction_summary_edit_id.borrow_mut() = None;
        self.add_submitted_text_to_history(summary)
            .unwrap_or_else(|error| throw_uncaught(ERROR, error.message));
        self.editor().borrow_mut().set_text(&JsString::from(""))?;
        self.render_edited_compaction_summary(&entry_id, summary);
        self.show_status("Compaction summary updated");
        self.ui.request_render();
        Ok(true)
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
            Ok(summary) => self
                .start_compaction_summary_edit(&entry, summary)
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error)),
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
        let (cwd, session_dir, current_file) = {
            let manager = session.session_manager();
            (
                manager.get_cwd().to_owned(),
                manager.get_session_dir().to_owned(),
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
                    progress.map(|callback| move |loaded: usize, total: usize| callback(loaded as f64, total as f64));
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
            let session = session.clone();
            let apply = Rc::clone(&apply);
            let session_dir = session_dir.clone();
            let control_db_path = control_db_path.clone();
            Rc::new(move |progress: Option<ProgressCallback>| {
                let mut progress_fn =
                    progress.map(|callback| move |loaded: usize, total: usize| callback(loaded as f64, total as f64));
                let result = session
                    .session_manager()
                    .uses_default_session_dir()
                    .and_then(|uses_default_dir| {
                        let dir = (!uses_default_dir).then(|| session_dir.clone());
                        crate::core::session_manager::SessionManager::list_all(
                            dir.as_ref(),
                            progress_fn
                                .as_mut()
                                .map(|callback| callback as &mut dyn FnMut(usize, usize)),
                            control_db_path.as_ref(),
                        )
                    })
                    .map_err(|error| error.message().clone());
                let applied = result.map(|sessions| apply(sessions));
                Box::pin(async move { applied })
            })
        };
        let archived_loader: SessionListLoader = {
            let session = session.clone();
            let apply = Rc::clone(&apply);
            let session_dir = session_dir.clone();
            let control_db_path = control_db_path.clone();
            Rc::new(move |_progress: Option<ProgressCallback>| {
                let result = session
                    .session_manager()
                    .uses_default_session_dir()
                    .and_then(|uses_default_dir| {
                        let dir = (!uses_default_dir).then(|| session_dir.clone());
                        crate::core::session_manager::SessionManager::list_archived(
                            dir.as_ref(),
                            control_db_path.as_ref(),
                        )
                    })
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
                    mode.handle_resume_session(&path, None).await;
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
            crate::modes::interactive::components::session_selector::SessionSelectorOptions {
                rename_session: Some(rename_session),
                show_rename_hint: Some(true),
                keybindings: Some(self.keybindings.clone()),
                control_db_path,
                archived_sessions_loader: Some(archived_loader),
                current_session_file_path: current_file.as_ref(),
            },
        )));
        *self.active_session_selector.borrow_mut() = Some(Rc::clone(&selector));
        self.show_selector(component(SharedSessionSelector(selector)));
    }

    async fn poll_active_session_selector(&self) {
        let Some(selector) = self.active_session_selector.borrow().clone() else {
            return;
        };
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

    async fn handle_resume_session(
        &self,
        session_path: &JsString,
        options: Option<crate::core::extensions::types::SwitchSessionOptions>,
    ) -> SessionTransitionResult {
        self.status_container.borrow_mut().clear();
        let with_session = options
            .and_then(|options| options.with_session)
            .map(crate::modes::extension_actions::adapt_with_session);
        let switched = {
            let runtime = self.runtime.clone();
            match runtime
                .switch_session(
                    session_path,
                    crate::core::agent_session_runtime::SwitchSessionOptions {
                        with_session: with_session.clone(),
                        project_trust_context_factory: Some(self.project_trust_context_factory()),
                        ..Default::default()
                    },
                )
                .await
            {
                Ok(result) if result.cancelled => return SessionTransitionResult { cancelled: true },
                Ok(_) => Ok(()),
                Err(error) => Err(error),
            }
        };
        match switched {
            Ok(()) => {
                self.continue_interrupted_resumed_session().await;
                self.show_status("Resumed session");
            }
            Err(error) => {
                let issue = error
                    .original
                    .as_ref()
                    .and_then(|error| error.downcast_ref::<crate::core::session_cwd::MissingSessionCwdError>())
                    .map(|error| error.issue.clone());
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
                    return SessionTransitionResult { cancelled: true };
                }
                let switched = {
                    let runtime = self.runtime.clone();
                    match runtime
                        .switch_session(
                            session_path,
                            crate::core::agent_session_runtime::SwitchSessionOptions {
                                cwd_override: Some(issue.fallback_cwd.clone()),
                                with_session,
                                project_trust_context_factory: Some(self.project_trust_context_factory()),
                            },
                        )
                        .await
                    {
                        Ok(result) if result.cancelled => return SessionTransitionResult { cancelled: true },
                        Ok(_) => Ok(()),
                        Err(error) => Err(error),
                    }
                };
                match switched {
                    Ok(()) => {
                        self.continue_interrupted_resumed_session().await;
                        self.show_status("Resumed session in current cwd");
                    }
                    Err(error) => self.handle_fatal_runtime_error(&JsString::from("Failed to resume session"), error),
                }
            }
        }
        SessionTransitionResult { cancelled: false }
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
            let (event, dialog, manual_code, uses_callback_server) = {
                let auth = self.auth_ui.borrow();
                let Some(state) = auth.as_ref() else {
                    return;
                };
                (
                    state.receiver.try_recv().ok(),
                    Rc::clone(&state.dialog),
                    state.manual_code.clone(),
                    state.uses_callback_server,
                )
            };
            let Some(event) = event else {
                return;
            };
            match event {
                AuthUiEvent::Notify(event) => {
                    self.notify_auth_dialog(&dialog, event, manual_code, uses_callback_server)
                }
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

    fn notify_auth_dialog(
        &self,
        dialog: &Rc<RefCell<LoginDialogComponent>>,
        event: AuthEvent,
        manual_code: ManualCodePromise,
        uses_callback_server: bool,
    ) {
        let dialog = dialog.borrow();
        match event {
            AuthEvent::AuthUrl { url, instructions } => {
                dialog.show_auth(&url, instructions.as_ref());
                if uses_callback_server {
                    let input = dialog.show_manual_input(&JsString::from(
                        "Paste redirect URL below, or complete login in browser:",
                    ));
                    tokio::task::spawn_local(async move {
                        let result = match input.await {
                            Ok(value) if value.is_empty() => return,
                            Ok(value) => Ok(value),
                            Err(_) => Err(CallbackError::new("Login cancelled")),
                        };
                        manual_code.send_if_modified(|settled| {
                            if settled.is_some() {
                                return false;
                            }
                            *settled = Some(result.clone());
                            true
                        });
                    });
                }
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
            AuthEvent::Progress { message } => dialog.show_progress(&message),
        }
    }

    fn show_auth_prompt(
        &self,
        dialog: &Rc<RefCell<LoginDialogComponent>>,
        prompt: AuthPrompt,
        respond: tokio::sync::oneshot::Sender<Result<Option<JsString>, JsString>>,
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
                        let _ = respond.send(Ok(id));
                    }
                    with_mode(mode_id, |mode| mode.restore_auth_dialog(&select_dialog));
                });
                let cancel_respond = Rc::clone(&respond);
                let cancel_dialog = Rc::clone(&dialog);
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    respond_once(&cancel_respond, Ok(None));
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
                    let result = receiver.await.map(Some).map_err(|error| error.0);
                    let _ = respond.send(result);
                });
            }
        }
    }

    async fn handle_login_command(&self, provider_id: &JsString) {
        let provider_info = self
            .session()
            .auth_storage()
            .get_oauth_providers()
            .into_iter()
            .find(|provider| provider_id == provider.id());
        let Some(provider_info) = provider_info else {
            self.show_status(js_format!("Unknown OAuth provider: {}", provider_id));
            return;
        };
        self.show_login_dialog(provider_info.id(), &JsString::from(provider_info.name()))
            .await;
    }

    fn show_login_auth_type_selector(&self) {
        let subscription_label = JsString::from("Use a subscription");
        let options = vec![subscription_label.clone(), JsString::from("Use an API key")];
        let mode_id = self.ui_id.get();
        let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |option| {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                let auth_type = if option == subscription_label {
                    LoginAuthType::Oauth
                } else {
                    LoginAuthType::ApiKey
                };
                mode.show_login_provider_selector(auth_type);
            });
        });
        let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                mode.ui.request_render();
            });
        });
        self.show_selector(component(ExtensionSelectorComponent::new(
            &JsString::from("Select authentication method:"),
            options,
            on_select,
            on_cancel,
            ExtensionSelectorOptions::default(),
        )));
    }

    fn show_login_provider_selector(&self, auth_type: LoginAuthType) {
        let provider_options = get_login_provider_options(self.session().model_registry(), Some(auth_type));
        if provider_options.is_empty() {
            let message = match auth_type {
                LoginAuthType::Oauth => "No subscription providers available.",
                LoginAuthType::ApiKey => "No API key providers available.",
            };
            self.show_status(message);
            return;
        }
        let mode_id = self.ui_id.get();
        let options_for_select = provider_options.clone();
        let on_select: Rc<dyn Fn(String)> = Rc::new(move |provider_id: String| {
            with_mode(mode_id, |mode| {
                mode.restore_editor();
                if let Some(provider_option) = options_for_select
                    .iter()
                    .find(|provider| provider.id == provider_id)
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
                mode.show_login_auth_type_selector();
            });
        });
        let runtime = self.session().model_registry().clone();
        self.show_selector(component(OAuthSelectorComponent::new(
            AuthSelectorMode::Login,
            runtime.auth_storage().clone(),
            provider_options,
            on_select,
            on_cancel,
            Some(runtime_auth_status_callback(runtime)),
        )));
    }

    async fn show_oauth_selector(&self, logout: bool) {
        if !logout {
            self.show_login_auth_type_selector();
            return;
        }
        let provider_options = match get_logout_provider_options(self.session().model_registry()) {
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
        let on_select: Rc<dyn Fn(String)> = Rc::new(move |provider_id: String| {
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
                    let runtime = mode.session().model_registry().clone();
                    let result = runtime
                        .auth_storage()
                        .logout(&provider_option.id)
                        .map_err(crate::core::auth_storage::StorageError::into_callback)
                        .and_then(|()| {
                            // Frozen6839–6852 catches logout/refresh/footer failures together.
                            runtime.refresh()?;
                            mode.update_available_provider_count()
                        });
                    match result {
                        Ok(()) => {
                            let message = if provider_option.auth_type == LoginAuthType::Oauth {
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
                            let message = error.message;
                            mode.show_error(&js_format!("Logout failed: {}", message));
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
            self.session().auth_storage().clone(),
            provider_options,
            on_select,
            on_cancel,
            Some(runtime_auth_status_callback(self.session().model_registry().clone())),
        )));
    }

    async fn start_provider_login(&self, provider_option: AuthSelectorProvider) {
        match provider_option.auth_type {
            LoginAuthType::Oauth => {
                self.show_login_dialog(&provider_option.id, &provider_option.name).await;
            }
            LoginAuthType::ApiKey => {
                if provider_option.id != "amazon-bedrock" {
                    self.show_api_key_login_dialog(&provider_option.id, &provider_option.name)
                        .await;
                } else {
                    self.show_ambient_auth_dialog(&provider_option);
                }
            }
        }
    }

    async fn show_login_dialog(&self, provider_id: &str, provider_name: &JsString) {
        if let Err(error) = self
            .run_provider_login(provider_id, provider_name, LoginAuthType::Oauth)
            .await
        {
            let message = error.message;
            if message != "Login cancelled" {
                self.show_error(&js_format!("Failed to login to {}: {}", provider_name, message));
            }
        }
    }

    async fn show_api_key_login_dialog(&self, provider_id: &str, provider_name: &JsString) {
        if let Err(error) = self
            .run_provider_login(provider_id, provider_name, LoginAuthType::ApiKey)
            .await
        {
            let message = error.message;
            if message != "Login cancelled" {
                self.show_error(&js_format!("Failed to save API key for {}: {}", provider_name, message));
            }
        }
    }

    async fn run_provider_login(
        &self,
        provider_id: &str,
        provider_name: &JsString,
        auth_type: LoginAuthType,
    ) -> Result<(), CallbackError> {
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
            let docs_path = node_join(crate::config::get_docs_path()?, "providers.md");
            dialog.borrow().show_info(&[
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Text,
                        "You can also use an AWS profile, IAM keys, or role-based credentials.",
                    )
                }),
                with_theme(|theme| theme.fg(ThemeColor::Muted, "See:")),
                with_theme(|theme| theme.fg(ThemeColor::Accent, &js_format!("  {}", docs_path))),
            ]);
        }
        self.mount_auth_dialog(&dialog);
        let (sender, receiver) = std::sync::mpsc::channel();
        let (manual_code, _) = tokio::sync::watch::channel(None);
        let uses_callback_server = self
            .session()
            .auth_storage()
            .get_oauth_providers()
            .iter()
            .find(|provider| provider.id() == provider_id)
            .and_then(|provider| provider.uses_callback_server())
            .unwrap_or(false);
        let signal = dialog.borrow().signal();
        *self.auth_ui.borrow_mut() = Some(AuthUiState {
            receiver,
            dialog: Rc::clone(&dialog),
            manual_code: manual_code.clone(),
            uses_callback_server,
        });
        let interaction = Arc::new(InteractiveAuthInteraction {
            sender: std::sync::Mutex::new(sender),
            signal,
            manual_code,
        });
        let runtime = self.session().model_registry().clone();
        let result = match auth_type {
            LoginAuthType::Oauth => runtime.auth_storage().login(provider_id, interaction).await,
            LoginAuthType::ApiKey => {
                let api_key = interaction
                    .prompt(AuthPrompt {
                        signal: None,
                        kind: AuthPromptKind::Text {
                            message: "Enter API key:".into(),
                            placeholder: None,
                        },
                    })
                    .await
                    .map_err(|error| crate::core::auth_storage::StorageError::new(error.message));
                match api_key {
                    Ok(api_key) if !api_key.trim().is_empty() => runtime.auth_storage().set(
                        provider_id,
                        pi_js_compat::js_json!({"type":"api_key","key": api_key.trim()}),
                    ),
                    Ok(_) => Err(crate::core::auth_storage::StorageError::new("API key cannot be empty.")),
                    Err(error) => Err(error),
                }
            }
        };
        *self.auth_ui.borrow_mut() = None;
        self.restore_editor();
        result.map_err(crate::core::auth_storage::StorageError::into_callback)?;
        self.complete_provider_authentication(provider_id, provider_name, auth_type, previous_model)
            .await?;
        Ok(())
    }

    fn schedule_anthropic_subscription_auth_warning(&self, model: Option<Model>) {
        let Some(mode) = with_mode(self.ui_id.get(), |mode| mode) else {
            return;
        };
        tokio::task::spawn_local(async move {
            mode.maybe_warn_about_anthropic_subscription_auth(model).await;
        });
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
        let runtime = self.session().model_registry().clone();
        if runtime.is_using_oauth(&model) {
            self.anthropic_subscription_warning_shown.set(true);
            self.show_warning(ANTHROPIC_SUBSCRIPTION_AUTH_WARNING);
            return;
        }
        let auth = runtime.get_api_key_for_provider("anthropic").await;
        if let Ok(Some(api_key)) = auth
            && api_key.starts_with("sk-ant-oat")
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
        let docs_path = node_join(
            crate::config::get_docs_path().unwrap_or_else(|error| error.raise_uncaught()),
            "providers.md",
        );
        dialog.borrow().show_info(&[
            with_theme(|theme| {
                theme.fg(
                    ThemeColor::Text,
                    "Amazon Bedrock uses AWS credentials instead of a single API key.",
                )
            }),
            with_theme(|theme| {
                theme.fg(
                    ThemeColor::Text,
                    "Configure an AWS profile, IAM keys, bearer token, or role-based credentials.",
                )
            }),
            with_theme(|theme| theme.fg(ThemeColor::Muted, "See:")),
            with_theme(|theme| theme.fg(ThemeColor::Accent, &js_format!("  {}", docs_path))),
        ]);
        self.mount_auth_dialog(&dialog);
    }

    async fn complete_provider_authentication(
        &self,
        provider_id: &str,
        provider_name: &JsString,
        auth_type: LoginAuthType,
        previous_model: Model,
    ) -> Result<(), CallbackError> {
        // Frozen6867 propagates into the API-key/OAuth caller catches.
        self.session().model_registry().refresh()?;
        let action_label = if auth_type == LoginAuthType::Oauth {
            js_format!("Logged in to {}", provider_name)
        } else {
            js_format!("Saved API key for {}", provider_name)
        };
        let mut selected_model: Option<Model> = None;
        let mut selection_error: Option<JsString> = None;
        if is_unknown_model(Some(&previous_model)) {
            let available_models = self.session().model_registry().get_available()?;
            let provider_models: Vec<Model> = available_models
                .into_iter()
                .filter(|model| model.provider == provider_id)
                .collect();
            let default_model_id = crate::core::model_resolver::default_model_for_provider(provider_id);
            match default_model_id {
                None => {
                    selection_error = Some(js_format!(
                        "{}, but no default model is configured for provider \"{}\". Use /model to select a model.",
                        action_label,
                        provider_id
                    ));
                }
                Some(_) if provider_models.is_empty() => {
                    selection_error = Some(js_format!(
                        "{}, but no models are available for that provider. Use /model to select a model.",
                        action_label
                    ));
                }
                Some(default_model_text) => {
                    selected_model = provider_models
                        .iter()
                        .find(|model| model.id == default_model_text)
                        .cloned();
                    match selected_model.clone() {
                        None => {
                            selection_error = Some(js_format!(
                                "{}, but its default model \"{}\" is not available. Use /model to select a model.",
                                action_label,
                                default_model_text
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

        self.update_available_provider_count()?;
        self.footer.borrow_mut().invalidate();
        self.update_editor_border_color();
        let auth_path = crate::config::get_auth_path()?;
        if let Some(model) = &selected_model {
            self.show_status(&js_format!(
                "{}. Selected {}. Credentials saved to {}",
                action_label,
                model.id,
                auth_path
            ));
            self.schedule_anthropic_subscription_auth_warning(Some(model.clone()));
            self.check_daxnuts_easter_egg(model);
        } else {
            self.show_status(&js_format!("{}. Credentials saved to {}", action_label, auth_path));
            if let Some(error) = selection_error {
                self.show_error(&error);
            } else {
                self.schedule_anthropic_subscription_auth_warning(None);
            }
        }
        Ok(())
    }

    fn set_extension_status(&self, key: &JsString, text: Option<ExtensionStatus>) {
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
                Some(ExtensionStatus::Text(with_theme(|theme| {
                    theme.fg(ThemeColor::Muted, "bwrap: full access")
                }))),
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
        self.set_extension_status(&JsString::from("bwrap"), Some(ExtensionStatus::Text(text)));
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
        self.stop()
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        self.runtime
            .clone()
            .dispose()
            .await
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        let resume_command = {
            let session = self.session();
            let manager = session.session_manager();
            format_resume_command(&manager).unwrap_or_else(|error| error.raise_uncaught())
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
        self.runtime
            .clone()
            .dispose()
            .await
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        self.theme_controller.borrow_mut().disable_auto_sync();
        self.ui.terminal().borrow_mut().drain_input(1000, 50);
        self.stop()
            .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        std::process::exit(0);
    }

    async fn restart_from_signal(&self) {
        self.theme_controller.borrow_mut().disable_auto_sync();
        let result = self.runtime.clone().restart(None, true).await;
        // `void this.restartProcess(...)`: a rejection is an uncaught exception.
        if let Err(error) = result {
            crate::utils::uncaught::throw_callback_uncaught(error);
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
        if matches!(
            &request,
            UiRequest::RuntimeNewSession { .. }
                | UiRequest::RuntimeFork { .. }
                | UiRequest::RuntimeNavigateTree { .. }
                | UiRequest::RuntimeSwitchSession { .. }
                | UiRequest::RuntimeReload { .. }
                | UiRequest::RuntimeRestart { .. }
        ) {
            with_mode(self.ui_id.get(), |mode| {
                tokio::task::spawn_local(async move {
                    mode.handle_ui_request_inner(request)
                        .await
                        .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
                });
            })
            .expect("runtime requests require their live UI owner");
        } else {
            self.handle_ui_request_inner(request)
                .await
                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
        }
    }

    async fn handle_ui_request_inner(&self, request: UiRequest) -> Result<(), CallbackError> {
        match request {
            UiRequest::LifecycleTransition(transition) | UiRequest::RendererCompletion(transition) => {
                tokio::task::spawn_local(transition);
            }
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
                let restore_editor = crate::modes::interactive::editor_dialog_restore::capture_editor_dialog_restore(
                    Rc::clone(&self.editor_container),
                    Rc::clone(&self.ui),
                );
                let respond = Rc::new(RefCell::new(Some(respond)));
                let select_respond = Rc::clone(&respond);
                let restore = Rc::clone(&restore_editor);
                let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |option| {
                    restore();
                    respond_once(&select_respond, Some(option));
                });
                let cancel_respond = Rc::clone(&respond);
                let restore = Rc::clone(&restore_editor);
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    restore();
                    respond_once(&cancel_respond, None);
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
                                with_mode(mode, |mode| {
                                    mode.toggle_tool_output_expansion()
                                        .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
                                });
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
                let restore_editor = crate::modes::interactive::editor_dialog_restore::capture_editor_dialog_restore(
                    Rc::clone(&self.editor_container),
                    Rc::clone(&self.ui),
                );
                let respond = Rc::new(RefCell::new(Some(respond)));
                let yes_respond = Rc::clone(&respond);
                let restore = Rc::clone(&restore_editor);
                let on_select: Rc<dyn Fn(JsString)> = Rc::new(move |option| {
                    restore();
                    respond_once(&yes_respond, option == "Yes");
                });
                let cancel_respond = Rc::clone(&respond);
                let restore = Rc::clone(&restore_editor);
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    restore();
                    respond_once(&cancel_respond, false);
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
                let restore_editor = crate::modes::interactive::editor_dialog_restore::capture_editor_dialog_restore(
                    Rc::clone(&self.editor_container),
                    Rc::clone(&self.ui),
                );
                let respond = Rc::new(RefCell::new(Some(respond)));
                let submit_respond = Rc::clone(&respond);
                let restore = Rc::clone(&restore_editor);
                let on_submit: Rc<dyn Fn(JsString)> = Rc::new(move |value| {
                    restore();
                    respond_once(&submit_respond, Some(value));
                });
                let cancel_respond = Rc::clone(&respond);
                let restore = Rc::clone(&restore_editor);
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    restore();
                    respond_once(&cancel_respond, None);
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
                let restore_editor = crate::modes::interactive::editor_dialog_restore::capture_editor_dialog_restore(
                    Rc::clone(&self.editor_container),
                    Rc::clone(&self.ui),
                );
                let respond = Rc::new(RefCell::new(Some(respond)));
                let submit_respond = Rc::clone(&respond);
                let restore = Rc::clone(&restore_editor);
                let on_submit: Rc<dyn Fn(JsString)> = Rc::new(move |value| {
                    restore();
                    respond_once(&submit_respond, Some(value));
                });
                let cancel_respond = Rc::clone(&respond);
                let restore = Rc::clone(&restore_editor);
                let on_cancel: Rc<dyn Fn()> = Rc::new(move || {
                    restore();
                    respond_once(&cancel_respond, None);
                });
                let editor = ExtensionEditorComponent::new(
                    Rc::new(InteractiveTuiHost(Rc::clone(&self.ui))),
                    self.keybindings.clone(),
                    &title,
                    prefill.as_ref(),
                    on_submit,
                    on_cancel,
                    crate::modes::interactive::components::extension_editor::ExtensionEditorOptions {
                        editor: EditorOptions::default(),
                        external_editor_command: Some(self.settings(|settings| settings.get_external_editor_command())),
                    },
                );
                self.show_selector(component(editor));
            }
            UiRequest::Notify { message, notify_type } => match notify_type {
                Some(NotifyType::Error) => self.show_error(&message),
                Some(NotifyType::Warning) => self.show_warning(&message),
                Some(NotifyType::Info) | None => self.show_status(&message),
            },
            UiRequest::BashOutput(chunk) => self.append_bash_output(&chunk),
            UiRequest::RequestRender => self.ui.request_render(),
            UiRequest::RequestComponentRender(identity) => self.request_extension_component_render(identity)?,
            UiRequest::RequestRenderForce => self.ui.request_render_force(),
            UiRequest::SetStatus { key, text } => {
                self.footer_data_provider.set_extension_status(&key, text);
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
                self.remove_extension_widget(&key);
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
                self.editor().borrow_mut().set_text(&text)?;
                self.ui.request_render();
            }
            UiRequest::SetToolsExpanded(expanded) => self.set_tools_expanded(expanded)?,
            UiRequest::SetFooter(factory) => {
                self.set_extension_footer(factory);
            }
            UiRequest::SetDefaultFooter(factory) => {
                self.set_default_extension_footer(factory);
            }
            UiRequest::SetHeader { factory, respond } => {
                let _ = respond.send(self.set_extension_header(factory));
            }
            UiRequest::Custom {
                factory,
                options,
                respond,
            } => self.show_extension_custom(Rc::new(move |context, done| factory(context, done)), options, respond)?,
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
            UiRequest::SetEditorComponent(factory) => self.set_custom_editor_component(factory)?,
            UiRequest::AbortAndRestoreQueued => {
                self.restore_queued_messages_to_editor(true)?;
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
                    let setup = options.setup.map(crate::modes::extension_actions::adapt_session_setup);
                    let with_session = options
                        .with_session
                        .map(crate::modes::extension_actions::adapt_with_session);
                    crate::core::agent_session_runtime::NewSessionOptions {
                        parent_session: options.parent_session,
                        setup,
                        with_session,
                    }
                });
                let runtime = self.runtime.clone();
                let result = runtime
                    .new_session(
                        options.unwrap_or(crate::core::agent_session_runtime::NewSessionOptions {
                            parent_session: None,
                            setup: None,
                            with_session: None,
                        }),
                    )
                    .await;
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
                    .unwrap_or(crate::core::agent_session_runtime::ForkPosition::Before);
                let with_session = options
                    .and_then(|options| options.with_session)
                    .map(crate::modes::extension_actions::adapt_with_session);
                let runtime = self.runtime.clone();
                let result = runtime.fork(&entry_id, position, with_session).await;
                let transition = match result {
                    Ok(result) => {
                        if !result.cancelled {
                            self.pending_compaction_summary_edit_id.borrow_mut().take();
                            self.live_compaction_display.borrow_mut().take();
                            self.editor()
                                .borrow_mut()
                                .set_text(&result.selected_text.unwrap_or_default())?;
                            self.show_status("Forked to new session");
                        }
                        Ok(SessionTransitionResult {
                            cancelled: result.cancelled,
                        })
                    }
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
                let transition = (|| match result {
                    Ok(result) if result.cancelled => Ok(SessionTransitionResult { cancelled: true }),
                    Ok(result) => {
                        self.pending_compaction_summary_edit_id.borrow_mut().take();
                        self.live_compaction_display.borrow_mut().take();
                        self.chat_container.borrow_mut().clear();
                        self.render_initial_messages();
                        if let Some(editor_text) = result.editor_text.filter(|text| !text.is_empty())
                            && self.editor().borrow().get_text()?.trim().is_empty()
                        {
                            self.editor().borrow_mut().set_text(&editor_text)?;
                        }
                        self.show_status("Navigated to selected point");
                        let mode_id = self.ui_id.get();
                        with_mode(mode_id, |mode| {
                            tokio::task::spawn_local(async move { mode.flush_compaction_queue(false).await });
                        });
                        Ok(SessionTransitionResult { cancelled: false })
                    }
                    Err(error) => Err(CallbackError::from(error)),
                })();
                let _ = respond.send(transition);
            }
            UiRequest::RuntimeSwitchSession { path, options, respond } => {
                let result = self.handle_resume_session(&path, options).await;
                let _ = respond.send(Ok(result));
            }
            UiRequest::RuntimeReload { respond } => {
                self.handle_reload_command().await;
                let _ = respond.send(());
            }
            UiRequest::RuntimeRestart { options, respond } => {
                let runtime = self.runtime.clone();
                let result = runtime
                    .restart(options.as_ref().and_then(|options| options.notice.as_ref()), true)
                    .await
                    .map_err(crate::core::extensions::types::HandlerError::from);
                let _ = respond.send(result);
            }
        }
        Ok(())
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

    fn set_extension_header(&self, factory: Option<ExtensionUiHeaderFactory>) -> Result<(), CallbackError> {
        let Some(built_in) = self.built_in_header.borrow().clone() else {
            return Ok(());
        };
        let custom = self.custom_header.borrow().clone();
        if let Some(custom) = &custom {
            dispose_extension_component(&custom.component);
        }
        let current = custom.as_ref().unwrap_or(&built_in);
        let index = self
            .header_container
            .borrow()
            .children()
            .iter()
            .position(|child| Rc::ptr_eq(child, &current.component));
        match factory {
            Some(factory) => {
                let header = factory(self.extension_factory_context())?;
                let mounted = MountedHeader {
                    component: component(ExtensionComponentAdapter {
                        inner: header.component.into(),
                    }),
                    set_expanded: header.set_expanded,
                };
                *self.custom_header.borrow_mut() = Some(mounted.clone());
                self.apply_header_expansion(&mounted)?;
                if let Some(index) = index {
                    self.header_container.borrow_mut().remove_child(&current.component);
                    self.header_container
                        .borrow_mut()
                        .insert_child(index, mounted.component);
                } else {
                    self.header_container.borrow_mut().insert_child(0, mounted.component);
                }
            }
            None => {
                self.custom_header.borrow_mut().take();
                self.apply_header_expansion(&built_in)?;
                if let Some(index) = index {
                    self.header_container.borrow_mut().remove_child(&current.component);
                    self.header_container
                        .borrow_mut()
                        .insert_child(index, built_in.component);
                }
            }
        }
        self.ui.request_render();
        Ok(())
    }

    fn apply_header_expansion(&self, header: &MountedHeader) -> Result<(), CallbackError> {
        if let Some(set_expanded) = &header.set_expanded {
            set_expanded(self.tool_output_expanded.get())?;
        }
        Ok(())
    }

    fn expand_active_header(&self) -> Result<(), CallbackError> {
        let header = self
            .custom_header
            .borrow()
            .clone()
            .or_else(|| self.built_in_header.borrow().clone());
        if let Some(header) = header {
            self.apply_header_expansion(&header)?;
        }
        Ok(())
    }

    fn show_extension_custom(
        &self,
        factory: LocalExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
        respond: tokio::sync::oneshot::Sender<Result<Option<JsValue>, CallbackError>>,
    ) -> Result<(), CallbackError> {
        let context = self.extension_factory_context();
        let respond = Arc::new(std::sync::Mutex::new(Some(respond)));
        let done_respond = Arc::clone(&respond);
        let component_slot = Rc::new(RefCell::new(None::<std::rc::Weak<dyn ExtensionUiComponent>>));
        let done_component = Rc::clone(&component_slot);
        let mode = self.ui_id.get();
        let overlay = options.as_ref().and_then(|options| options.overlay).unwrap_or(false);
        let restore_slot = crate::modes::interactive::editor_dialog_restore::capture_editor_dialog_restore(
            Rc::clone(&self.editor_container),
            Rc::clone(&self.ui),
        );
        let done_restore_slot = Rc::clone(&restore_slot);
        let saved_text = self.editor().borrow().get_text()?;
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
                    mode.editor()
                        .borrow_mut()
                        .set_text(&done_text)
                        .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
                    done_restore_slot();
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
                    let _ = respond.send(Err(error.into()));
                }
                return Ok(());
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
                            mode.editor()
                                .borrow_mut()
                                .set_text(&saved_text)
                                .unwrap_or_else(|error| crate::utils::uncaught::throw_callback_uncaught(error));
                            restore_slot();
                        }
                    });
                    if let Some(respond) = respond.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
                        let _ = respond.send(Err(error.into()));
                    }
                }
            }
        });
        Ok(())
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
                    );
                    component_value.update_content((**assistant).clone());
                    let component_ref = component(component_value);
                    self.transcript_tail_container
                        .borrow_mut()
                        .add_child(Rc::clone(&component_ref));
                    *self.streaming_component.borrow_mut() = Some(component_ref);
                    *self.streaming_message.borrow_mut() = Some((**assistant).clone());
                    self.ui.request_render();
                }
                AgentMessage::Custom(_) => {
                    self.add_message_to_chat(&message, false);
                    self.ui.request_render();
                }
                AgentMessage::User(_) => {
                    if crate::core::runtime_message_markers::is_duplicate_turn_guard_message(
                        None,
                        runtime_message_marker,
                    ) {
                        return;
                    }
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
                    pi_ai::types::AssistantMessageEvent::ThinkingEnd {
                        content_index, partial, ..
                    } => {
                        matches!(partial.content.get(*content_index), Some(pi_ai::types::AssistantContent::Thinking(content))
                            if crate::core::assistant_narration::is_server_narration_signature(content.thinking_signature.as_deref()))
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
                        component_value.update_content((**assistant).clone());
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
                        component_value.update_content(assistant.clone());
                    }
                    self.move_streaming_component_to_chat();
                    self.add_completed_thinking_duration_before_tool_call(&assistant);

                    if matches!(
                        assistant.stop_reason,
                        pi_ai::types::StopReason::Aborted | pi_ai::types::StopReason::Error
                    ) {
                        let error_message = error_message
                            .or_else(|| assistant.error_message.clone().filter(|error| !error.is_empty()))
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
                    component_value.mark_execution_started(started_at, ToolExecutionStartOptions::default());
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
                    component_value.mark_execution_started(started_at, ToolExecutionStartOptions::default());
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
            AgentSessionEvent::CompactionStart(event) => {
                self.stop_working_loader();
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
                } else if let Some(error) = event.error_message.as_ref().filter(|error| !error.is_empty()) {
                    let source_hint = self.auto_compaction_source_hint.borrow().clone();
                    let failure_message =
                        format_compaction_failure_message(error, &JsString::from(&event.reason), source_hint.as_ref());
                    if event.reason == "manual" {
                        self.show_error(&failure_message);
                    } else {
                        self.chat_container.borrow_mut().add_child(component(Spacer::new(1)));
                        self.chat_container.borrow_mut().add_child(component(Text::new(
                            with_theme(|theme| theme.fg(ThemeColor::Error, failure_message.clone())),
                            1,
                            0,
                        )));
                    }
                }
                *self.auto_compaction_source_hint.borrow_mut() = None;
                self.sync_working_loader_visibility();
                if let Some(mode) = with_mode(self.ui_id.get(), |mode| mode) {
                    tokio::task::spawn_local(async move {
                        mode.flush_compaction_queue(event.will_retry).await;
                    });
                }
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
                let retry_message = move |seconds: f64| {
                    js_format!(
                        "Retrying ({}/{}) in {}s... ({} to cancel)",
                        attempt,
                        max_attempts,
                        pi_js_compat::number::js_number_to_string(seconds),
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
                    retry_message((event.delay_ms / 1000.0).ceil()),
                    Some(self.get_working_loader_indicator(WorkingLoaderView::Main)),
                ));
                *self.retry_loader.borrow_mut() = Some(Rc::clone(&loader));
                let tick_loader = Rc::clone(&loader);
                let countdown = CountdownTimer::new(
                    event.delay_ms,
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
                        event
                            .final_error
                            .as_ref()
                            .filter(|error| !error.is_empty())
                            .unwrap_or(&JsString::from("Unknown error"))
                    ));
                }
                self.ui.request_render();
            }
            AgentSessionEvent::ProviderStreamRetry(event) => {
                let message = Self::format_provider_stream_retry_message(&event.retry);
                self.chat_container.borrow_mut().add_child(component(Text::new(
                    with_theme(|theme| theme.fg(ThemeColor::Warning, &message)),
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
            AgentSessionEvent::Agent(AgentEvent::AgentEnd { .. }) => {}
            AgentSessionEvent::Agent(AgentEvent::TurnStart) => {}
            AgentSessionEvent::Agent(AgentEvent::TurnEnd { .. }) => {}
            AgentSessionEvent::Message(_) => {}
        }
    }

    fn format_provider_stream_retry_message(retry: &pi_ai::types::ProviderRetryEvent) -> JsString {
        use pi_js_compat::number::js_number_to_string;
        let attempts = retry.max_attempts.map_or_else(
            || js_format!(" (attempt {})", js_number_to_string(retry.attempt)),
            |max| js_format!(" ({}/{})", js_number_to_string(retry.attempt), js_number_to_string(max)),
        );
        let delay = retry
            .delay_ms
            .map(|delay| js_format!(" in {}s", js_number_to_string((delay / 1000.0).ceil())))
            .unwrap_or_default();
        let fallback_transport = retry.fallback_transport.map(|transport| match transport {
            pi_ai::types::Transport::Sse => "SSE",
            pi_ai::types::Transport::Websocket => "WEBSOCKET",
            pi_ai::types::Transport::WebsocketCached => "WEBSOCKET-CACHED",
            pi_ai::types::Transport::Auto => "AUTO",
        });
        let action = fallback_transport.map_or_else(
            || js_format!("provider retry{}{}", attempts, delay),
            |transport| js_format!("falling back to {}", transport),
        );
        js_format!("{}: {}", action, retry.reason)
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
        let definition = self.session().extension_runner().get_tool_definition(tool_name);
        let component_value = ToolExecutionComponent::new(
            tool_name,
            tool_call_id,
            args,
            ToolExecutionOptions {
                show_images: Some(self.settings(|settings| settings.get_show_images())),
                image_width_cells: Some(self.settings(|settings| settings.get_image_width_cells())),
                hide_output: Some(self.settings(|settings| settings.get_hide_tool_output())),
            },
            definition,
            request_render,
            cwd,
        );
        let completion_sender = self.ui_sender.clone();
        component_value
            .borrow()
            .set_render_completion_executor(Some(Arc::new(move |completion| {
                if completion_sender
                    .send(UiRequest::RendererCompletion(completion))
                    .is_err()
                {
                    // Disposed owner: the same no-op boundary as InteractiveUiContext::send.
                }
            })));
        component_value
            .borrow_mut()
            .set_expanded(self.tool_output_expanded.get());
        component_value
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
        lines.push(js_format!("Model: {}/{}.", source.provider, source.model));
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
            return rest;
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
    mod scripted_registry {
        use crate as pi_coding_agent;
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/scripted_registry.rs"
        ));
    }
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
    use pi_tui::terminal::{Terminal, TerminalEvent, TerminalRawModeError};

    use super::*;
    use crate::core::agent_session::{AgentSession, AgentSessionConfig};
    use crate::core::agent_session_runtime::{
        CreateAgentSessionRuntimeOptions, CreateAgentSessionRuntimeResult, create_agent_session_runtime,
    };
    use crate::core::agent_session_services::AgentSessionServices;
    use crate::core::auth_storage::AuthStorage;
    use crate::core::model_registry::ModelRegistry;
    use crate::core::resource_loader::StaticResourceLoader;
    use crate::core::session_manager::SessionManager;
    use crate::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};

    type TerminalLifecycleObserver = Rc<dyn Fn(&'static str) -> Result<(), CallbackError>>;

    struct CaptureTerminal {
        rows: usize,
        columns: usize,
        writes: Vec<JsString>,
        titles: Vec<JsString>,
        events: Vec<TerminalEvent>,
        progress: Vec<bool>,
        start_error: Option<TerminalRawModeError>,
        lifecycle_observer: Option<TerminalLifecycleObserver>,
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
        fn start(&mut self) -> Result<(), CallbackError> {
            if let Some(observer) = &self.lifecycle_observer {
                observer("start")?;
            }
            match &self.start_error {
                Some(error) => Err(error.clone().into()),
                None => Ok(()),
            }
        }
        fn stop(&mut self) -> Result<(), CallbackError> {
            if let Some(observer) = &self.lifecycle_observer {
                observer("stop")?;
            }
            Ok(())
        }
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
        fn set_title(&mut self, title: &JsString) {
            self.titles.push(title.clone());
        }
        fn set_progress(&mut self, active: bool) {
            self.progress.push(active);
        }
        fn poll_events(&mut self) -> Vec<TerminalEvent> {
            std::mem::take(&mut self.events)
        }
    }

    fn completed_fixture_response(text: &str) -> pi_ai::providers::faux::FauxResponseStep {
        faux_assistant_message(
            vec![
                faux_text(text),
                pi_ai::providers::faux::faux_tool_call(
                    "end_turn",
                    pi_js_compat::js_json::JsObject::from_iter([("reason".into(), JsValue::String(text.into()))]),
                    Some(format!("end-{text}")),
                ),
            ],
            Default::default(),
        )
        .into()
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fixture_streams_keep_their_responses_across_other_registry_refreshes() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (first, _first_terminal, first_faux, _first_temp) = build_mode().await;
                let (second, _second_terminal, second_faux, _second_temp) = build_mode().await;
                first_faux.set_responses(vec![completed_fixture_response("first session")]);
                second_faux.set_responses(vec![completed_fixture_response("second session")]);
                second.session().model_registry().refresh().unwrap();
                first
                    .session()
                    .prompt("first request", PromptOptions::default())
                    .await
                    .unwrap();
                assert_eq!(
                    first.session().get_last_assistant_text(),
                    Some("first session".into()),
                    "messages: {:?}",
                    first.session().messages()
                );
                first.session().model_registry().refresh().unwrap();
                second
                    .session()
                    .prompt("second request", PromptOptions::default())
                    .await
                    .unwrap();
                assert_eq!(
                    second.session().get_last_assistant_text(),
                    Some("second session".into())
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn hidden_coordination_queue_keeps_user_input_and_delivery_intact() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, faux, _temp) = build_mode().await;
                mode.settings_mut(|settings| settings.set_hide_coordination_messages(true));
                let gate = Arc::new(tokio::sync::Notify::new());
                let entered = Arc::new(tokio::sync::Notify::new());
                let stream_gate = Arc::clone(&gate);
                let stream_entered = Arc::clone(&entered);
                faux.set_responses(vec![
                    pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(move |_, _, _, _| {
                        let gate = Arc::clone(&stream_gate);
                        let entered = Arc::clone(&stream_entered);
                        Box::pin(async move {
                            entered.notify_one();
                            gate.notified().await;
                            Ok(pi_ai::providers::faux::faux_assistant_message(
                                "done",
                                Default::default(),
                            ))
                        })
                    })),
                    pi_ai::providers::faux::faux_assistant_message("queued", Default::default()).into(),
                ]);
                let session = mode.session();
                let task =
                    tokio::task::spawn_local(async move { session.prompt("start", PromptOptions::default()).await });
                entered.notified().await;
                let envelope = JsString::from("From:\n- agent: peer\n\nMessage:\nsecret coordination");
                mode.session()
                    .prompt(
                        envelope.clone(),
                        PromptOptions {
                            streaming_behavior: Some(StreamingBehavior::FollowUp),
                            source: Some(pi_ai::types::InputSource::Extension),
                            ..Default::default()
                        },
                    )
                    .await
                    .unwrap();
                mode.session().follow_up("visible user input", &[]).await.unwrap();
                mode.update_pending_messages_display();
                let output = pi_tui::utils::strip_terminal_sequences(JsString::join(
                    mode.pending_messages_container.borrow_mut().render(100),
                    "\n",
                ));
                assert!(!output.contains("secret coordination"), "{output:?}");
                assert!(output.contains("visible user input"), "{output:?}");
                assert_eq!(
                    mode.session().get_follow_up_messages(),
                    vec![envelope.clone(), "visible user input".into()]
                );
                mode.compaction_queued_messages
                    .borrow_mut()
                    .push(CompactionQueuedMessage {
                        text: envelope.clone(),
                        mode: "steer",
                    });
                let (steering, follow_up) = mode.get_all_queued_messages(true);
                assert_eq!(steering, vec![envelope.clone()], "compaction input is not filtered");
                assert_eq!(follow_up, vec![JsString::from("visible user input")]);
                assert_eq!(mode.get_all_queued_messages(false).1.len(), 2);
                mode.settings_mut(|settings| settings.set_hide_coordination_messages(false));
                assert_eq!(mode.get_all_queued_messages(true).1.len(), 2);
                gate.notify_one();
                task.await.unwrap().unwrap();
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn coordination_transcript_hides_and_restores_existing_components() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                mode.settings_mut(|settings| settings.set_hide_coordination_messages(true));
                let message: AgentMessage = pi_js_compat::js_json::from_value(&pi_js_compat::js_json!({
                    "role":"user", "content":"From:\n- agent: peer\n\nMessage:\ncoordination body",
                    "inputSource":"extension", "timestamp":0,
                }))
                .unwrap();
                mode.add_message_to_chat(&message, false);
                assert!(mode.chat_container.borrow_mut().render(100).is_empty());
                let children = mode.chat_container.borrow().children().to_vec();
                mode.settings_mut(|settings| settings.set_hide_coordination_messages(false));
                let frame = JsString::join(mode.chat_container.borrow_mut().render(100), "\n");
                assert!(frame.contains("coordination body"), "{frame:?}");
                assert_eq!(children.len(), mode.chat_container.borrow().children().len());
                assert!(Rc::ptr_eq(&children[0], &mode.chat_container.borrow().children()[0]));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_dialog_restores_previous_editor_children_and_focus() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let previous = component(Text::new("previous panel", 0, 0));
                let focus = component(pi_tui::Input::new(Default::default()));
                *mode.editor_container.borrow_mut() =
                    Container::with_children(vec![Rc::clone(&previous), Rc::clone(&focus)]);
                mode.ui.set_focus(Some(Rc::clone(&focus)));
                let (respond, receiver) = tokio::sync::oneshot::channel();
                mode.handle_ui_request(UiRequest::Input {
                    title: "nested input".into(),
                    placeholder: None,
                    timeout: None,
                    secret: false,
                    respond,
                })
                .await;
                mode.ui
                    .get_focused_component()
                    .unwrap()
                    .borrow_mut()
                    .handle_input(&JsString::from("\u{1b}"));
                assert_eq!(receiver.await.unwrap(), None);
                let children = mode.editor_container.borrow().children().to_vec();
                assert_eq!(children.len(), 2);
                assert!(Rc::ptr_eq(&children[0], &previous));
                assert!(Rc::ptr_eq(&children[1], &focus));
                assert!(Rc::ptr_eq(&mode.ui.get_focused_component().unwrap(), &focus));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn editor_dialog_restore_matches_frozen_snapshots_and_raw_children() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        let local = tokio::task::LocalSet::new();
        local.run_until(async {
            let (mode, _terminal, _faux, _temp) = build_mode().await;
            let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");
            for focus_index in [1, -1] {
                let raw = JsString::from_utf16(vec![0xd800,b'x' as u16,0xdc00]);
                let changed = JsString::from("changed ") + &raw;
                let first = Rc::new(RefCell::new(Text::new(raw.clone(),0,0)));
                let first_ref: ComponentRef = first.clone();
                let second = component(Text::new("second",0,0));
                let container = Rc::new(RefCell::new(Container::with_children(vec![first_ref,Rc::clone(&second)])));
                mode.ui.set_focus((focus_index==1).then(||Rc::clone(&second)));
                let restore = crate::modes::interactive::editor_dialog_restore::capture_editor_dialog_restore(Rc::clone(&container),Rc::clone(&mode.ui));
                let dialog = component(Text::new("dialog",0,0));
                *container.borrow_mut() = Container::with_children(vec![Rc::clone(&dialog)]);
                mode.ui.set_focus(Some(dialog));
                first.borrow_mut().set_text(changed.clone());
                restore();
                let mut frames = vec![container.borrow_mut().render(30)];
                let focus = if mode.ui.get_focused_component().is_some_and(|focused|Rc::ptr_eq(&focused,&second)) {1} else {-1};
                container.borrow_mut().clear();
                mode.ui.set_focus(None);
                restore();
                frames.push(container.borrow_mut().render(30));
                let focus_again = if mode.ui.get_focused_component().is_some_and(|focused|Rc::ptr_eq(&focused,&second)) {1} else {-1};
                let request = pi_js_compat::js_json!({
                    "id":"editor-restore", "module":"modes/interactive/editor-dialog-restore.ts", "operation":"editorRestore",
                    "texts":[raw,"second"],"changedText":changed,"focusIndex":focus_index,"width":30,
                });
                let expected = oracle.request(&request);
                assert_eq!(expected.get("ok"),Some(&JsValue::Bool(true)),"{expected:?}");
                assert_eq!(pi_js_compat::js_json!({"frames":frames,"focus":focus,"focusAgain":focus_again}),expected.get("output").unwrap().clone());
            }
        }).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn install_telemetry_abort_timeout_matches_frozen() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        const CHILD: &str = "PI_TELEMETRY_ABORT_CHILD";
        const RESULT: &str = "PI_TELEMETRY_ABORT_RESULT";
        if let Ok(branch) = std::env::var(CHILD) {
            struct ProbeFetch {
                captured: Arc<std::sync::Mutex<Option<pi_js_compat::abort::AbortSignal>>>,
                complete: bool,
            }
            impl Fetch for ProbeFetch {
                fn fetch(
                    &self,
                    _request: HttpRequest,
                    signal: Option<pi_js_compat::abort::AbortSignal>,
                ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, CallbackError>>
                {
                    *self.captured.lock().unwrap() = signal.clone();
                    let complete = self.complete;
                    Box::pin(async move {
                        if !complete {
                            signal.unwrap().wait().await;
                        }
                        Err(CallbackError::new("fixture request finished"))
                    })
                }
            }
            tokio::task::LocalSet::new().run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let captured = Arc::new(std::sync::Mutex::new(None));
                *mode.http_fetch.borrow_mut() = Some(Arc::new(ProbeFetch { captured:captured.clone(),complete:branch=="complete" }));
                mode.report_install_telemetry(&JsString::from("abort-proof"));
                let signal = tokio::time::timeout(Duration::from_secs(1), async {
                    loop { if let Some(signal) = captured.lock().unwrap().clone() { break signal; } tokio::task::yield_now().await; }
                }).await.expect("mock fetch received signal");
                let output = match tokio::time::timeout(Duration::from_secs(6),signal.wait()).await {
                    Ok(reason) => {
                        let kind = match &reason { pi_js_compat::abort::AbortReason::DomException(_) => "DOMException", pi_js_compat::abort::AbortReason::Error(_) => "Error", _ => "value" };
                        let value = reason.value();
                        serde_json::json!({"aborted":signal.aborted(),"kind":kind,"name":value.get("name"),"message":value.get("message"),"code":value.get("code")})
                    }
                    Err(_) => serde_json::json!({"aborted":signal.aborted(),"kind":"missing"}),
                };
                std::fs::write(std::env::var(RESULT).expect("result path"), output.to_string()).expect("write native reason result");
            }).await;
            return;
        }
        let mut oracle = oracle::TsOracle::spawn("interactive-abort-timeout-oracle.ts", "packages/coding-agent/src");
        let mut mismatches = Vec::new();
        let results = tempfile::tempdir().unwrap();
        for complete in [false, true] {
            let result_path = results.path().join(format!("result-{complete}.json"));
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "modes::interactive::interactive_mode::ui_tests::install_telemetry_abort_timeout_matches_frozen",
                    "--nocapture",
                ])
                .env(CHILD, if complete { "complete" } else { "pending" })
                .env(RESULT, &result_path)
                .env_remove("PI_OFFLINE")
                .env("PI_TELEMETRY", "1")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let expected = oracle.request(&serde_json::json!({"complete":complete}));
            assert_eq!(expected["ok"], true, "source: {expected}");
            let child = child.wait_with_output().unwrap();
            assert!(
                child.status.success(),
                "native child: {}",
                String::from_utf8_lossy(&child.stderr)
            );
            let actual: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&result_path).expect("native reason result")).unwrap();
            if actual != expected["output"] {
                eprintln!("complete={complete}: native {actual} / source {}", expected["output"]);
                mismatches.push(complete);
            }
        }
        assert!(
            mismatches.is_empty(),
            "timeout reason/lifetime mismatches: {mismatches:?}"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn session_command_frames_match_frozen() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new().run_until(async {
            let mut oracle = oracle::TsOracle::spawn("session-info-oracle.ts", "packages/coding-agent/src");
            for persisted in [false, true] {
                let (mode, _terminal, _faux, _temp) = if persisted { build_mode_with_persisted_session().await } else { build_mode().await };
                let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS.iter().map(|name| ((*name).into(),crate::modes::interactive::theme::ColorValue::Index(0.0))).collect();
                crate::modes::interactive::theme::set_theme_instance(Arc::new(Theme::new(&colors,&colors,crate::modes::interactive::theme::ColorMode::Color256,None,None).unwrap()));
                for populated in [false,true] {
                    if populated {
                        let session = mode.session();
                        let mut manager = session.session_manager();
                        manager.set_session_name(Some(&JsString::from_utf16(vec![83,55296,32,73]))).unwrap();
                        let mut message = AssistantMessage::empty("faux","faux","faux-1",0.0);
                        message.usage.input = 12345.25; message.usage.output = 6789.5; message.usage.cache_read = 23456.75; message.usage.cache_write = 1000.0; message.usage.cost.total = 0.123456;
                        manager.append_message(AgentMessage::Assistant(Box::new(message))).unwrap();
                    }
                    for width in [25,120] {
                        let stats = mode.session().get_session_stats();
                        let name = mode.session().session_manager().get_session_name();
                        let request = serde_json::json!({"stats":stats,"name":name.as_ref().map(|name|name.units()),"width":width});
                        let expected = oracle.request(&request);
                        assert_eq!(expected["ok"],true,"source: {expected}");
                        mode.chat_container.borrow_mut().clear();
                        mode.handle_session_command();
                        let lines: Vec<_> = mode.chat_container.borrow_mut().render(width).iter().map(|line|line.units().to_vec()).collect();
                        assert_eq!(serde_json::json!(lines),expected["output"]["lines"],"persisted={persisted},populated={populated},width={width}");
                    }
                }
            }
        }).await;
    }

    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_path_commands.rs"
    ));
    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_transfer_commands.rs"
    ));
    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_display_commands.rs"
    ));
    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_help_notices.rs"
    ));
    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_event_display.rs"
    ));
    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_audit_native.rs"
    ));
    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_startup.rs"
    ));
    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_session_options.rs"
    ));
    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_live_compaction.rs"
    ));

    #[tokio::test]
    async fn cd_and_resume_supply_the_frozen_interactive_trust_context() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new().run_until(async {
            let mut source=oracle::TsOracle::spawn("interactive-trust-context-oracle.mts","packages/coding-agent/src");
            for operation in ["resume","cd","cd-relative"] {
                let observed=Arc::new(Mutex::new(Vec::new()));
                let captured=observed.clone();
                let (mode,_terminal,_faux,temp)=build_mode_with_extra_providers(StaticResourceLoader::new(),None,SettingsManager::in_memory(Default::default(),SettingsManagerCreateOptions::default()),Vec::new(),serde_json::Map::new(),true,TestModeSetup{runtime_request_observer:Some(Arc::new(move|request|{
                    if request.session_start_event.as_ref().is_some_and(|event| event.reason != crate::core::extensions::types::SessionStartReason::Startup) {
                        captured.lock().unwrap().push(request.project_trust_context.as_ref().map(|context|serde_json::json!({"cwd":context.cwd,"mode":context.mode,"hasUI":context.has_ui})));
                    }
                })),..Default::default()}).await;
                let target=temp.path().join("trust-target");
                std::fs::create_dir(&target).unwrap();
                let target=js_path(&target);
                let mut manager=SessionManager::create(&target,Some(&js_path(&temp.path().join("resume-target"))),None).unwrap();
                manager.persist_for_recovery().unwrap();
                let file=manager.get_session_file().unwrap().clone();
                let expected=source.request(&serde_json::json!({"operation":if operation=="resume" {"resume"} else {"cd"},"cwd":mode.cwd(),"target":target,"file":file,"command":if operation=="cd-relative" {serde_json::json!("trust-target")} else {serde_json::json!(target)}}));
                assert_eq!(expected["context"],serde_json::json!({"cwd":target,"mode":"tui","hasUI":true}));
                let task=tokio::task::spawn_local({let mode=mode.clone();let target=target.clone();async move {
                    if operation=="resume" {mode.handle_resume_session(&file, None).await;}
                    else if operation=="cd-relative" {mode.handle_cd_command("/cd trust-target".into()).await;}
                    else {mode.handle_cd_command(js_format!("/cd {}",target)).await;}
                }});
                tokio::time::timeout(Duration::from_secs(5),async {
                    while !task.is_finished() {mode.handle_ui_requests().await;tokio::time::sleep(Duration::from_millis(1)).await;}
                }).await.expect("UI lifecycle drain");
                task.await.unwrap();
                let observed=observed.lock().unwrap();
                assert_eq!(observed.len(),1,"{operation}");
                assert_eq!(serde_json::json!({"context":observed[0]}),expected,"{operation}");
            }
        }).await;
    }

    #[tokio::test]
    async fn worker_relocation_rebinds_interactive_session_on_ui_owner() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, _terminal, _faux, temp) = build_mode().await;
                let target = temp.path().join("relocated");
                std::fs::create_dir(&target).unwrap();
                let target = js_path(&target);
                let outgoing = mode.session();
                let runner = outgoing.extension_runner();
                runner
                    .create_context()
                    .relocate_after_tool_result("worker-call", &target)
                    .unwrap();
                let worker = std::thread::spawn(move || {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap()
                        .block_on(async move {
                            runner.deliver_tool_result_relocation("worker-call").await.unwrap();
                            runner.activate_tool_result_relocation().await.unwrap();
                        });
                });
                tokio::time::timeout(Duration::from_secs(5), async {
                    while !worker.is_finished() {
                        mode.handle_ui_requests().await;
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                })
                .await
                .expect("UI drain keeps worker lifecycle moving");
                worker.join().unwrap();
                assert_eq!(
                    mode.session().cwd(),
                    &target,
                    "runtime hook must replace the UI session"
                );
                assert!(
                    mode.session().extension_runner().has_ui(),
                    "replacement must bind UI extensions before activation resolves"
                );
            })
            .await;
    }

    #[tokio::test]
    async fn runtime_rebind_holds_gate_and_preserves_worker_owner_submission_order() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new()
            .run_until(async {
                let mut source =
                    oracle::TsOracle::spawn("agent-session-runtime-rebind-oracle.mts", "packages/coding-agent/src");
                let expected = source.request(&serde_json::json!({}));
                assert_eq!(expected["ok"], true, "source: {expected}");
                let (mode, _terminal, _faux, temp) = build_mode().await;
                for name in ["first", "worker", "owner"] {
                    std::fs::create_dir(temp.path().join(name)).unwrap();
                }
                let log = Arc::new(Mutex::new(Vec::<String>::new()));
                let (release, held) = tokio::sync::oneshot::channel();
                let held = Arc::new(Mutex::new(Some(held)));
                let (entered, started) = tokio::sync::oneshot::channel();
                let entered = Arc::new(Mutex::new(Some(entered)));
                let owner_thread = std::thread::current().id();
                mode.runtime.set_rebind_session(Some(Arc::new({
                    let log = Arc::clone(&log);
                    let id = mode.ui_id.get();
                    move |session| {
                        assert_eq!(
                            std::thread::current().id(),
                            owner_thread,
                            "all callbacks run on UI owner"
                        );
                        let name = encode_node_utf8(&crate::utils::paths::node_basename(session.cwd()));
                        log.lock().unwrap().push(format!("start:{name}"));
                        let wait = if name == "first" {
                            entered.lock().unwrap().take().unwrap().send(()).unwrap();
                            held.lock().unwrap().take()
                        } else {
                            None
                        };
                        let log = Arc::clone(&log);
                        let (respond, receiver) = tokio::sync::oneshot::channel();
                        with_mode(id, |mode| {
                            tokio::task::spawn_local(async move {
                                if let Some(wait) = wait {
                                    wait.await.unwrap();
                                }
                                mode.rebind_current_session(true).await.unwrap();
                                log.lock().unwrap().push(format!("end:{name}"));
                                respond.send(()).unwrap();
                            });
                        })
                        .unwrap();
                        Box::pin(async move {
                            receiver.await.unwrap();
                            Ok(())
                        })
                    }
                })));
                let first = tokio::task::spawn_local({
                    let runtime = mode.runtime.clone();
                    let target = js_path(&temp.path().join("first"));
                    async move {
                        runtime.relocate(&target, None).await.unwrap();
                    }
                });
                started.await.unwrap();
                let (submitted, queued) = tokio::sync::oneshot::channel();
                let runtime = mode.runtime.clone();
                let target = js_path(&temp.path().join("worker"));
                let worker = std::thread::spawn(move || {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap()
                        .block_on(async move {
                            let mut relocation = Box::pin(runtime.relocate(&target, None));
                            let mut submitted = Some(submitted);
                            std::future::poll_fn(move |context| {
                                let result = relocation.as_mut().poll(context);
                                if let Some(submitted) = submitted.take() {
                                    submitted.send(()).unwrap();
                                }
                                result
                            })
                            .await
                            .unwrap();
                        });
                });
                queued.await.unwrap();
                let owner = tokio::task::spawn_local({
                    let runtime = mode.runtime.clone();
                    let target = js_path(&temp.path().join("owner"));
                    async move {
                        runtime.relocate(&target, None).await.unwrap();
                    }
                });
                tokio::task::yield_now().await;
                tokio::task::yield_now().await;
                let before_release = log.lock().unwrap().clone();
                release.send(()).unwrap();
                tokio::time::timeout(Duration::from_secs(5), async {
                    first.await.unwrap();
                    owner.await.unwrap();
                    while !worker.is_finished() {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                })
                .await
                .expect("lifecycle completes without another UI drain");
                worker.join().unwrap();
                assert_eq!(
                    serde_json::json!({"beforeRelease":before_release,"afterRelease":*log.lock().unwrap()}),
                    expected["output"]
                );
                assert!(mode.session().cwd().ends_with("/owner"));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn agent_log_preview_replaces_invalid_utf8() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, _terminal, _faux, temp) = build_mode().await;
                let path = temp.path().join("agent.log");
                std::fs::write(&path, b"before\xe2\x82after\xff end").unwrap();
                assert_eq!(
                    mode.read_agent_log_preview(&path.to_str().unwrap().into()),
                    JsString::from("before\u{fffd}after\u{fffd} end")
                );
            })
            .await;
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

    type RuntimeRequestObserver = Arc<dyn Fn(&CreateAgentSessionRuntimeOptions) + Send + Sync>;

    #[derive(Default)]
    struct TestModeSetup {
        runtime_request_observer: Option<RuntimeRequestObserver>,
        multi_agent_store: Option<Arc<MultiAgentStore>>,
        steer_multi_agent: Option<SteerMultiAgent>,
        model_registry: Option<ModelRegistry>,
        cwd: Option<String>,
        runtime_factory_failure: Option<Arc<Mutex<Option<CallbackError>>>>,
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
        let cwd = setup.cwd.clone().unwrap_or_else(|| temp.path().display().to_string());
        let faux = Arc::new(faux_provider(RegisterFauxProviderOptions {
            provider: Some("faux".to_owned()),
            api: Some("faux".to_owned()),
            models: Some(vec![
                FauxModelDefinition {
                    id: "faux-1".to_owned(),
                    name: Some("Faux Model".into()),
                    reasoning: Some(true),
                    input: None,
                    cost: None,
                    context_window: Some(100_000.0),
                    max_tokens: Some(4096.0),
                },
                FauxModelDefinition {
                    id: "faux-2".to_owned(),
                    name: Some("Faux Second Model".into()),
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
        let model_registry = setup
            .model_registry
            .clone()
            .unwrap_or_else(|| ModelRegistry::in_memory(AuthStorage::in_memory(credentials)).unwrap());
        scripted_registry::register_scripted_provider(&model_registry, Arc::clone(&faux.provider));
        for provider in extra_providers {
            scripted_registry::register_scripted_provider(&model_registry, provider);
        }

        let factory = {
            let model_registry = model_registry.clone();
            let faux = faux.clone();
            let loader = Arc::clone(&loader);
            let settings = Arc::new(Mutex::new(settings));
            let observer = setup.runtime_request_observer.clone();
            let failure = setup.runtime_factory_failure.clone();
            Arc::new(move |request: CreateAgentSessionRuntimeOptions| {
                if let Some(observer) = &observer {
                    observer(&request);
                }
                let model_registry = model_registry.clone();
                let faux = faux.clone();
                let loader = Arc::clone(&loader);
                let settings = Arc::clone(&settings);
                let initial_active_tool_names = initial_active_tool_names.clone();
                let failure = failure.clone();
                Box::pin(async move {
                    if let Some(failure) = failure.as_ref().and_then(|failure| failure.lock().unwrap().clone()) {
                        return Err(failure);
                    }
                    let model = faux.get_model(None).expect("faux model");
                    let stream_provider = Arc::clone(&faux.provider);
                    let messages = request.session_manager.build_session_context().messages;
                    let agent = Agent::new(AgentOptions {
                        initial_state: Some(AgentInitialState {
                            model: Some(model),
                            system_prompt: Some(JsString::default()),
                            thinking_level: Some(ThinkingLevel::Off),
                            tools: Some(Vec::new()),
                            messages: Some(messages),
                        }),
                        stream_fn: Some(Arc::new(
                            move |model: Model, context: Context, options: SimpleStreamOptions| {
                                let stream = stream_provider.stream_simple(model, context, options);
                                Box::pin(async move { stream.map_err(pi_agent_core::AgentError::from) })
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
                        model_registry: model_registry.clone(),
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
                        no_supervisor: None,
                        supervisor_decision_requester: None,
                        multi_agent_store: None,
                        multi_agent_agent_id: None,
                        multi_agent_parent_session_id: None,
                        multi_agent_requires_agent_id: false,
                        disable_runtime_coordination_inbound: false,
                        on_shared_channel_message_delivered: None,
                        extension_runner_ref: None,
                        session_start_event: request.session_start_event,
                    })
                    .map_err(|error| error.to_string())?;
                    Ok(CreateAgentSessionRuntimeResult {
                        session,
                        services: AgentSessionServices {
                            cwd: request.cwd,
                            agent_dir: request.agent_dir,
                            auth_storage: model_registry.auth_storage().clone(),
                            model_registry,
                            settings_manager,
                            resource_loader: loader,
                            diagnostics: Vec::new(),
                        },
                        diagnostics: Vec::new(),
                        model_fallback_message: None,
                    })
                })
                    as pi_ai::request::BoxFuture<'static, Result<CreateAgentSessionRuntimeResult, CallbackError>>
            }) as crate::core::agent_session_runtime::CreateAgentSessionRuntimeFactory
        };
        let session_manager = if persist_session {
            let session_dir = Path::new(&cwd).join("sessions").display().to_string();
            SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&session_dir)), None)
                .expect("persisted session manager")
        } else {
            SessionManager::in_memory(Some(&JsString::from(&cwd)), None).expect("session manager")
        };
        let runtime = create_agent_session_runtime(
            factory,
            &JsString::from(&cwd),
            &JsString::from(&cwd),
            session_manager,
            None,
            None,
        )
        .await
        .expect("runtime");
        let runtime = runtime;
        let terminal = Rc::new(RefCell::new(CaptureTerminal {
            rows: 24,
            columns: 80,
            writes: Vec::new(),
            titles: Vec::new(),
            events: Vec::new(),
            progress: Vec::new(),
            start_error: None,
            lifecycle_observer: None,
        }));
        let terminal_trait: Rc<RefCell<dyn Terminal>> = terminal.clone();
        let mode = InteractiveMode::new(
            runtime,
            InteractiveModeOptions {
                multi_agent_store: setup.multi_agent_store,
                steer_multi_agent: setup.steer_multi_agent,
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
        let theme_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent")).join("theme");
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
            mode.ui.flush_pending().unwrap();
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

    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_login.rs"
    ));

    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_login_callbacks.rs"
    ));

    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_login_manual.rs"
    ));

    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_login_prompt.rs"
    ));

    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_login_completion.rs"
    ));

    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_login_count.rs"
    ));

    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_login_defaults.rs"
    ));

    include!(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
        "/tests/support/interactive_login_warnings.rs"
    ));

    mod reload_fixture {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/interactive_reload.rs"
        ));
    }

    #[tokio::test]
    async fn reload_body_matches_frozen_loading_settings_status_and_raw_failure() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-reload-body-oracle.mts", "packages/coding-agent/src");
            for error in [None, Some(JsString::from_utf16(vec![102, 97, 105, 108, 32, 0xd800]))] {
                let expected = source.request(&serde_json::json!({"error":error.as_ref().map(JsString::units),"colors":crate::modes::interactive::theme::REQUIRED_COLORS}));
                assert_eq!(expected["ok"], true, "source: {expected}");
                let loader = reload_fixture::HeldReloadLoader::new(error.clone());
                let (mode, _terminal, _faux, _temp) = build_mode_with_loader(loader.clone(), None).await;
                mode.bind_extensions().await.unwrap();
                mode.default_editor.borrow_mut().set_padding_x(1);
                mode.default_editor.borrow_mut().set_autocomplete_max_visible(5);
                mode.ui.set_show_hardware_cursor(false);
                mode.ui.set_clear_on_shrink(false);
                mode.settings_mut(|settings| {
                    settings.set_editor_padding_x(7.0);
                    settings.set_autocomplete_max_visible(9.0);
                    settings.set_show_hardware_cursor(true);
                    settings.set_clear_on_shrink(true);
                    settings.set_hide_thinking_block(true);
                });
                let reload = tokio::task::spawn_local({
                    let mode = mode.clone();
                    async move { mode.handle_reload_command().await; }
                });
                tokio::time::timeout(Duration::from_secs(5), loader.entered.notified()).await.unwrap();
                let loading: Vec<_> = mode.editor_container.borrow_mut().render(80).into_iter()
                    .map(|line| pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line| !line.is_empty()).collect();
                loader.release.notify_one();
                tokio::time::timeout(Duration::from_secs(5), reload).await.unwrap().unwrap();
                let messages: Vec<_> = mode.chat_container.borrow_mut().render(200).into_iter()
                    .map(|line| pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line| !line.is_empty()).map(|line|line.units().to_vec()).collect();
                let restored = mode.editor_container.borrow().children().iter().any(|child| Rc::ptr_eq(child, &mode.editor_handle.borrow()));
                assert_eq!(serde_json::json!({"loading":loading,"padding":mode.default_editor.borrow().get_padding_x(),"visible":mode.default_editor.borrow().get_autocomplete_max_visible(),"hardware":mode.ui.get_show_hardware_cursor(),"clear":mode.ui.get_clear_on_shrink(),"messages":messages,"restored":restored}), expected["output"], "error={error:?}");
                mode.stop().unwrap();
            }
        }).await;
    }

    #[tokio::test]
    async fn diagnostic_helpers_match_frozen_prefix_spacing_and_exact_utf16_frames() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-diagnostics-oracle.mts", "packages/coding-agent/src");
            let steps = [
                ("error",JsString::from_utf16(vec![114,97,119,32,0xd800,32,0xd83d,0xde00])),
                ("warning",JsString::from_utf16(vec![119,97,114,110,32,0xdc00,10,108,105,110,101])),
                ("error",JsString::from("Error: nested")),
            ];
            for width in [0,1,5,25,120] {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS.iter().map(|name|((*name).into(),crate::modes::interactive::theme::ColorValue::Index(0.0))).collect();
                crate::modes::interactive::theme::set_theme_instance(Arc::new(Theme::new(&colors,&colors,crate::modes::interactive::theme::ColorMode::Color256,None,None).unwrap()));
                let expected = source.request(&serde_json::json!({"width":width,"colors":crate::modes::interactive::theme::REQUIRED_COLORS,"steps":steps.iter().map(|(kind,message)|serde_json::json!({"kind":kind,"message":message.units()})).collect::<Vec<_>>()}));
                assert_eq!(expected["ok"],true,"source: {expected}");
                let mut frames = Vec::new();
                for (kind,message) in &steps {
                    if *kind=="error" { mode.show_error(message); } else { mode.show_warning(message); }
                    frames.push(mode.chat_container.borrow_mut().render(width).into_iter().map(|line|line.units().to_vec()).collect::<Vec<_>>());
                }
                assert_eq!(serde_json::json!(frames),expected["output"],"width={width}");
                mode.stop().unwrap();
            }
        }).await;
    }

    #[tokio::test]
    async fn reload_restores_raw_transcript_before_session_start_without_replaying_history() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new().run_until(async {
            let snapshot = Arc::new(Mutex::new(None));
            let mode_id = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let extension_runtime = crate::core::extensions::create_extension_runtime();
            let factory: crate::core::extensions::ExtensionFactory = Arc::new({
                let snapshot = snapshot.clone();
                let mode_id = mode_id.clone();
                move |api| {
                    let snapshot = snapshot.clone();
                    let mode_id = mode_id.clone();
                    api.on_session_start(move |event: crate::core::extensions::SessionStartEvent, _: crate::core::extensions::ExtensionContext| -> crate::core::extensions::BoxFuture<'static,Result<Option<()>,HandlerError>> {
                        let snapshot = snapshot.clone();
                        let mode_id = mode_id.clone();
                        Box::pin(async move {
                            if event.reason == crate::core::extensions::SessionStartReason::Reload {
                                with_mode(mode_id.load(Ordering::SeqCst),|mode| {
                                    let lines: Vec<_> = mode.chat_container.borrow_mut().render(120).into_iter().map(|line|pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line|!line.is_empty()).map(|line|line.units().to_vec()).collect();
                                    *snapshot.lock().unwrap() = Some(serde_json::json!({"lines":lines,"hidden":mode.hide_thinking_block.get(),"history":mode.default_editor.borrow().get_history().iter().map(|line|line.units().to_vec()).collect::<Vec<_>>()}));
                                    mode.chat_container.borrow_mut().add_child(component(Text::new("extension reload message",0,0)));
                                }).expect("session_start runs on live UI owner");
                            }
                            Ok(None)
                        })
                    }).unwrap();
                    Box::pin(async { Ok(()) })
                }
            });
            let extension = crate::core::extensions::load_extension_from_factory(factory,&"/tmp".into(),Arc::new(crate::core::event_bus::EventBusController::new()),extension_runtime.clone(),&"/tmp/reload-transcript".into()).await.unwrap();
            let loader = Arc::new(StaticResourceLoader { extensions: Some(crate::core::extensions::LoadExtensionsResult { extensions:vec![extension],errors:Vec::new(),runtime:extension_runtime }),..Default::default() });
            let (mode, _terminal, _faux, _temp) = build_mode_with_loader(loader,None).await;
            mode_id.store(mode.ui_id.get(),Ordering::SeqCst);
            mode.settings_mut(|settings|settings.set_hide_thinking_block(true));
            mode.default_editor.borrow_mut().add_to_history_without_persistence(&"keep history".into());
            let text = JsString::from_utf16(vec![114,101,108,111,97,100,32,0xd800]);
            mode.session().session_manager().append_message(AgentMessage::User(pi_ai::types::UserMessage {
                role: pi_ai::types::UserRole::Tag, content: pi_ai::types::UserContent::Text(text.clone()), input_source:None, provider_native:None, timestamp:1000.0,
            })).unwrap();
            mode.chat_container.borrow_mut().add_child(component(Text::new("stale transcript",0,0)));
            let mut source = oracle::TsOracle::spawn("interactive-reload-body-oracle.mts", "packages/coding-agent/src");
            let expected = source.request(&serde_json::json!({"preStart":true,"text":text.units(),"cwd":mode.cwd(),"colors":crate::modes::interactive::theme::REQUIRED_COLORS}));
            assert_eq!(expected["ok"],true,"source: {expected}");
            mode.bind_extensions().await.unwrap();
            mode.handle_reload_command().await;
            assert_eq!(snapshot.lock().unwrap().as_ref(),Some(&expected["output"]["preStart"]));
            let messages: Vec<_> = mode.chat_container.borrow_mut().render(200).into_iter().map(|line|pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line|!line.is_empty()).map(|line|line.units().to_vec()).collect();
            assert_eq!(serde_json::json!(messages),expected["output"]["messages"],"fallback restore must not erase session_start output");
            mode.stop().unwrap();
        }).await;
    }

    #[tokio::test]
    async fn reload_expands_session_start_header_and_catches_its_raw_failure_like_frozen() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        struct ReloadHeader {
            expanded: Arc<AtomicBool>,
            plain: bool,
        }
        impl ExtensionUiComponent for ReloadHeader {
            fn render(&self, width: usize) -> Vec<JsString> {
                let text = if self.plain {
                    "header-plain"
                } else if self.expanded.load(Ordering::SeqCst) {
                    "header-expanded"
                } else {
                    "header-collapsed"
                };
                Text::new(text, 0, 0).render(width)
            }
        }
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-reload-body-oracle.mts", "packages/coding-agent/src");
            for stage in ["expand","plain","throw"] {
                let raw = JsString::from_utf16(vec![101,120,112,97,110,100,32,0xd800]);
                let expected = source.request(&serde_json::json!({"header":stage,"headerError":raw.units(),"colors":crate::modes::interactive::theme::REQUIRED_COLORS}));
                assert_eq!(expected["ok"],true,"source: {expected}");
                let expanded = Arc::new(AtomicBool::new(false));
                let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let extension_runtime = crate::core::extensions::create_extension_runtime();
                let factory: crate::core::extensions::ExtensionFactory = Arc::new({
                    let expanded = expanded.clone();
                    move |api| {
                        let expanded = expanded.clone();
                        let calls = calls.clone();
                        let raw = raw.clone();
                        api.on_session_start(move |event: crate::core::extensions::SessionStartEvent, ctx: crate::core::extensions::ExtensionContext| -> crate::core::extensions::BoxFuture<'static,Result<Option<()>,HandlerError>> {
                            let expanded = expanded.clone();
                            let calls = calls.clone();
                            let raw = raw.clone();
                            Box::pin(async move {
                                if event.reason != crate::core::extensions::SessionStartReason::Reload { return Ok(None); }
                                let state = expanded.clone();
                                ctx.ui()?.set_header(Some(Arc::new(move |_| {
                                    let expanded = state.clone();
                                    let calls = calls.clone();
                                    let raw = raw.clone();
                                    let set_expanded: Option<HeaderExpansionHandler> = if stage == "plain" { None } else {
                                        Some(Rc::new({
                                            let expanded = expanded.clone();
                                            move |value| {
                                                let index = calls.fetch_add(1,Ordering::SeqCst);
                                                if stage == "throw" && index == 1 { return Err(CallbackError::new(raw.clone())); }
                                                expanded.store(value,Ordering::SeqCst);
                                                Ok(())
                                            }
                                        }))
                                    };
                                    Ok(crate::core::extensions::types::ExtensionUiHeader { component: Box::new(ReloadHeader { expanded, plain: stage=="plain" }), set_expanded })
                                })))?;
                                expanded.store(false,Ordering::SeqCst);
                                Ok(None)
                            })
                        }).unwrap();
                        Box::pin(async { Ok(()) })
                    }
                });
                let extension = crate::core::extensions::load_extension_from_factory(factory,&"/tmp".into(),Arc::new(crate::core::event_bus::EventBusController::new()),extension_runtime.clone(),&"/tmp/reload-header".into()).await.unwrap();
                let loader = Arc::new(StaticResourceLoader { extensions: Some(crate::core::extensions::LoadExtensionsResult { extensions:vec![extension],errors:Vec::new(),runtime:extension_runtime }),..Default::default() });
                let (mode, _terminal, _faux, _temp) = build_mode_with_loader(loader,None).await;
                mode.settings_mut(|settings|settings.set_quiet_startup(true));
                mode.tool_output_expanded.set(true);
                mode.build_header();
                mode.bind_extensions().await.unwrap();
                mode.handle_reload_command().await;
                let header: Vec<_> = mode.header_container.borrow_mut().render(80).into_iter().map(|line|pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line|!line.is_empty()).collect();
                let messages: Vec<_> = mode.chat_container.borrow_mut().render(200).into_iter().map(|line|pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line|!line.is_empty()).map(|line|line.units().to_vec()).collect();
                assert_eq!(serde_json::json!(header),expected["output"]["header"],"stage={stage}");
                assert_eq!(serde_json::json!(messages),expected["output"]["messages"],"stage={stage}");
                mode.stop().unwrap();
            }
        }).await;
    }

    #[tokio::test]
    async fn header_owner_and_worker_preserve_frozen_factory_and_expansion_errors() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-header-error-oracle.mts", "packages/coding-agent/src");
            for worker in [false,true] {
                for stage in ["factory","expand"] {
                    let (mode, _terminal, _faux, _temp) = build_mode().await;
                    mode.settings_mut(|settings| settings.set_quiet_startup(true));
                    mode.tool_output_expanded.set(true);
                    mode.build_header();
                    let raw = JsString::from_utf16(vec![104,101,97,100,101,114,32,0xd800]);
                    let expected = source.request(&serde_json::json!({"stage":stage,"error":raw.units()}));
                    assert_eq!(expected["ok"],true,"source: {expected}");
                    let native = NativeNodeError::from_constructor(NativeErrorClass::TypeError,raw.clone()).with_property("code",JsValue::String("HEADER_FAILURE".into()));
                    let failure = CallbackError::from_error(raw,native);
                    let owner_thread = std::thread::current().id();
                    let factory: ExtensionUiHeaderFactory = Arc::new({
                        let failure = failure.clone();
                        move |_| {
                            assert_eq!(std::thread::current().id(),owner_thread,"factory must run synchronously on UI owner");
                            if stage == "factory" { return Err(failure.clone()); }
                            let failure = failure.clone();
                            Ok(crate::core::extensions::types::ExtensionUiHeader {
                                component: Box::new(StaticWidget),
                                set_expanded: Some(Rc::new(move |_| {
                                    assert_eq!(std::thread::current().id(),owner_thread,"expansion must run on UI owner");
                                    Err(failure.clone())
                                })),
                            })
                        }
                    });
                    let ui = mode.ui_context();
                    let result = if worker {
                        let worker = std::thread::spawn(move || ui.set_header(Some(factory)));
                        tokio::time::timeout(Duration::from_secs(5),async {
                            while !worker.is_finished() { mode.handle_ui_requests().await; tokio::time::sleep(Duration::from_millis(1)).await; }
                        }).await.expect("worker receives owner result");
                        worker.join().unwrap()
                    } else { ui.set_header(Some(factory)) };
                    let error = result.expect_err("header failure must propagate to its caller");
                    let value = match error.thrown_value() {
                        pi_js_compat::abort::AbortReason::Error(value) => value,
                        reason => panic!("unexpected header rejection: {reason:?}"),
                    };
                    let frame: Vec<_> = mode.header_container.borrow_mut().render(80).into_iter().map(|line|line.units().to_vec()).collect();
                    assert_eq!(serde_json::json!({"message":value.message.units(),"name":value.name,"code":value.value().get("code"),"typeError":value.is_instance_of(NativeErrorClass::TypeError),"sameError":error==failure,"frame":frame}),expected["output"],"worker={worker},stage={stage}");
                    mode.stop().unwrap();
                }
            }
        }).await;
    }

    #[test]
    fn header_startup_and_registration_frames_match_frozen_expandability() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "modes::interactive::interactive_mode::ui_tests::header_render_process_child",
                "--nocapture",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "header child failed:\n{}\n{}",
            String::from_utf8(output.stdout).unwrap(),
            String::from_utf8(output.stderr).unwrap()
        );
    }

    #[tokio::test]
    #[ignore = "invoked by header_startup_and_registration_frames_match_frozen_expandability"]
    async fn header_render_process_child() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        struct HeaderFixture {
            expanded: Rc<Cell<bool>>,
            expandable: bool,
        }
        impl ExtensionUiComponent for HeaderFixture {
            fn render(&self, width: usize) -> Vec<JsString> {
                let text = if self.expandable {
                    if self.expanded.get() {
                        "custom-expanded"
                    } else {
                        "custom-collapsed"
                    }
                } else {
                    "custom"
                };
                Text::new(text, 0, 0).render(width)
            }
        }
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-header-oracle.mts", "packages/coding-agent/src");
            for (expanded, quiet, verbose) in [(false,false,false),(true,false,false),(false,true,false),(false,true,true)] {
                let (initial, terminal, _faux, _temp) = build_mode().await;
                let terminal: Rc<RefCell<dyn Terminal>> = terminal;
                let mode = InteractiveMode::new(initial.runtime.clone(), InteractiveModeOptions { verbose, ..Default::default() }, Some(terminal)).await.unwrap();
                mode.settings_mut(|settings| settings.set_quiet_startup(quiet));
                mode.tool_output_expanded.set(expanded);
                let bindings = if expanded { vec![("app.clear".into(),vec!["ctrl+x".into()]),("app.tools.expand".into(),vec!["ctrl+y".into()])] } else { Vec::new() };
                KeybindingsManager::new(bindings, None).install();
                let request_bindings = if expanded { serde_json::json!({"app.clear":["ctrl+x"],"app.tools.expand":["ctrl+y"]}) } else { serde_json::json!({}) };
                let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS.iter().map(|name|((*name).into(),crate::modes::interactive::theme::ColorValue::Index(0.0))).collect();
                crate::modes::interactive::theme::set_theme_instance(Arc::new(Theme::new(&colors,&colors,crate::modes::interactive::theme::ColorMode::Color256,None,None).unwrap()));
                let expected = source.request(&serde_json::json!({"expanded":expanded,"quiet":quiet,"verbose":verbose,"version":mode.version,"bindings":request_bindings,"width":100,"colors":crate::modes::interactive::theme::REQUIRED_COLORS}));
                assert_eq!(expected["ok"],true,"source: {expected}");
                mode.build_header();
                mode.loaded_resources_container.borrow_mut().add_child(component(ExpandableText::new(|| "resource-collapsed".into(), || "resource-expanded".into(), false, 0, 0)));
                mode.chat_container.borrow_mut().add_child(component(ExpandableText::new(|| "chat-collapsed".into(), || "chat-expanded".into(), false, 0, 0)));
                mode.chat_container.borrow_mut().add_child(component(Text::new("plain child", 0, 0)));
                let capture = || {
                    let render = |container: &mut dyn Component| container.render(100).into_iter().map(|line|line.units().to_vec()).collect::<Vec<_>>();
                    serde_json::json!({"header":render(&mut *mode.header_container.borrow_mut()),"resources":render(&mut *mode.loaded_resources_container.borrow_mut()),"chat":render(&mut *mode.chat_container.borrow_mut())})
                };
                let mut frames = Vec::new();
                let mut capture_expansion = || {
                    frames.push(capture());
                    for expanded in [true, false] {
                        mode.set_tools_expanded(expanded).unwrap();
                        frames.push(capture());
                    }
                };
                capture_expansion();
                for expandable in [true,false] {
                    mode.set_extension_header(Some(Arc::new(move |_| {
                        let expanded = Rc::new(Cell::new(false));
                        let set_expanded: Option<HeaderExpansionHandler> = if expandable {
                            let expanded = expanded.clone();
                            Some(Rc::new(move |value| { expanded.set(value); Ok(()) }))
                        } else { None };
                        Ok(crate::core::extensions::types::ExtensionUiHeader {
                            component: Box::new(HeaderFixture { expanded, expandable }), set_expanded,
                        })
                    }))).unwrap();
                    capture_expansion();
                }
                mode.set_extension_header(None).unwrap();
                capture_expansion();
                assert_eq!(mode.settings(|settings| settings.get_output_pad()), 1, "expansion does not change output padding");
                assert_eq!(serde_json::json!(frames),expected["output"],"expanded={expanded},quiet={quiet},verbose={verbose}");
                mode.stop().unwrap();
            }
        }).await;
    }

    #[tokio::test]
    async fn extension_shortcuts_bind_and_reload_match_frozen_input_context_and_errors() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-shortcuts-oracle.mts", "packages/coding-agent/src");
            for error in [None, Some(JsString::from_utf16(vec![101,114,114,32,0xd800]))] {
                let contexts = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
                let extension_runtime = crate::core::extensions::create_extension_runtime();
                let text = JsString::from_utf16(vec![65,0xd800,66]);
                let factory: crate::core::extensions::ExtensionFactory = Arc::new({
                    let error = error.clone();
                    let contexts = contexts.clone();
                    let text = text.clone();
                    move |api| {
                        let error = error.clone();
                        let contexts = contexts.clone();
                        let text = text.clone();
                        api.register_shortcut("alt+q".into(), None, Arc::new(move |ctx| {
                            let error = error.clone();
                            let contexts = contexts.clone();
                            let text = text.clone();
                            Box::pin(async move {
                                contexts.lock().unwrap().push(serde_json::json!({"cwd":ctx.cwd()?,"hasUI":ctx.has_ui()?,"idle":ctx.is_idle()?,"trusted":ctx.is_project_trusted()?}));
                                if let Some(error) = error { return Err(HandlerError::new(error)); }
                                ctx.ui()?.set_editor_text(&text)?;
                                Ok(())
                            })
                        })).unwrap();
                        Box::pin(async { Ok(()) })
                    }
                });
                let extension = crate::core::extensions::load_extension_from_factory(factory, &"/tmp".into(), Arc::new(crate::core::event_bus::EventBusController::new()), extension_runtime.clone(), &"/tmp/shortcut".into()).await.unwrap();
                let loader = Arc::new(StaticResourceLoader { extensions: Some(crate::core::extensions::LoadExtensionsResult { extensions: vec![extension], errors: Vec::new(), runtime: extension_runtime }), ..Default::default() });
                let (mode, _terminal, _faux, _temp) = build_mode_with_loader(loader, None).await;
                let expected = source.request(&serde_json::json!({"cwd":mode.cwd(),"text":text.units(),"error":error.as_ref().map(JsString::units)}));
                assert_eq!(expected["ok"],true,"source: {expected}");
                mode.bind_extensions().await.unwrap();
                for reload in [false, true] {
                    if reload { mode.handle_reload_command().await; }
                    mode.chat_container.borrow_mut().clear();
                    contexts.lock().unwrap().clear();
                    mode.editor().borrow_mut().set_text(&JsString::default()).unwrap();
                    mode.default_editor.borrow_mut().handle_input(&"\u{1b}q".into());
                    for _ in 0..20 { mode.handle_ui_requests().await; tokio::time::sleep(Duration::from_millis(1)).await; }
                    let errors: Vec<_> = mode.chat_container.borrow_mut().render(200).into_iter().map(|line|pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line|!line.is_empty()).map(|line|line.units().to_vec()).collect();
                    assert_eq!(serde_json::json!({"contexts":*contexts.lock().unwrap(),"editor":mode.editor().borrow().get_text().unwrap().units(),"errors":errors}),expected["output"],"reload={reload},error={error:?}");
                }
                mode.stop().unwrap();
            }
        }).await;
    }

    #[tokio::test]
    async fn reload_implicit_trust_matches_frozen_persistence_and_status_branches() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        use crate::core::trust_manager::ProjectTrustDecision;
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-reload-body-oracle.mts", "packages/coding-agent/src");
            for (resources, trusted, auto_matches, prior, locked) in [
                (true, true, true, None, false), (true, true, true, Some(true), false),
                (true, true, true, Some(false), false), (true, true, false, None, false),
                (true, false, true, None, false), (false, true, true, None, false),
                (true, true, true, None, true),
            ] {
                let (initial, terminal, _faux, temp) = build_mode().await;
                let cwd = initial.cwd();
                let source_cwd = temp.path().join("source-cwd");
                let source_agent = temp.path().join("source-agent");
                std::fs::create_dir(&source_cwd).unwrap();
                std::fs::create_dir(&source_agent).unwrap();
                if resources {
                    std::fs::create_dir_all(temp.path().join(".pi/extensions")).unwrap();
                    std::fs::create_dir_all(source_cwd.join(".pi/extensions")).unwrap();
                }
                if let Some(prior) = prior {
                    let decision = if prior { ProjectTrustDecision::Trusted } else { ProjectTrustDecision::Untrusted };
                    ProjectTrustStore::new(&cwd).unwrap().set(&cwd, decision).unwrap();
                    ProjectTrustStore::new(&js_path(&source_agent)).unwrap().set(&js_path(&source_cwd), decision).unwrap();
                }
                if locked {
                    std::fs::create_dir(temp.path().join("trust.json.lock")).unwrap();
                    std::fs::create_dir(source_agent.join("trust.json.lock")).unwrap();
                }
                let expected = source.request(&serde_json::json!({"trust":true,"trustLocked":locked,"projectTrusted":trusted,"cwd":source_cwd,"agentDir":source_agent,"autoCwd":if auto_matches { js_path(&source_cwd) } else { "/not-fixture".into() },"colors":crate::modes::interactive::theme::REQUIRED_COLORS}));
                assert_eq!(expected["ok"], true, "source: {expected}");
                let terminal_trait: Rc<RefCell<dyn Terminal>> = terminal;
                let mode = InteractiveMode::new(initial.runtime.clone(), InteractiveModeOptions {
                    auto_trust_on_reload_cwd: Some(if auto_matches { cwd.clone() } else { "/not-fixture".into() }),
                    ..Default::default()
                }, Some(terminal_trait)).await.unwrap();
                mode.settings_mut(|settings| settings.set_project_trusted(trusted));
                mode.bind_extensions().await.unwrap();
                mode.handle_reload_command().await;
                if locked { std::fs::remove_dir(temp.path().join("trust.json.lock")).unwrap(); }
                let decision = match ProjectTrustStore::new(&cwd).unwrap().get(&cwd).unwrap() {
                    ProjectTrustDecision::Unknown => serde_json::Value::Null,
                    ProjectTrustDecision::Trusted => serde_json::json!(true),
                    ProjectTrustDecision::Untrusted => serde_json::json!(false),
                };
                let messages: Vec<_> = mode.chat_container.borrow_mut().render(200).into_iter()
                    .map(|line|pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line|!line.is_empty()).map(|line|line.units().to_vec()).collect();
                assert_eq!(decision, expected["output"]["decision"], "resources={resources},trusted={trusted},auto={auto_matches},prior={prior:?},locked={locked}");
                assert_eq!(mode.auto_trust_on_reload_cwd.borrow().is_some(),expected["output"]["pendingTrust"].as_bool().unwrap());
                assert_eq!(serde_json::json!(messages), expected["output"]["messages"]);
                mode.stop().unwrap();
            }
        }).await;
    }

    #[tokio::test]
    async fn reload_while_compacting_matches_frozen_warning_without_reloading_resources() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new()
            .run_until(async {
                let mut source =
                    oracle::TsOracle::spawn("interactive-reload-busy-oracle.mts", "packages/coding-agent/src");
                let expected = source.request(&serde_json::json!({"streaming":false,"compacting":true}));
                assert_eq!(expected["ok"], true, "source: {expected}");
                let entered = Arc::new(tokio::sync::Notify::new());
                let release = Arc::new(tokio::sync::Notify::new());
                let extension_runtime = crate::core::extensions::create_extension_runtime();
                let factory: crate::core::extensions::ExtensionFactory = Arc::new({
                    let entered = entered.clone();
                    let release = release.clone();
                    move |api| {
                        let entered = entered.clone();
                        let release = release.clone();
                        api.on_session_compaction_source(
                            move |_: crate::core::extensions::SessionCompactionSourceEvent,
                                  _: crate::core::extensions::ExtensionContext|
                                  -> crate::core::extensions::BoxFuture<
                                'static,
                                Result<Option<crate::core::extensions::SessionCompactionSourceResult>, HandlerError>,
                            > {
                                let entered = entered.clone();
                                let release = release.clone();
                                Box::pin(async move {
                                    entered.notify_one();
                                    release.notified().await;
                                    Ok(None)
                                })
                            },
                        )
                        .unwrap();
                        Box::pin(async { Ok(()) })
                    }
                });
                let extension = crate::core::extensions::load_extension_from_factory(
                    factory,
                    &"/tmp".into(),
                    Arc::new(crate::core::event_bus::EventBusController::new()),
                    extension_runtime.clone(),
                    &"/tmp/held-compaction".into(),
                )
                .await
                .unwrap();
                let loader = Arc::new(StaticResourceLoader {
                    extensions: Some(crate::core::extensions::LoadExtensionsResult {
                        extensions: vec![extension],
                        errors: Vec::new(),
                        runtime: extension_runtime,
                    }),
                    ..Default::default()
                });
                let (mode, _terminal, _faux, _temp) = build_mode_with_loader(loader, None).await;
                let compact = tokio::task::spawn_local({
                    let session = mode.session();
                    async move { session.compact(None).await }
                });
                tokio::time::timeout(Duration::from_secs(5), entered.notified())
                    .await
                    .unwrap();
                assert!(mode.session().is_compacting());
                assert!(!mode.session().is_streaming());
                mode.handle_reload_command().await;
                release.notify_one();
                let _result = compact.await.unwrap();
                let lines: Vec<_> = mode
                    .chat_container
                    .borrow_mut()
                    .render(120)
                    .into_iter()
                    .map(|line| pi_tui::utils::strip_terminal_sequences(&line).trim())
                    .filter(|line| !line.is_empty())
                    .collect();
                assert_eq!(serde_json::json!(lines), expected["output"]);
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reload_while_streaming_matches_frozen_warning() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new()
            .run_until(async {
                let mut source =
                    oracle::TsOracle::spawn("interactive-reload-busy-oracle.mts", "packages/coding-agent/src");
                let expected = source.request(&serde_json::json!({"streaming":true,"compacting":false}));
                assert_eq!(expected["ok"], true, "{expected}");
                let (mode, _terminal, faux, _temp) = build_mode().await;
                let gate = Arc::new(tokio::sync::Notify::new());
                let entered = Arc::new(tokio::sync::Notify::new());
                let stream_gate = gate.clone();
                let stream_entered = entered.clone();
                faux.set_responses(vec![pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
                    move |_, _, _, _| {
                        let gate = stream_gate.clone();
                        let entered = stream_entered.clone();
                        Box::pin(async move {
                            entered.notify_one();
                            gate.notified().await;
                            Ok(pi_ai::providers::faux::faux_assistant_message(
                                "done",
                                Default::default(),
                            ))
                        })
                    },
                ))]);
                let session = mode.session();
                let task =
                    tokio::task::spawn_local(async move { session.prompt("start", PromptOptions::default()).await });
                entered.notified().await;
                assert!(mode.session().is_streaming());
                mode.handle_reload_command().await;
                gate.notify_one();
                task.await.unwrap().unwrap();
                let lines = mode
                    .chat_container
                    .borrow_mut()
                    .render(100)
                    .into_iter()
                    .map(|line| pi_tui::utils::strip_terminal_sequences(&line).trim())
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>();
                assert_eq!(serde_json::json!(lines), expected["output"]);
                mode.stop().unwrap();
            })
            .await;
    }

    async fn wait_for_fixture_model_selector(mode: &InteractiveMode) {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let frame = JsString::join(mode.editor_container.borrow_mut().render(100), "\n");
            let has_selector = frame.contains("Only showing models from configured providers");
            let has_models = frame.contains("faux-1") || frame.contains("faux-2");
            if !has_selector || has_models || frame.contains("No matching models") {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "selector did not publish fixture models: {frame:?}"
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn viewed_child_cycle_routes_live_targets_and_preserves_frozen_errors() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        use crate::bundled_extensions::agents_core::session_factories::AgentSessionMutationTarget;
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-viewed-target-oracle.mts", "packages/coding-agent/src");
            for (view, target, scope) in [(true, "missing", 0), (true, "core", 0), (true, "interactive", 0), (true, "interactive", 1), (true, "interactive", 2), (false, "missing", 0)] {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let (child_mode, _child_terminal, _child_faux, _child_temp) = build_mode_with_persisted_session().await;
                let child_session = child_mode.session();
                let models = ["faux-1", "faux-2"].map(|id| mode.session().model_registry().find("faux", id).unwrap());
                child_session.set_scoped_models(models.iter().take(scope).map(|model| crate::core::model_resolver::ScopedModel {model: model.clone(), thinking_level: None}).collect());
                let child_id = JsString::from_utf16(vec![0xD800, 0x61]);
                let expected = source.request(&serde_json::json!({"models":models,"child":view.then(|| child_id.units()), "target":target,"scope":scope}));
                assert_eq!(expected["ok"], true, "{expected}");
                if view {
                    *mode.child_view.borrow_mut() = Some(ChildAgentView { transcript_path: None, agent_id: child_id, session_manager: None, watcher: None, watch_path: None, reload_deadline: None, watch_retry_deadline: None, reload_retries: 0 });
                }
                let child_target = Arc::new(AgentSessionMutationTarget::new(child_session.clone()));
                if target == "core" {
                    bind_interactive_mode_session_mutation_target_resolver(&mode, Arc::new(move || Ok(Some(ResolvedMutationTarget::Core(child_target.clone())))));
                } else if target == "interactive" {
                    bind_interactive_mode_session_mutation_target_resolver(&mode, Arc::new(move || Ok(Some(ResolvedMutationTarget::Interactive(child_target.clone())))));
                }
                mode.cycle_model(CycleDirection::Forward).await;
                let lines = mode.chat_container.borrow_mut().render(100).into_iter().map(|line| pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line| !line.is_empty()).collect::<Vec<_>>();
                let errors = lines.iter().filter(|line| line.starts_with("Error: ")).map(|line| line.units().to_vec()).collect::<Vec<_>>();
                let statuses = lines.iter().filter(|line| !line.starts_with("Error: ")).collect::<Vec<_>>();
                let actual = serde_json::json!({"main":mode.session().model().id,"child":child_session.model().id,"errors":errors,"statuses":statuses});
                assert_eq!(actual, expected["output"], "view={view}, target={target}, scope={scope}");
                mode.stop().unwrap();
            }
        }).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn viewed_child_candidates_preserve_target_errors_before_registry_catch() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        use crate::bundled_extensions::agents_core::session_factories::AgentSessionMutationTarget;
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-viewed-candidates-oracle.mts", "packages/coding-agent/src");
            for (view, kind, scoped) in [(false,"missing",false),(true,"missing",false),(true,"core",false),(true,"throw",false),(true,"interactive",false),(true,"interactive",true)] {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let models = ["faux-1", "faux-2"].map(|id| mode.session().model_registry().find("faux", id).unwrap());
                let message = JsString::from_utf16(vec![0xD800, 0x61]);
                let expected = source.request(&serde_json::json!({"models":models,"view":view,"target":kind,"scoped":scoped,"message":message.units()}));
                assert_eq!(expected["ok"], true, "{expected}");
                if scoped { mode.session().set_scoped_models(vec![crate::core::model_resolver::ScopedModel {model:models[1].clone(),thinking_level:None}]); }
                if view { *mode.child_view.borrow_mut() = Some(ChildAgentView {transcript_path:None,agent_id:"child".into(),session_manager:None,watcher:None,watch_path:None,reload_deadline:None,watch_retry_deadline:None,reload_retries:0}); }
                let target = Arc::new(AgentSessionMutationTarget::new(mode.session()));
                if kind == "throw" {
                    bind_interactive_mode_session_mutation_target_resolver(&mode, Arc::new(move || Err(pi_js_compat::node_error::NativeNodeError::new("TypeError", message.clone()).with_property("code",JsValue::String("E_TARGET".into())).into())));
                } else if kind == "core" {
                    bind_interactive_mode_session_mutation_target_resolver(&mode, Arc::new(move || Ok(Some(ResolvedMutationTarget::Core(target.clone())))));
                } else if kind == "interactive" {
                    bind_interactive_mode_session_mutation_target_resolver(&mode, Arc::new(move || Ok(Some(ResolvedMutationTarget::Interactive(target.clone())))));
                }
                let actual = match mode.get_model_candidates() {
                    Ok(models) => serde_json::json!({"models":models.iter().map(|model| model.id.clone()).collect::<Vec<_>>()}),
                    Err(error) => {
                        let code = match error.thrown_value() {
                            pi_js_compat::abort::AbortReason::Error(native) => native.value().get("code").cloned(),
                            _ => None,
                        };
                        serde_json::json!({"error":{"name":error.name,"message":error.message.units(),"code":code}})
                    }
                };
                assert_eq!(actual, expected["output"], "view={view}, target={kind}, scoped={scoped}");
                mode.stop().unwrap();
            }
        }).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn viewed_child_selector_resolves_again_when_selection_target_changes() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        use crate::bundled_extensions::agents_core::session_factories::AgentSessionMutationTarget;
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-model-command-oracle.mts", "packages/coding-agent/src");
            let (mode, terminal, _faux, _temp) = build_mode().await;
            let _events = start(&mode).await;
            let (first, _first_terminal, _first_faux, _first_temp) = build_mode().await;
            let (second, _second_terminal, _second_faux, _second_temp) = build_mode().await;
            let selected = Arc::new(std::sync::Mutex::new(first.session()));
            let selected_for_resolver = selected.clone();
            bind_interactive_mode_session_mutation_target_resolver(&mode, Arc::new(move || Ok(Some(ResolvedMutationTarget::Interactive(Arc::new(AgentSessionMutationTarget::new(selected_for_resolver.lock().unwrap().clone())))))));
            *mode.child_view.borrow_mut() = Some(ChildAgentView {transcript_path: None, agent_id: "child".into(), session_manager: None, watcher: None, watch_path: None, reload_deadline: None, watch_retry_deadline: None, reload_retries: 0});
            let models = ["faux-1", "faux-2"].map(|id| mode.session().model_registry().find("faux", id).unwrap());
            let expected = source.request(&serde_json::json!({"models":models,"search":null,"keys":["\x1b[B","\r"],"rebind":true,"colors":crate::modes::interactive::theme::REQUIRED_COLORS}));
            assert_eq!(expected["ok"], true, "{expected}");
            mode.handle_model_command(None).await.unwrap();
            wait_for_fixture_model_selector(&mode).await;
            *selected.lock().unwrap() = second.session();
            for key in ["\x1b[B", "\r"] {
                terminal.borrow_mut().send_input(key);
                mode.ui.process_events();
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            assert_eq!(first.session().model().id, expected["output"]["model"].as_str().unwrap());
            assert_eq!(second.session().model().id, expected["output"]["rebound"].as_str().unwrap());
            assert_eq!(mode.session().model().id, "faux-1");
            mode.stop().unwrap();
        }).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn viewed_child_footer_prefers_persisted_then_published_then_transcript_model() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-child-footer-oracle.mts", "packages/coding-agent/src");
            for (persisted, published) in [(false, true), (true, true), (false, false)] {
                let store = Arc::new(MultiAgentStore::new(Default::default()));
                let (mode, _terminal, _faux, temp) = build_mode_with_store(store.clone()).await;
                let cwd = js_path(temp.path());
                let dir = js_path(&temp.path().join("child"));
                let db = js_path(&temp.path().join("child-control.db"));
                let mut manager = SessionManager::create(&cwd, Some(&dir), None).unwrap();
                manager.set_metadata_control_db_path(Some(&db), Default::default()).unwrap();
                manager.append_model_change(&"faux".into(), &"faux-1".into()).unwrap();
                if persisted {
                    manager.set_session_model(&"faux".into(), &"faux-2".into()).unwrap();
                    manager.set_session_thinking_level(&"high".into()).unwrap();
                }
                let agent_model = published.then(|| crate::core::multi_agent_store::AgentModel {provider_id: "faux".into(),model_id: "faux-2".into(),thinking_level: Some("low".into())});
                let child_id = spawn_agent(&store, "child", None, None, agent_model.clone());
                let persisted_settings = manager.read_persisted_session_settings().unwrap();
                let persisted_wire = persisted_settings.as_ref().map(|settings| serde_json::json!({"model":settings.model.as_ref().map(|model| serde_json::json!({"provider":model.provider,"modelId":model.model_id})),"thinkingLevel":settings.thinking_level}));
                let models = ["faux-1", "faux-2"].map(|id| mode.session().model_registry().find("faux", id).unwrap());
                let expected = source.request(&serde_json::json!({"cwd":cwd,"models":models,"transcript":{"provider":"faux","modelId":"faux-1"},"persisted":persisted_wire,"published":agent_model}));
                assert_eq!(expected["ok"], true, "{expected}");
                *mode.child_view.borrow_mut() = Some(ChildAgentView {transcript_path: None, agent_id: child_id, session_manager: Some(Arc::new(std::sync::Mutex::new(manager))), watcher: None, watch_path: None, reload_deadline: None, watch_retry_deadline: None, reload_retries: 0});
                assert!(mode.render_selected_agent_view().unwrap());
                let footer = mode.footer_data_provider.get_session_override().unwrap();
                let actual = serde_json::json!({"model":footer.model.map(|model| model.id),"thinking":footer.thinking_level,"contextWindow":footer.context_usage.map(|usage| usage.context_window)});
                assert_eq!(actual["model"], expected["output"]["model"], "persisted={persisted}, published={published}");
                assert_eq!(actual["thinking"], expected["output"]["thinking"]);
                assert_eq!(actual["contextWindow"].as_f64(), expected["output"]["contextWindow"].as_f64());
                mode.stop().unwrap();
            }
        }).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn viewed_child_model_command_mutates_child_not_main() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        use crate::bundled_extensions::agents_core::session_factories::AgentSessionMutationTarget;
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-model-command-oracle.mts", "packages/coding-agent/src");
            for (search, keys) in [(Some("faux-2"), vec![]), (Some("2"), vec!["\r"]), (None, vec!["\x1b[B", "\r"])] {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let (child_mode, _child_terminal, _child_faux, _child_temp) = build_mode_with_persisted_session().await;
                let child_session = child_mode.session();
                let target = Arc::new(AgentSessionMutationTarget::new(child_session.clone()));
                bind_interactive_mode_session_mutation_target_resolver(&mode, Arc::new(move || Ok(Some(ResolvedMutationTarget::Interactive(target.clone())))));
                *mode.child_view.borrow_mut() = Some(ChildAgentView {transcript_path: None, agent_id: "child".into(), session_manager: Some(child_session.session_manager_handle()), watcher: None, watch_path: None, reload_deadline: None, watch_retry_deadline: None, reload_retries: 0});
                let models = ["faux-1", "faux-2"].map(|id| mode.session().model_registry().find("faux", id).unwrap());
                let expected = source.request(&serde_json::json!({"models":models,"search":search,"keys":keys,"colors":crate::modes::interactive::theme::REQUIRED_COLORS}));
                assert_eq!(expected["ok"], true, "{expected}");
                mode.handle_model_command(search.map(JsString::from)).await.unwrap();
                wait_for_fixture_model_selector(&mode).await;
                for key in keys {
                    terminal.borrow_mut().send_input(key);
                    mode.ui.process_events();
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                assert_eq!(child_session.model().id, expected["output"]["model"].as_str().unwrap(), "search {search:?}");
                assert_eq!(mode.session().model().id, "faux-1", "main must not change");
                assert_eq!(mode.footer_data_provider.get_session_override().unwrap().model.unwrap().id, child_session.model().id);
                mode.stop().unwrap();
            }
        }).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn model_command_uses_exact_references_and_the_frozen_model_selector() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new().run_until(async {
            let mut source = oracle::TsOracle::spawn("interactive-model-command-oracle.mts", "packages/coding-agent/src");
            let mut cases = [None, Some("2"), Some("faux-2"), Some("faux/faux-2"), Some("FAUX-2"), Some("Second"), Some("missing"), Some("")].into_iter().map(|search| (search, Vec::<&str>::new())).collect::<Vec<_>>();
            cases.extend([(Some("2"), vec!["\r"]), (Some("Second"), vec!["\x1b"]), (None, vec!["\x1b[B", "\r"])]);
            for (search, keys) in cases {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let models = ["faux-1", "faux-2"].iter().map(|id| mode.session().model_registry().find("faux", *id).unwrap()).collect::<Vec<_>>();
                let expected = source.request(&serde_json::json!({"models":models,"search":search,"keys":keys,"colors":crate::modes::interactive::theme::REQUIRED_COLORS}));
                assert_eq!(expected["ok"], true, "{expected}");
                let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS.iter().map(|name| ((*name).into(),crate::modes::interactive::theme::ColorValue::Index(0.0))).collect();
                crate::modes::interactive::theme::set_theme_instance(Arc::new(Theme::new(&colors,&[],crate::modes::interactive::theme::ColorMode::Color256,None,None).unwrap()));
                mode.handle_model_command(search.map(JsString::from)).await.unwrap();
                wait_for_fixture_model_selector(&mode).await;
                for key in &keys {
                    terminal.borrow_mut().send_input(key);
                    mode.ui.process_events();
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                let lines = mode.editor_container.borrow_mut().render(100);
                let has_selector = lines.iter().any(|line| pi_tui::utils::strip_terminal_sequences(line).contains("Only showing models from configured providers"));
                let selector_lines = has_selector.then(|| lines.iter().map(|line| line.units().to_vec()).collect::<Vec<_>>());
                let statuses = mode.chat_container.borrow_mut().render(100).into_iter().map(|line| pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line| !line.is_empty()).collect::<Vec<_>>();
                let actual = serde_json::json!({"model":mode.session().model().id,"selectorLines":selector_lines,"statuses":statuses});
                assert_eq!(actual, expected["output"], "query {search:?}, keys {keys:?}");
                mode.stop().unwrap();
            }
        }).await;
    }

    fn record_editor_terminal_snapshot(directory: &Path, trace: &Path, event: &str) {
        use std::io::Write as _;
        let mut files = std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                let bytes = entry
                    .file_type()
                    .unwrap()
                    .is_file()
                    .then(|| std::fs::read(entry.path()).unwrap());
                serde_json::json!({"name":entry.file_name().to_str().unwrap(),"bytes":bytes})
            })
            .collect::<Vec<_>>();
        files.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(trace)
            .unwrap();
        writeln!(file, "{}", serde_json::json!({"event":event,"files":files})).unwrap();
    }

    fn normalize_editor_clock_nonce(value: &mut serde_json::Value, path: &str, name: &str, expected_path: &str) {
        match value {
            serde_json::Value::String(text) => {
                *text = text.replace(path, expected_path).replace(name, "pi-editor-0.pi.md")
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    normalize_editor_clock_nonce(value, path, name, expected_path);
                }
            }
            serde_json::Value::Object(values) => {
                for value in values.values_mut() {
                    normalize_editor_clock_nonce(value, path, name, expected_path);
                }
            }
            _ => {}
        }
    }

    fn append_editor_callback_event(trace: &Path, event: &str) {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new().append(true).open(trace).unwrap();
        writeln!(file, "{}", serde_json::json!({"event":event})).unwrap();
    }

    fn editor_fixture_rejection(event: &str) -> CallbackError {
        CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Value(JsValue::String(
            JsString::from(event) + "-" + JsString::from_utf16(vec![0xd800]),
        )))
    }

    struct FallibleEditorFixture {
        text: Rc<RefCell<JsString>>,
        trace: PathBuf,
        throws: Vec<String>,
        omit_expanded: bool,
    }

    impl FallibleEditorFixture {
        fn call(&self, event: &str) -> Result<(), CallbackError> {
            append_editor_callback_event(&self.trace, event);
            if self.throws.iter().any(|point| point == event) {
                return Err(editor_fixture_rejection(event));
            }
            Ok(())
        }
    }
    impl Component for FallibleEditorFixture {
        fn render(&mut self, _width: usize) -> Vec<JsString> {
            Vec::new()
        }
    }
    impl EditorComponent for FallibleEditorFixture {
        fn get_text(&self) -> Result<JsString, CallbackError> {
            self.call("get")?;
            Ok(self.text.borrow().clone())
        }
        fn get_expanded_text(&self) -> Result<JsString, CallbackError> {
            if self.omit_expanded {
                return self.get_text();
            }
            self.call("expanded")?;
            Ok(self.text.borrow().clone())
        }
        fn set_text(&mut self, text: &JsString) -> Result<(), CallbackError> {
            self.call("set")?;
            *self.text.borrow_mut() = text.clone();
            Ok(())
        }
    }

    fn run_editor_flow_case(outcome: &str) {
        const CHILD: &str = "PI_EDITOR_FLOW_CHILD_ROOT";
        let sandbox = tempfile::tempdir().unwrap();
        let directory = sandbox.path().join("node temporary root");
        std::fs::create_dir(&directory).unwrap();
        let trace = sandbox.path().join("trace.jsonl");
        let heartbeat = sandbox.path().join("heartbeat");
        let fixture = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"))
            .join("tests/fixtures/external_editor.py");
        let command = format!(
            "python3 {} {} {} {outcome}  --flag",
            fixture.display(),
            trace.display(),
            heartbeat.display()
        );
        let command = if outcome == "missing" {
            "/pi-missing-editor".to_owned()
        } else if outcome == "sync-spawn" {
            format!("{}/child", fixture.display())
        } else {
            command
        };
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "modes::interactive::interactive_mode::ui_tests::tmpdir_external_editor_matches_frozen_inline_flow",
                "--nocapture",
            ])
            .env(CHILD, sandbox.path())
            .env("PI_EDITOR_FLOW_COMMAND", &command)
            .env("PI_EDITOR_FLOW_OUTCOME", outcome)
            .env("TMPDIR", &directory)
            .env_remove("TMP")
            .env_remove("TEMP")
            .current_dir(sandbox.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "stdout {}\nstderr {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let banner = format!("Launching external editor: {command}\nPi will resume when the editor exits.\n");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).matches(&banner).count(),
            usize::from(!matches!(outcome, "get" | "expanded" | "stop"))
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tmpdir_external_editor_sync_spawn_error_matches_frozen() {
        run_editor_flow_case("sync-spawn");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tmpdir_external_editor_matches_frozen_inline_flow() {
        const CHILD: &str = "PI_EDITOR_FLOW_CHILD_ROOT";
        let Some(sandbox) = std::env::var_os(CHILD) else {
            for outcome in [
                "success",
                "fail",
                "signal",
                "empty",
                "missing",
                "remove",
                "directory",
                "get",
                "expanded",
                "set",
                "stop",
                "start",
                "set+start",
            ] {
                run_editor_flow_case(outcome);
            }
            return;
        };
        let sandbox = PathBuf::from(sandbox);
        tokio::task::LocalSet::new().run_until(async {
            let directory=sandbox.join("node temporary root");
            let trace=sandbox.join("trace.jsonl");
            let heartbeat=sandbox.join("heartbeat");
            let command=std::env::var("PI_EDITOR_FLOW_COMMAND").unwrap();
            let outcome=std::env::var("PI_EDITOR_FLOW_OUTCOME").unwrap();
            let callbacks=matches!(outcome.as_str(),"get"|"expanded"|"set"|"stop"|"start"|"set+start");
            let throws=if callbacks {outcome.split('+').map(str::to_owned).collect::<Vec<_>>()} else {Vec::new()};
            let settings=SettingsManager::in_memory(serde_json::from_value(serde_json::json!({"externalEditor":command})).unwrap(),SettingsManagerCreateOptions::default());
            let (mode,terminal,_faux,_temp)=build_mode_with_settings(StaticResourceLoader::new(),None,settings).await;
            let _events=start(&mode).await;
            let text=JsString::from("original ")+JsString::from_utf16(vec![0xd800]);
            mode.editor().borrow_mut().set_text(&text).unwrap();
            let callback_text=Rc::new(RefCell::new(text.clone()));
            if callbacks {*mode.editor.borrow_mut()=Rc::new(RefCell::new(FallibleEditorFixture {text:callback_text.clone(),trace:trace.clone(),throws:throws.clone(),omit_expanded:outcome=="get"}));}
            let mut request=pi_js_compat::js_json::JsValue::from(serde_json::json!({"directory":directory,"trace":trace,"heartbeat":heartbeat,"command":command,"now":0,"callbacks":callbacks,"throwAt":throws,"omitExpanded":outcome=="get"}));
            request.as_object_mut().unwrap().insert("text".into(),pi_js_compat::js_json::JsValue::String(text));
            let mut source=crate::test_oracle::TsOracle::spawn("interactive-external-editor-oracle.mts","packages/coding-agent/src");
            let expected=source.request(&request).to_serde().unwrap();
            assert_eq!(expected["ok"],true,"{expected}");
            if heartbeat.exists() {std::fs::remove_file(&heartbeat).unwrap();}
            let oracle_temp=directory.join("pi-editor-0.pi.md");
            if oracle_temp.is_dir() {std::fs::remove_dir(&oracle_temp).unwrap();}
            std::fs::write(&trace,"").unwrap();
            let observed_directory=directory.clone();let observed_trace=trace.clone();
            terminal.borrow_mut().lifecycle_observer=Some(Rc::new(move |event| {record_editor_terminal_snapshot(&observed_directory,&observed_trace,event);if throws.iter().any(|point|point==event){return Err(editor_fixture_rejection(event));}Ok(())}));
            let heartbeat_task=tokio::task::spawn_local({let heartbeat=heartbeat.clone();async move {tokio::task::yield_now().await;std::fs::write(heartbeat,"ready").unwrap();}});
            let before=pi_js_compat::date::js_now_millis() as i64;
            let result=mode.handle_open_external_editor().await;
            let error=match result {Ok(())=>serde_json::Value::Null,Err(error)=> match error.thrown_value() {pi_js_compat::abort::AbortReason::Error(native)=>native.value().to_serde().unwrap(),pi_js_compat::abort::AbortReason::Value(JsValue::String(value))=>serde_json::json!({"rawUnits":value.units()}),other=>panic!("unexpected rejection {other:?}")}};
            let after=pi_js_compat::date::js_now_millis() as i64;
            heartbeat_task.await.unwrap();
            terminal.borrow_mut().lifecycle_observer=None;
            let native_trace=std::fs::read_to_string(&trace).unwrap().lines().map(|line|serde_json::from_str::<serde_json::Value>(line).unwrap()).collect::<Vec<_>>();
            let native_text=if callbacks {callback_text.borrow().units().to_vec()}else{mode.editor().borrow().get_text().unwrap().units().to_vec()};
            let native_chat=mode.chat_container.borrow_mut().render(100).iter().map(|line|line.units().to_vec()).collect::<Vec<_>>();
            let path=native_trace.iter().find(|event|event["event"]=="stop").map(|stop| {
                let name=stop["files"].as_array().unwrap().iter().filter_map(|file|file["name"].as_str()).find(|name|name.starts_with("pi-editor-")).unwrap();
                let path=directory.join(name);
                assert_eq!(path.parent(),Some(directory.as_path()));
                let millis=name.strip_prefix("pi-editor-").and_then(|name|name.strip_suffix(".pi.md")).expect("literal frozen pi-editor-${Date.now()}.pi.md").parse::<i64>().unwrap();
                assert!((before.min(after)..=before.max(after)).contains(&millis));
                path
            });
            let mut actual=serde_json::json!({"trace":native_trace,"text":native_text,"chat":native_chat,"error":error});
            if let Some(path)=path {normalize_editor_clock_nonce(&mut actual,path.to_str().unwrap(),path.file_name().unwrap().to_str().unwrap(),directory.join("pi-editor-0.pi.md").to_str().unwrap());}
            assert_eq!(actual["trace"],expected["output"]["trace"]);
            assert_eq!(actual["text"],expected["output"]["text"]);
            assert_eq!(actual["chat"],expected["output"]["chat"]);
            assert_eq!(actual["error"],expected["output"]["error"]);
            mode.stop().unwrap();
        }).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tmpdir_clipboard_and_share_consumers_match_frozen_mutable_environment() {
        const CHILD: &str = "PI_TMPDIR_CONSUMER_CHILD";
        let Some(operation) = std::env::var(CHILD).ok() else {
            let mut failures = Vec::new();
            for operation in ["powershell", "image", "image-text", "share"] {
                let sandbox = tempfile::tempdir().unwrap();
                let before = sandbox.path().join("before");
                let after = sandbox.path().join("after-😀");
                let bin = sandbox.path().join("bin");
                for path in [&before, &after, &bin] {
                    std::fs::create_dir(path).unwrap();
                }
                let fixture = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"))
                    .join("tests/fixtures/tmpdir_consumers.py");
                for program in ["wl-paste", "xclip", "wslpath", "gh"] {
                    let path = bin.join(program);
                    std::fs::copy(&fixture, &path).unwrap();
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
                }
                let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","modes::interactive::interactive_mode::ui_tests::tmpdir_clipboard_and_share_consumers_match_frozen_mutable_environment","--nocapture"])
                    .env(CHILD,operation).env("PATH",std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap()))).unwrap()).env("PI_TMPDIR_PROBE_OPERATION",operation).env("PI_TMPDIR_PROBE_REPORT",sandbox.path().join("report.json")).env("PI_TMPDIR_PROBE_AFTER",&after).env("PI_TMPDIR_PROBE_BIN",&bin).env("TMPDIR",before).env_remove("TMP").env_remove("TEMP").output().unwrap();
                if !output.status.success() {
                    failures.push(format!(
                        "{operation}:stdout {} stderr {}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
            }
            assert!(failures.is_empty(), "{}", failures.join("\n"));
            return;
        };
        tokio::task::LocalSet::new()
            .run_until(async {
                use pi_js_compat::process_env;
                let after = std::env::var("PI_TMPDIR_PROBE_AFTER").unwrap();
                let bin = PathBuf::from(std::env::var_os("PI_TMPDIR_PROBE_BIN").unwrap());
                let report = PathBuf::from(std::env::var_os("PI_TMPDIR_PROBE_REPORT").unwrap());
                let mut source = crate::test_oracle::TsOracle::spawn(
                    "interactive-tmpdir-consumers-oracle.mts",
                    "packages/coding-agent/src",
                );
                let mut mismatches = Vec::new();
                for environment in [
                    serde_json::json!({"TMPDIR":after}),
                    serde_json::json!({"TMPDIR":"","TMP":after}),
                    serde_json::json!({"TEMP":after}),
                    serde_json::json!({"TMPDIR":format!("{after}///")}),
                ] {
                    process_env::clear();
                    for key in ["TMPDIR", "TMP", "TEMP"] {
                        process_env::remove(key);
                    }
                    for (key, value) in environment.as_object().unwrap() {
                        process_env::set(key, value.as_str().unwrap());
                    }
                    let path = std::env::join_paths(
                        std::iter::once(bin.clone()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
                    )
                    .unwrap();
                    process_env::set("PATH", path.to_str().unwrap());
                    process_env::set("WAYLAND_DISPLAY", "fixture");
                    process_env::set("WSL_DISTRO_NAME", "fixture");
                    if operation == "image-text" {
                        process_env::remove("WSL_DISTRO_NAME");
                    }
                    let expected = source.request(&serde_json::json!({"operation":operation,"env":environment}));
                    assert_eq!(expected["ok"], true, "{expected}");
                    let path = if operation == "powershell" {
                        let environment = crate::utils::clipboard_image::ClipboardEnvironment::from_process();
                        let result = crate::utils::clipboard_image::read_clipboard_image(&environment, None)
                            .await
                            .unwrap();
                        assert!(result.is_none());
                        let observed: serde_json::Value =
                            serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
                        JsString::from(observed["path"].as_str().unwrap())
                    } else {
                        let (mode, _terminal, _faux, _temp) = if operation == "share" {
                            build_mode_with_persisted_session().await
                        } else {
                            build_mode().await
                        };
                        let _events = start(&mode).await;
                        if operation == "image-text" {
                            mode.editor()
                                .borrow_mut()
                                .set_text(&JsString::from("original"))
                                .unwrap();
                            mode.handle_clipboard_paste().await;
                            assert_eq!(expected["output"]["unchanged"], true);
                            assert_eq!(
                                mode.editor().borrow().get_text().unwrap(),
                                "original",
                                "frozen image-only paste ignores text clipboard"
                            );
                            mode.stop().unwrap();
                            continue;
                        }
                        let path = if operation == "image" {
                            mode.handle_clipboard_paste().await;
                            let path = mode.editor().borrow().get_text().unwrap();
                            assert_eq!(
                                std::fs::read(encode_node_utf8(&path)).unwrap(),
                                b"clipboard-image-bytes"
                            );
                            path
                        } else {
                            mode.session()
                                .session_manager()
                                .append_message(AgentMessage::User(pi_ai::types::UserMessage {
                                    role: pi_ai::types::UserRole::Tag,
                                    content: pi_ai::types::UserContent::Text("share fixture".into()),
                                    input_source: None,
                                    provider_native: None,
                                    timestamp: 0.0,
                                }))
                                .unwrap();
                            mode.session()
                                .session_manager()
                                .append_message(AgentMessage::Assistant(Box::new(AssistantMessage::empty(
                                    "faux", "faux", "faux-1", 1.0,
                                ))))
                                .unwrap();
                            mode.handle_share_command_with_program(&JsString::from(bin.join("gh").to_str().unwrap()))
                                .await;
                            let bytes = std::fs::read(&report).unwrap_or_else(|error| {
                                panic!(
                                    "share fixture did not reach gist call: {error}; frame {:?}",
                                    mode.chat_container.borrow_mut().render(200)
                                )
                            });
                            let observed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                            assert_eq!(observed["exists"], true);
                            JsString::from(observed["path"].as_str().unwrap())
                        };
                        mode.stop().unwrap();
                        path
                    };
                    let actual = crate::utils::paths::node_dirname(&path);
                    let expected = JsString::from(expected["output"]["root"].as_str().unwrap());
                    if actual != expected {
                        mismatches.push(format!("env={environment},actual={actual:?},expected={expected:?}"));
                    }
                }
                process_env::clear();
                assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn loaded_diagnostic_sections_match_frozen_skill_prompt_extension_theme_order() {
        use pi_js_compat::js_json::{JsObject, JsValue, to_value};
        tokio::task::LocalSet::new()
            .run_until(async {
                let diagnostic = |message: JsString, path: &str| ResourceDiagnostic {
                    diagnostic_type: ResourceDiagnosticType::Warning,
                    message,
                    path: Some(path.into()),
                    collision: None,
                };
                let skill_diagnostics = vec![diagnostic("skill warning".into(), "/tmp/skill-warning")];
                let prompt_diagnostics = vec![diagnostic("prompt warning".into(), "/tmp/prompt-warning")];
                let theme_diagnostics = vec![diagnostic(
                    JsString::from("theme warning ") + JsString::from_utf16(vec![0xdc00]),
                    "/tmp/theme-warning",
                )];
                let extension_runtime = crate::core::extensions::create_extension_runtime();
                let factory: crate::core::extensions::ExtensionFactory = Arc::new(|api| {
                    api.register_command("model", None, None, Arc::new(|_, _| Box::pin(async { Ok(()) })))
                        .unwrap();
                    Box::pin(async { Ok(()) })
                });
                let extension = crate::core::extensions::load_extension_from_factory(
                    factory,
                    &"/tmp".into(),
                    Arc::new(crate::core::event_bus::EventBusController::new()),
                    extension_runtime.clone(),
                    &"/tmp/order-extension".into(),
                )
                .await
                .unwrap();
                let loader = Arc::new(StaticResourceLoader {
                    skills: crate::core::resource_loader::LoadedSkills {
                        skills: Vec::new(),
                        diagnostics: skill_diagnostics.clone(),
                    },
                    prompts: crate::core::resource_loader::LoadedPrompts {
                        prompts: Vec::new(),
                        diagnostics: prompt_diagnostics.clone(),
                    },
                    themes: crate::core::resource_loader::LoadedThemes {
                        themes: Vec::new(),
                        diagnostics: theme_diagnostics.clone(),
                    },
                    extensions: Some(crate::core::extensions::LoadExtensionsResult {
                        extensions: vec![extension],
                        errors: Vec::new(),
                        runtime: extension_runtime,
                    }),
                    ..Default::default()
                });
                let (mode, _terminal, _faux, _temp) = build_mode_with_loader(loader, None).await;
                mode.settings_mut(|settings| settings.set_quiet_startup(true));
                let _events = start(&mode).await;
                let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS
                    .iter()
                    .map(|name| ((*name).into(), crate::modes::interactive::theme::ColorValue::Index(0.0)))
                    .collect();
                crate::modes::interactive::theme::set_theme_instance(Arc::new(
                    Theme::new(
                        &colors,
                        &[],
                        crate::modes::interactive::theme::ColorMode::Color256,
                        None,
                        None,
                    )
                    .unwrap(),
                ));
                let commands = mode
                    .session()
                    .extension_runner()
                    .get_registered_commands()
                    .into_iter()
                    .map(|command| {
                        JsValue::Object(JsObject::from([
                            ("name".into(), JsValue::String(command.name)),
                            ("invocationName".into(), JsValue::String(command.invocation_name)),
                            ("sourceInfo".into(), to_value(&command.source_info).unwrap()),
                        ]))
                    })
                    .collect();
                let mut request = JsValue::from(
                    serde_json::json!({"width":160,"colors":crate::modes::interactive::theme::REQUIRED_COLORS}),
                );
                let fields = request.as_object_mut().unwrap();
                fields.insert("commands".into(), JsValue::Array(commands));
                fields.insert("skillDiagnostics".into(), to_value(&skill_diagnostics).unwrap());
                fields.insert("promptDiagnostics".into(), to_value(&prompt_diagnostics).unwrap());
                fields.insert("themeDiagnostics".into(), to_value(&theme_diagnostics).unwrap());
                let mut source = crate::test_oracle::TsOracle::spawn(
                    "interactive-builtin-conflicts-oracle.mts",
                    "packages/coding-agent/src",
                );
                let expected = source.request(&request);
                assert_eq!(
                    expected.get("ok").and_then(JsValue::as_bool),
                    Some(true),
                    "{expected:?}"
                );
                mode.show_loaded_resources(false, true).unwrap();
                let actual = mode.loaded_resources_container.borrow_mut().render(160);
                let expected = expected.get("output").unwrap().as_array().unwrap();
                assert_eq!(actual.len(), expected.len());
                for (index, (line, expected)) in actual.iter().zip(expected).enumerate() {
                    let units = expected
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
                        .collect::<Vec<_>>();
                    assert_eq!(line.units(), units, "row {index}");
                }
                assert!(actual.iter().any(|line| line.units().contains(&0xdc00)));
                mode.stop().unwrap();
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rank5_builtin_conflicts_render_frozen_registered_command_diagnostics() {
        use pi_js_compat::js_json::{JsObject, JsValue, to_value};
        tokio::task::LocalSet::new()
            .run_until(async {
                let mut source = crate::test_oracle::TsOracle::spawn(
                    "interactive-builtin-conflicts-oracle.mts",
                    "packages/coding-agent/src",
                );
                for registrations in [
                    vec![vec!["ordinary"]],
                    vec![vec!["model"]],
                    vec![vec!["model"], vec!["model"]],
                    vec![vec!["hotkeys", "model", "MODEL"]],
                ] {
                    let extension_runtime = crate::core::extensions::create_extension_runtime();
                    let mut extensions = Vec::new();
                    for (index, names) in registrations.into_iter().enumerate() {
                        let factory: crate::core::extensions::ExtensionFactory = Arc::new(move |api| {
                            for name in &names {
                                api.register_command(*name, None, None, Arc::new(|_, _| Box::pin(async { Ok(()) })))
                                    .unwrap();
                            }
                            Box::pin(async { Ok(()) })
                        });
                        let mut path: JsString = format!("/tmp/conflict-{index}").into();
                        if index > 0 {
                            path.push_str(JsString::from_utf16(vec![0xd800]));
                        }
                        extensions.push(
                            crate::core::extensions::load_extension_from_factory(
                                factory,
                                &"/tmp".into(),
                                Arc::new(crate::core::event_bus::EventBusController::new()),
                                extension_runtime.clone(),
                                &path,
                            )
                            .await
                            .unwrap(),
                        );
                    }
                    let loader = Arc::new(StaticResourceLoader {
                        extensions: Some(crate::core::extensions::LoadExtensionsResult {
                            extensions,
                            errors: Vec::new(),
                            runtime: extension_runtime,
                        }),
                        ..Default::default()
                    });
                    let (mode, _terminal, _faux, _temp) = build_mode_with_loader(loader, None).await;
                    mode.settings_mut(|settings| settings.set_quiet_startup(true));
                    let _events = start(&mode).await;
                    let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS
                        .iter()
                        .map(|name| ((*name).into(), crate::modes::interactive::theme::ColorValue::Index(0.0)))
                        .collect();
                    crate::modes::interactive::theme::set_theme_instance(Arc::new(
                        Theme::new(
                            &colors,
                            &[],
                            crate::modes::interactive::theme::ColorMode::Color256,
                            None,
                            None,
                        )
                        .unwrap(),
                    ));
                    let commands = mode
                        .session()
                        .extension_runner()
                        .get_registered_commands()
                        .into_iter()
                        .map(|command| {
                            JsValue::Object(JsObject::from([
                                ("name".into(), JsValue::String(command.name)),
                                ("invocationName".into(), JsValue::String(command.invocation_name)),
                                ("sourceInfo".into(), to_value(&command.source_info).unwrap()),
                            ]))
                        })
                        .collect();
                    let mut request = JsValue::from(
                        serde_json::json!({"width":160,"colors":crate::modes::interactive::theme::REQUIRED_COLORS}),
                    );
                    request
                        .as_object_mut()
                        .unwrap()
                        .insert("commands".into(), JsValue::Array(commands));
                    let expected = source.request(&request);
                    assert_eq!(
                        expected.get("ok").and_then(JsValue::as_bool),
                        Some(true),
                        "{expected:?}"
                    );
                    mode.show_loaded_resources(false, true).unwrap();
                    let lines = mode.loaded_resources_container.borrow_mut().render(160);
                    let expected = expected.get("output").unwrap().as_array().unwrap();
                    assert_eq!(lines.len(), expected.len(), "request {request:?}");
                    for (index, (line, expected)) in lines.iter().zip(expected).enumerate() {
                        let units = expected
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
                            .collect::<Vec<_>>();
                        assert_eq!(line.units(), units, "row {index}");
                    }
                    mode.stop().unwrap();
                }
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rank5_model_select_shortcut_matches_frozen_key_dispatch_and_selection() {
        tokio::task::LocalSet::new().run_until(async {
            let mut source=crate::test_oracle::TsOracle::spawn("interactive-model-command-oracle.mts","packages/coding-agent/src");
            for keys in [vec![],vec!["\x1b"],vec!["\r"],vec!["\x1b[B","\r"]] {
                let (mode,terminal,_faux,_temp)=build_mode().await;
                let _events=start(&mode).await;
                let colors:Vec<_>=crate::modes::interactive::theme::REQUIRED_COLORS.iter().map(|name|((*name).into(),crate::modes::interactive::theme::ColorValue::Index(0.0))).collect();
                crate::modes::interactive::theme::set_theme_instance(Arc::new(Theme::new(&colors,&[],crate::modes::interactive::theme::ColorMode::Color256,None,None).unwrap()));
                let models=["faux-1","faux-2"].iter().map(|id|mode.session().model_registry().find("faux",*id).unwrap()).collect::<Vec<_>>();
                let expected=source.request(&serde_json::json!({"models":models,"search":null,"keys":keys,"viaShortcut":true,"colors":crate::modes::interactive::theme::REQUIRED_COLORS}));
                assert_eq!(expected["ok"],true,"{expected}");
                terminal.borrow_mut().send_input("\x0c");
                mode.ui.process_events();
                mode.drain_actions().await;
                wait_for_fixture_model_selector(&mode).await;
                for key in &keys {
                    terminal.borrow_mut().send_input(key);
                    mode.ui.process_events();
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                let lines=mode.editor_container.borrow_mut().render(100);
                let has_selector=lines.iter().any(|line|pi_tui::utils::strip_terminal_sequences(line).contains("Only showing models from configured providers"));
                let selector_lines=has_selector.then(||lines.iter().map(|line|line.units().to_vec()).collect::<Vec<_>>());
                let statuses=mode.chat_container.borrow_mut().render(100).into_iter().map(|line|pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line|!line.is_empty()).collect::<Vec<_>>();
                assert_eq!(serde_json::json!({"model":mode.session().model().id,"selectorLines":selector_lines,"statuses":statuses}),expected["output"],"keys {keys:?}");
                mode.stop().unwrap();
            }
        }).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rank5_hotkeys_command_renders_the_complete_frozen_page() {
        tokio::task::LocalSet::new().run_until(async {
            let mut source=crate::test_oracle::TsOracle::spawn("interactive-hotkeys-oracle.mts","packages/coding-agent/src");
            let (mode,_terminal,_faux,_temp)=build_mode().await;
            let _events=start(&mode).await;
            let colors:Vec<_>=crate::modes::interactive::theme::REQUIRED_COLORS.iter().map(|name|((*name).into(),crate::modes::interactive::theme::ColorValue::Index(0.0))).collect();
            crate::modes::interactive::theme::set_theme_instance(Arc::new(Theme::new(&colors,&[],crate::modes::interactive::theme::ColorMode::Color256,None,None).unwrap()));
            assert!(mode.handle_slash_command(&"hotkeys".into(),&JsString::default()).await.unwrap());
            for width in [40,60,100] {
                let expected=source.request(&serde_json::json!({"width":width,"colors":crate::modes::interactive::theme::REQUIRED_COLORS,"shortcuts":[]}));
                assert_eq!(expected["ok"],true,"{expected}");
                let lines=mode.chat_container.borrow_mut().render(width);
                let expected=expected["output"].as_array().unwrap();
                assert_eq!(lines.len(),expected.len(),"width {width}");
                for (index,(line,expected)) in lines.iter().zip(expected).enumerate() {
                    assert_eq!(serde_json::json!(line.units()),*expected,"width {width}, row {index}");
                }
            }
            mode.stop().unwrap();
        }).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rank5_hotkeys_preserve_lone_surrogate_empty_and_absent_descriptions() {
        use pi_js_compat::js_json::{JsObject, JsValue};
        tokio::task::LocalSet::new()
            .run_until(async {
                let extension_runtime = crate::core::extensions::create_extension_runtime();
                let descriptions = vec![
                    Some(JsString::from_utf16(vec![0xd800, 0x78])),
                    Some(JsString::default()),
                    None,
                ];
                let keys = ["alt+shift+q", "alt+shift+w", "alt+shift+e"];
                let captured = descriptions.clone();
                let factory: crate::core::extensions::ExtensionFactory = Arc::new(move |api| {
                    for (key, description) in keys.iter().zip(&captured) {
                        api.register_shortcut(
                            (*key).into(),
                            description.clone(),
                            Arc::new(|_| Box::pin(async { Ok(()) })),
                        )
                        .unwrap();
                    }
                    Box::pin(async { Ok(()) })
                });
                let extension_path: JsString = "/tmp/hotkeys-description".into();
                let extension = crate::core::extensions::load_extension_from_factory(
                    factory,
                    &"/tmp".into(),
                    Arc::new(crate::core::event_bus::EventBusController::new()),
                    extension_runtime.clone(),
                    &extension_path,
                )
                .await
                .unwrap();
                let loader = Arc::new(StaticResourceLoader {
                    extensions: Some(crate::core::extensions::LoadExtensionsResult {
                        extensions: vec![extension],
                        errors: Vec::new(),
                        runtime: extension_runtime,
                    }),
                    ..Default::default()
                });
                let (mode, _terminal, _faux, _temp) = build_mode_with_loader(loader, None).await;
                let _events = start(&mode).await;
                let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS
                    .iter()
                    .map(|name| ((*name).into(), crate::modes::interactive::theme::ColorValue::Index(0.0)))
                    .collect();
                crate::modes::interactive::theme::set_theme_instance(Arc::new(
                    Theme::new(
                        &colors,
                        &[],
                        crate::modes::interactive::theme::ColorMode::Color256,
                        None,
                        None,
                    )
                    .unwrap(),
                ));
                let mut request = JsValue::from(
                    serde_json::json!({"width":100,"colors":crate::modes::interactive::theme::REQUIRED_COLORS}),
                );
                let shortcuts = keys
                    .iter()
                    .zip(descriptions)
                    .map(|(key, description)| {
                        let mut fields = JsObject::from([
                            ("key".into(), JsValue::String((*key).into())),
                            ("extensionPath".into(), JsValue::String(extension_path.clone())),
                        ]);
                        if let Some(description) = description {
                            fields.insert("description".into(), JsValue::String(description));
                        }
                        JsValue::Object(fields)
                    })
                    .collect();
                request
                    .as_object_mut()
                    .unwrap()
                    .insert("shortcuts".into(), JsValue::Array(shortcuts));
                let mut source =
                    crate::test_oracle::TsOracle::spawn("interactive-hotkeys-oracle.mts", "packages/coding-agent/src");
                let expected = source.request(&request);
                assert_eq!(
                    expected.get("ok").and_then(JsValue::as_bool),
                    Some(true),
                    "{expected:?}"
                );
                mode.handle_slash_command(&"hotkeys".into(), &JsString::default())
                    .await
                    .unwrap();
                let actual = mode.chat_container.borrow_mut().render(100);
                let expected = expected.get("output").unwrap().as_array().unwrap();
                assert_eq!(actual.len(), expected.len());
                for (line, expected) in actual.iter().zip(expected) {
                    let units = expected
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
                        .collect::<Vec<_>>();
                    assert_eq!(line.units(), units);
                }
                assert!(
                    actual.iter().any(|line| line.units().contains(&0xd800)),
                    "description must stay raw UTF-16"
                );
                mode.stop().unwrap();
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn original_literal_command_sequence_matches_frozen_prompt_validation() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new()
            .run_until(async {
                let names = [
                    "new",
                    "settings",
                    "scoped-models",
                    "share",
                    "copy",
                    "unname",
                    "session",
                    "tools",
                    "extensions",
                    "changelog",
                    "hotkeys",
                    "fork",
                    "clone",
                    "tree",
                    "trust",
                    "logout",
                    "continue",
                    "reload",
                    "arminsayshi",
                ];
                let inputs = names.iter().map(|name| format!("/{name} extra")).collect::<Vec<_>>();
                let mut source =
                    oracle::TsOracle::spawn("interactive-command-sequence-oracle.mts", "packages/coding-agent/src");
                let expected = source.request(&serde_json::json!({"inputs":inputs}));
                assert_eq!(expected["ok"], true, "{expected}");
                let (mode, _terminal, faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let session_id = mode.session().session_manager().get_session_id().to_owned();
                let mut frames = Vec::new();
                for text in &inputs {
                    faux.set_responses(vec![
                        faux_assistant_message(faux_text("literal accepted"), Default::default()).into(),
                    ]);
                    mode.session().agent().wait_for_idle().await;
                    assert!(!mode.session().is_compacting());
                    mode.submit(text.into()).await.unwrap();
                    mode.session().agent().wait_for_idle().await;
                    let accepted = mode
                        .session()
                        .messages()
                        .iter()
                        .filter(|message| matches!(message, AgentMessage::User(_)))
                        .map(InteractiveMode::get_user_message_text)
                        .collect::<Vec<_>>();
                    let errors = mode
                        .chat_container
                        .borrow_mut()
                        .render(120)
                        .into_iter()
                        .map(|line| pi_tui::utils::strip_terminal_sequences(&line).trim())
                        .filter(|line| !line.is_empty())
                        .collect::<Vec<_>>();
                    frames.push(serde_json::json!({"accepted":accepted,"errors":errors}));
                    assert_eq!(mode.session().session_manager().get_session_id(), &session_id);
                }
                assert_eq!(serde_json::json!(frames), expected["frames"]);
                assert_eq!(expected["frames"][18]["accepted"].as_array().unwrap().len(), 18);
                assert_eq!(
                    expected["frames"][18]["errors"],
                    serde_json::json!(["Error: Unknown slash command: /arminsayshi"])
                );
                mode.stop().unwrap();
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unsupported_builtin_arguments_and_old_aliases_are_literal_prompts() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new()
            .run_until(async {
                let mut source =
                    oracle::TsOracle::spawn("interactive-command-dispatch-oracle.mts", "packages/coding-agent/src");
                let mut inputs = vec!["/new extra".to_owned()];
                for name in [
                    "settings",
                    "scoped-models",
                    "share",
                    "copy",
                    "unname",
                    "session",
                    "tools",
                    "extensions",
                    "changelog",
                    "hotkeys",
                    "fork",
                    "clone",
                    "tree",
                    "trust",
                    "logout",
                    "continue",
                    "reload",
                    "arminsayshi",
                    "dementedelves",
                    "resume",
                    "agents",
                    "quit",
                ] {
                    inputs.push(format!("/{name} extra"));
                }
                inputs.extend([
                    "/help".to_owned(),
                    "/thinking".to_owned(),
                    "/theme".to_owned(),
                    "/daxnuts".to_owned(),
                ]);
                for text in inputs {
                    let (mode, _terminal, _faux, _temp) = build_mode().await;
                    let _events = start(&mode).await;
                    let session_id = mode.session().session_manager().get_session_id().to_owned();
                    let prompts = Rc::new(RefCell::new(Vec::<JsString>::new()));
                    let observed = prompts.clone();
                    *mode.on_input_callback.borrow_mut() = Some(Rc::new(move |text| observed.borrow_mut().push(text)));
                    let expected = source.request(&serde_json::json!({"text":text}));
                    assert_eq!(expected["ok"], true, "{expected}");
                    assert_eq!(
                        expected["prompts"],
                        serde_json::json!([text]),
                        "frozen must submit literal text"
                    );
                    mode.submit(JsString::from(&text)).await.unwrap();
                    assert_eq!(serde_json::json!(*prompts.borrow()), expected["prompts"], "{text}");
                    assert_eq!(mode.session().session_manager().get_session_id(), &session_id, "{text}");
                    mode.stop().unwrap();
                }
            })
            .await;
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
                assert_eq!(mode.editor().borrow().get_text().unwrap(), "hi");
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
                mode.show_loaded_resources(false, false).unwrap();
                let rendered = pi_tui::utils::strip_terminal_sequences(JsString::join(
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
    async fn extension_widget_replacement_disposes_before_constructing_new_widget() {
        struct Widget(Arc<Mutex<Vec<&'static str>>>);
        impl ExtensionUiComponent for Widget {
            fn render(&self, _width: usize) -> Vec<JsString> {
                vec!["widget".into()]
            }
            fn dispose(&self) {
                self.0.lock().unwrap().push("disposed");
            }
        }
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let log = Arc::new(Mutex::new(Vec::new()));
                for _ in 0..2 {
                    let events = Arc::clone(&log);
                    mode.ui_sender
                        .send(UiRequest::SetWidgetComponent {
                            key: "lifetime".into(),
                            placement: WidgetPlacement::AboveEditor,
                            content: Some(Arc::new(move |_| {
                                events.lock().unwrap().push("constructed");
                                Box::new(Widget(Arc::clone(&events)))
                            })),
                        })
                        .unwrap();
                    mode.handle_ui_requests().await;
                }
                assert_eq!(*log.lock().unwrap(), vec!["constructed", "disposed", "constructed"]);
                mode.clear_extension_widgets();
                assert_eq!(
                    *log.lock().unwrap(),
                    vec!["constructed", "disposed", "constructed", "disposed"]
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn extension_component_targeted_refresh_preserves_unrelated_render_state() {
        struct Widget(Arc<Mutex<(JsString, usize)>>);
        impl ExtensionUiComponent for Widget {
            fn render(&self, _width: usize) -> Vec<JsString> {
                let mut state = self.0.lock().unwrap();
                state.1 += 1;
                vec![state.0.clone()]
            }
            fn render_identity(&self) -> usize {
                Arc::as_ptr(&self.0) as usize
            }
        }
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let changed = Arc::new(Mutex::new((JsString::from("widget before"), 0)));
                let unrelated = Arc::new(Mutex::new((JsString::from("unrelated row"), 0)));
                for (key, state) in [("target", Arc::clone(&changed)), ("other", Arc::clone(&unrelated))] {
                    mode.ui_sender
                        .send(UiRequest::SetWidgetComponent {
                            key: key.into(),
                            content: Some(Arc::new(move |_| Box::new(Widget(Arc::clone(&state))))),
                            placement: WidgetPlacement::AboveEditor,
                        })
                        .unwrap();
                }
                mode.handle_ui_requests().await;
                let widgets: Vec<_> = mode
                    .extension_widgets_above
                    .borrow()
                    .values()
                    .filter_map(|widget| match widget {
                        ExtensionWidget::Component(component) => Some(Rc::clone(component)),
                        _ => None,
                    })
                    .collect();
                mode.ui.clear();
                for widget in &widgets {
                    mode.ui.add_child(Rc::clone(widget));
                }
                mode.ui.start().unwrap();
                mode.ui.flush_pending().unwrap();
                let region = mode.ui.create_render_region(Rc::clone(&widgets[0])).unwrap();
                region.place(pi_tui::tui::RenderRegionRect {
                    row: 0,
                    col: 0,
                    width: 80,
                    height: 1,
                });
                let before = unrelated.lock().unwrap().1;
                changed.lock().unwrap().0 = "widget changed".into();
                terminal.borrow_mut().writes.clear();
                mode.extension_factory_context()
                    .tui
                    .request_component_render(Arc::as_ptr(&changed) as usize);
                mode.handle_ui_requests().await;
                mode.ui.flush_pending().unwrap();
                assert_eq!(
                    unrelated.lock().unwrap().1,
                    before,
                    "target refresh must not render another component"
                );
                assert!(JsString::join(terminal.borrow().writes.clone(), "\n").contains("widget changed"));
                mode.ui.stop().unwrap();
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
                        &js_path(
                            &Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"))
                                .join("theme")
                                .join("light.json"),
                        ),
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
            mode.ui.flush_pending().unwrap();
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

    mod selector_config_oracle {
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
    }

    #[test]
    fn session_selector_config_failure_is_deferred_to_the_loader_catch() {
        let directory = tempfile::tempdir().unwrap();
        let mut oracle = selector_config_oracle::TsOracle::spawn(
            "session-selector-config-boundary-oracle.ts",
            "packages/coding-agent/src/modes/interactive/interactive-mode.ts",
        );
        let result: serde_json::Value = oracle.request(&serde_json::json!({"cwd": directory.path().to_str().unwrap()}));
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(
            result["output"],
            serde_json::json!({"before": 0, "lookups": 1, "caught": true}),
            "{result}"
        );
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "modes::interactive::interactive_mode::ui_tests::session_selector_config_failure_child",
                "--nocapture",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "isolated selector HOME/config child"]
    async fn session_selector_config_failure_child() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, terminal, _faux, _temp) = build_mode().await;
                let mut events = start(&mode).await;
                pump(&mode, &mut events, &terminal, |output| output.contains("faux-1")).await;
                pi_js_compat::process_env::set(&crate::config::env_agent_dir(), "file://not-local/config");
                mode.show_session_selector();
                assert!(
                    mode.active_session_selector.borrow().is_some(),
                    "current scope must mount before default-dir lookup"
                );
                let selector = mode.active_session_selector.borrow().clone().unwrap();
                for _ in 0..5 {
                    selector.lock().await.poll(Instant::now());
                    tokio::task::yield_now().await;
                }
                selector.lock().await.handle_input(&JsString::from("\t"));
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Failed to load sessions:")
                })
                .await;
                assert!(
                    output.contains("Failed to load sessions: File URL host must be"),
                    "{output:?}"
                );
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn resume_missing_cwd_uses_caught_issue_after_session_file_changes_like_frozen() {
        mod oracle {
            include!(concat!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
                "/tests/support/oracle.rs"
            ));
        }
        tokio::task::LocalSet::new().run_until(async {
            let (mode, _terminal, _faux, temp) = build_mode().await;
            let file = temp.path().join("resume.jsonl");
            let header = |name: &str| format!("{}\n",serde_json::json!({"type":"session","version":3,"id":"resume-id","timestamp":"2025-01-01T00:00:00.000Z","cwd":temp.path().join(name).to_str().unwrap()}));
            let original_header = header("original-missing");
            let changed_header = header("changed-missing");
            std::fs::write(&file,&original_header).unwrap();
            let colors: Vec<_> = crate::modes::interactive::theme::REQUIRED_COLORS.iter().map(|name|((*name).into(),crate::modes::interactive::theme::ColorValue::Index(0.0))).collect();
            crate::modes::interactive::theme::set_theme_instance(Arc::new(Theme::new(&colors,&colors,crate::modes::interactive::theme::ColorMode::Color256,None,None).unwrap()));
            let mut source = oracle::TsOracle::spawn("interactive-resume-cwd-error-oracle.mts","packages/coding-agent/src");
            let expected = source.request(&serde_json::json!({"file":file.to_str().unwrap(),"fallback":mode.session().cwd().units(),"changedHeader":changed_header,"colors":crate::modes::interactive::theme::REQUIRED_COLORS}));
            assert_eq!(expected["ok"],true,"source: {expected}");
            std::fs::write(&file,original_header).unwrap();
            mode.runtime.set_host_transition_executor(Some(Arc::new({
                let file = file.clone();
                move |transition| {
                    let file = file.clone();
                    let changed_header = changed_header.clone();
                    tokio::task::spawn_local(async move {
                        transition.await;
                        std::fs::write(file,changed_header).unwrap();
                    });
                    Ok(())
                }
            })));
            let prior_session = mode.session().session_id();
            let resume = tokio::task::spawn_local({
                let mode = mode.clone();
                let path = js_path(&file);
                async move { mode.handle_resume_session(&path, None).await; }
            });
            let (title,message,respond) = tokio::time::timeout(Duration::from_secs(5),async {
                loop {
                    let request = mode.ui_requests.borrow_mut().as_mut().unwrap().try_recv();
                    match request {
                        Ok(UiRequest::Confirm {title,message,respond,..}) => break (title,message,respond),
                        Ok(_) => panic!("unexpected resume UI request"),
                        Err(std::sync::mpsc::TryRecvError::Empty) => tokio::task::yield_now().await,
                        Err(error) => panic!("resume UI closed: {error}"),
                    }
                }
            }).await.unwrap();
            respond.send(false).unwrap();
            tokio::time::timeout(Duration::from_secs(5),resume).await.unwrap().unwrap();
            let frames: Vec<_> = mode.chat_container.borrow_mut().render(200).into_iter().map(|line|pi_tui::utils::strip_terminal_sequences(&line).trim()).filter(|line|!line.is_empty()).map(|line|line.units().to_vec()).collect();
            assert_eq!(serde_json::json!({"confirmations":[{"title":title.units(),"message":message.units()}],"frames":frames,"cancelled":mode.session().session_id()==prior_session}),expected["output"]);
            mode.stop().unwrap();
        }).await;
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
                    let control_db = manager.get_metadata_control_db_path().cloned();
                    let mut replacement = SessionManager::create(&cwd, Some(&JsString::from(&session_dir)), None)
                        .expect("persisted manager");
                    replacement
                        .set_metadata_control_db_path(control_db.as_ref(), Default::default())
                        .expect("metadata binding");
                    *manager = replacement;
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
                            timestamp: 1.0,
                        },
                    ))
                    .expect("append message");
                target.persist_for_recovery().expect("persist target session");
                let target_id = target.get_session_id().to_owned();
                let target_file = target.get_session_file().expect("target file");
                assert!(std::path::Path::new(&encode_node_utf8(target_file)).exists());

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
                    mode.ui.flush_pending().unwrap();
                    mode.handle_ui_requests().await;
                    ready = mode.active_session_selector.borrow().as_ref().is_some_and(|selector| {
                        selector
                            .try_lock()
                            .map(|selector| selector.session_list().borrow().get_selected_session_path().is_some())
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
    async fn login_selector_opens_frozen_bedrock_setup_dialog() {
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

                terminal.borrow_mut().send_input("amazon-bedrock");
                mode.ui.process_events();
                terminal.borrow_mut().send_input("\r");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("Amazon Bedrock uses AWS credentials instead of a single API key.")
                })
                .await;
                assert!(output.contains("Amazon Bedrock uses AWS credentials instead of a single API key."));
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
                assert!(!mode.session().auth_storage().has("faux"));
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
                assert_eq!(mode.editor().borrow().get_text().unwrap(), "hi");
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
            _signal: Option<pi_js_compat::abort::AbortSignal>,
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
                let rendered = pi_tui::utils::strip_terminal_sequences(JsString::join(
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
                let rendered = pi_tui::utils::strip_terminal_sequences(JsString::join(
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
                        pi_tui::utils::strip_terminal_sequences(JsString::join(editor.borrow_mut().render(80), "\n"));
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
                    .model_registry()
                    .get_all()
                    .into_iter()
                    .find(|model| model.provider == "anthropic")
                    .expect("anthropic model");

                mode.maybe_warn_about_anthropic_subscription_auth(Some(model.clone()))
                    .await;
                let rendered = pi_tui::utils::strip_terminal_sequences(JsString::join(
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
                let rendered = pi_tui::utils::strip_terminal_sequences(JsString::join(
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
                    .model_registry()
                    .get_all()
                    .into_iter()
                    .find(|model| model.provider == "anthropic")
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
                    JsString::join(mode.chat_container.borrow_mut().render(200), "\n"),
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
                now: Some(Arc::new(|| Ok("2026-06-27T00:00:00.000Z".into()))),
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
            key_order: Vec::new(),
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
                path: Some(path.to_owned()),
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
                ..Default::default()
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
                    timestamp: 0.0,
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

                assert!(mode.select_agent_slot(1).unwrap());
                assert_eq!(store.get_selected_agent_id().as_ref(), Some(&active));
                assert!(!status_text(&mode).contains("not active"));

                assert!(mode.select_agent_slot(2).unwrap());
                assert_eq!(store.get_selected_agent_id().as_ref(), Some(&active));
                assert!(!mode.select_agent_slot(5).unwrap());
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
                        [
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
                        [
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
                let main_footer =
                    pi_tui::utils::strip_terminal_sequences(JsString::join(mode.footer.borrow_mut().render(200), "\n"));
                assert!(main_footer.contains("faux-1"), "{main_footer:?}");

                assert!(mode.select_agent_slot(1).unwrap());
                let footer =
                    pi_tui::utils::strip_terminal_sequences(JsString::join(mode.footer.borrow_mut().render(200), "\n"));
                assert!(footer.contains("faux-2"), "{footer:?}");
                assert!(!footer.contains("faux-1"), "{footer:?}");
                assert!(footer.contains('↑'), "child usage must be shown: {footer:?}");
                let chat = chat_text(&mode);
                assert!(chat.contains("child prompt"), "{chat:?}");

                assert!(mode.select_agent_view_from_bridge(&JsString::from("main")));
                let footer =
                    pi_tui::utils::strip_terminal_sequences(JsString::join(mode.footer.borrow_mut().render(200), "\n"));
                assert!(footer.contains("faux-1"), "{footer:?}");
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn signed_thinking_end_refreshes_hidden_public_narration() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                mode.hide_thinking_block.set(true);
                *mode.hidden_thinking_label.borrow_mut() = "PRIVATE".into();
                let mut partial = faux_assistant_message("", Default::default());
                partial.content = vec![pi_ai::types::AssistantContent::Thinking(
                    pi_ai::types::ThinkingContent::new("PUBLIC"),
                )];
                mode.handle_event(AgentSessionEvent::Agent(AgentEvent::MessageStart {
                    message: AgentMessage::Assistant(Box::new(partial.clone())),
                }))
                .await;
                let tail = || JsString::join(mode.transcript_tail_container.borrow_mut().render(80), "\n");
                assert!(tail().contains("PRIVATE"));
                for (index, signature) in [(0, None), (9, Some("Eg0KC0IJbmFycmF0aW9u"))] {
                    let mut ignored = partial.clone();
                    let pi_ai::types::AssistantContent::Thinking(thinking) = &mut ignored.content[0] else {
                        unreachable!()
                    };
                    thinking.thinking_signature = signature.map(str::to_owned);
                    mode.handle_event(AgentSessionEvent::Agent(AgentEvent::MessageUpdate {
                        message: AgentMessage::Assistant(Box::new(ignored.clone())),
                        assistant_message_event: Box::new(pi_ai::types::AssistantMessageEvent::ThinkingEnd {
                            content_index: index,
                            content: "PUBLIC".into(),
                            partial: ignored,
                        }),
                    }))
                    .await;
                    assert!(tail().contains("PRIVATE"));
                }
                let pi_ai::types::AssistantContent::Thinking(thinking) = &mut partial.content[0] else {
                    unreachable!()
                };
                thinking.thinking_signature = Some("Eg0KC0IJbmFycmF0aW9u".into());
                mode.handle_event(AgentSessionEvent::Agent(AgentEvent::MessageUpdate {
                    message: AgentMessage::Assistant(Box::new(partial.clone())),
                    assistant_message_event: Box::new(pi_ai::types::AssistantMessageEvent::ThinkingEnd {
                        content_index: 0,
                        content: "PUBLIC".into(),
                        partial,
                    }),
                }))
                .await;
                let output = tail();
                assert!(
                    output.contains("PUBLIC"),
                    "signed public narration must render despite hide-thinking: {output:?}"
                );
                assert!(!output.contains("PRIVATE"));
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
                assert!(mode.select_agent_slot(1).unwrap());
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
                        timestamp: 0.0,
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
                assert_eq!(mode.editor().borrow().get_text().unwrap(), "real editor");
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
                assert_eq!(mode.editor().borrow().get_text().unwrap(), "fixed-cell input");
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
                    )
                    .unwrap();
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
                mode.set_extension_header(Some(Arc::new(|_| {
                    Ok(crate::core::extensions::types::ExtensionUiHeader {
                        component: Box::new(StaticWidget),
                        set_expanded: None,
                    })
                })))
                .unwrap();
                mode.ui.render_now().unwrap();
                terminal.borrow_mut().writes.clear();
                tui.request_render_force();
                mode.handle_ui_requests().await;
                mode.ui.flush_pending().unwrap();
                assert!(
                    terminal.borrow().output().contains("custom widget"),
                    "forced redraw must repaint unchanged frame: {:?}",
                    terminal.borrow().writes
                );
                terminal.borrow_mut().writes.clear();
                tui.stop().unwrap();
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
                tui.stop().unwrap();
                let error = TerminalRawModeError::new(true, "fixture raw-mode failure");
                terminal.borrow_mut().start_error = Some(error.clone());
                let actual = tui.start().unwrap_err();
                assert_eq!(actual.thrown_value(), CallbackError::from(error.clone()).thrown_value());
                assert!(
                    actual
                        .original
                        .as_ref()
                        .unwrap()
                        .downcast_ref::<TerminalRawModeError>()
                        .is_some_and(|original| original == &error)
                );
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
                )
                .unwrap();
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
                mode.ui.render_now().unwrap();
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
                )
                .unwrap();
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
                    mode.editor().borrow_mut().set_text(&JsString::from("before")).unwrap();
                    let editor = mode.editor();
                    let (send, receive) = tokio::sync::oneshot::channel();
                    mode.show_extension_custom(
                        Rc::new(move |context, done| {
                            editor.borrow_mut().set_text(&JsString::from("factory")).unwrap();
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
                    )
                    .unwrap();
                    tokio::task::yield_now().await;
                    let ui = mode.ui_context();
                    ui.set_editor_text(&JsString::from("during")).unwrap();
                    mode.handle_ui_requests().await;
                    assert_eq!(mode.editor().borrow().get_text().unwrap(), "during");
                    terminal.borrow_mut().send_input("\r");
                    mode.ui.process_events();
                    assert_eq!(receive.await.unwrap().unwrap(), Some(pi_js_compat::js_json!("")));
                    assert_eq!(
                        mode.editor().borrow().get_text().unwrap(),
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
                        mode.editor().borrow_mut().set_text(&JsString::from("before")).unwrap();
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
                        ui.set_editor_text(&JsString::from("during")).unwrap();
                        mode.handle_ui_requests().await;
                        terminal.borrow_mut().writes.clear();
                        mode.ui.request_render_force();
                        mode.ui.render_now().unwrap();
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
                                mode.editor().borrow().get_text().unwrap(),
                                "during",
                                "late settlement must not restore twice"
                            );
                        } else if reject {
                            assert_eq!(result.take().unwrap().await, Err(CallbackError::new("async failure")));
                            assert!(!mode.ui.has_overlay_entries());
                            assert!(Rc::ptr_eq(&editor, &mode.ui.get_focused_component().unwrap()));
                            assert_eq!(
                                mode.editor().borrow().get_text().unwrap(),
                                if overlay { "during" } else { "before" }
                            );
                        } else {
                            assert_eq!(mode.ui.has_overlay_entries(), overlay);
                            assert!(!Rc::ptr_eq(&editor, &mode.ui.get_focused_component().unwrap()));
                            terminal.borrow_mut().writes.clear();
                            mode.ui.render_now().unwrap();
                            terminal.borrow_mut().send_input("\r");
                            mode.ui.process_events();
                            assert_eq!(result.take().unwrap().await.unwrap(), Some(pi_js_compat::js_json!("")));
                            assert_eq!(
                                mode.editor().borrow().get_text().unwrap(),
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
                        mode.editor().borrow_mut().set_text(&JsString::from("before")).unwrap();
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
                        )
                        .unwrap();
                        tokio::task::yield_now().await;
                        assert_eq!(receive.await.unwrap(), Err(CallbackError::new("rejected")));
                        mode.editor()
                            .borrow_mut()
                            .set_text(&JsString::from("after rejection"))
                            .unwrap();
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
                            mode.editor().borrow().get_text().unwrap(),
                            if overlay { "after rejection" } else { "before" }
                        );
                        mode.editor()
                            .borrow_mut()
                            .set_text(&JsString::from("after close"))
                            .unwrap();
                        done.call(None);
                        assert_eq!(
                            mode.editor().borrow().get_text().unwrap(),
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
                mode.editor()
                    .borrow_mut()
                    .set_text(&JsString::from("draft 🚀"))
                    .unwrap();
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
                    assert_eq!(result.await, Err(CallbackError::new("fixture constructor failed")));
                    assert!(!mode.ui.has_overlay_entries());
                    assert!(Rc::ptr_eq(&focused, &mode.ui.get_focused_component().unwrap()));
                    assert_eq!(mode.editor().borrow().get_text().unwrap(), "draft 🚀");
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
                )
                .unwrap();
                tokio::task::yield_now().await;
                terminal.borrow_mut().writes.clear();
                mode.ui.render_now().unwrap();
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
                )
                .unwrap();
                tokio::task::yield_now().await;
                mode.ui.render_now().unwrap();
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
                )
                .unwrap();
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
                )
                .unwrap();
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
                )
                .unwrap();
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
    async fn rendered_message_history_persistence_error_bubbles_before_later_work() {
        tokio::task::LocalSet::new().run_until(async {
            let (mode, _terminal, _faux, temp) = build_mode().await;
            let database = temp.path().join("failing-history.db");
            let editor = CustomEditor::new(
                Rc::new(InteractiveTuiHost(Rc::clone(&mode.ui))),
                crate::modes::interactive::theme::get_editor_theme(),
                KeybindingsManager::new(Default::default(), None),
                CustomEditorOptions {
                    prompt_history_control_db_path: Some(database.display().to_string().into()),
                    ..Default::default()
                },
            ).unwrap();
            let editor = Rc::new(RefCell::new(editor));
            *mode.editor.borrow_mut() = editor.clone();
            let connection = rusqlite::Connection::open(&database).unwrap();
            connection.execute_batch("CREATE TRIGGER deny_history BEFORE INSERT ON prompt_history BEGIN SELECT RAISE(ABORT, 'history-write-failed'); END;").unwrap();
            let message = AgentMessage::User(pi_ai::types::UserMessage {
                role: pi_ai::types::UserRole::Tag,
                content: pi_ai::types::UserContent::Text("persist me".into()),
                input_source: None,
                provider_native: None,
                timestamp: 1.0,
            });
            let result = crate::utils::uncaught::catch_panics(|| mode.add_message_to_chat(&message, true));
            assert!(result.is_err(), "frozen render aborts when inherited addToHistory throws");
            let error = result.unwrap_err();
            assert!(format_uncaught(error.as_ref()).contains("history-write-failed"));
            assert_eq!(editor.borrow().get_history(), vec![JsString::from("persist me")]);
            assert_eq!(connection.query_row("SELECT count(*) FROM prompt_history", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        }).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn selected_agent_history_persistence_error_is_reported_and_restores_submitted_text() {
        tokio::task::LocalSet::new().run_until(async {
            let store = test_store();
            let id = spawn_agent(&store, "writer", Some(1), Some(&"/tmp/child.jsonl".into()), None);
            let mut child = store.get_agent(&id).unwrap();
            child.parent_id = Some("main".into());
            child.revision += 1;
            store.publish_lifecycle_coordinator_snapshot(&child).unwrap();
            let (mode, _terminal, _faux, temp) = build_mode_with_extra_providers(
                StaticResourceLoader::new(), None,
                SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default()),
                Vec::new(), serde_json::Map::new(), false,
                TestModeSetup {
                    multi_agent_store: Some(store.clone()),
                    steer_multi_agent: Some(Arc::new(|_, _, _| Box::pin(async { Ok(()) }))),
                    ..Default::default()
                },
            ).await;
            assert!(matches!(store.select_agent_view_with_status(&id), crate::core::multi_agent_store::AgentViewSelectionResult::Ok(_)));
            let database = temp.path().join("failing-steering-history.db");
            let editor = CustomEditor::new(
                Rc::new(InteractiveTuiHost(Rc::clone(&mode.ui))),
                crate::modes::interactive::theme::get_editor_theme(),
                KeybindingsManager::new(Default::default(), None),
                CustomEditorOptions {
                    prompt_history_control_db_path: Some(database.display().to_string().into()),
                    ..Default::default()
                },
            ).unwrap();
            let editor = Rc::new(RefCell::new(editor));
            *mode.editor.borrow_mut() = editor.clone();
            let connection = rusqlite::Connection::open(&database).unwrap();
            connection.execute_batch("CREATE TRIGGER deny_history BEFORE INSERT ON prompt_history BEGIN SELECT RAISE(ABORT, 'history-write-failed'); END;").unwrap();
            assert!(mode.submit_selected_agent_steering(&"persist me".into(), &" persist me ".into()).await.unwrap());
            assert_eq!(editor.borrow().get_text(), " persist me ");
            assert!(chat_text(&mode).contains("history-write-failed"));
            assert_eq!(editor.borrow().get_history(), vec![JsString::from("persist me")]);
            assert_eq!(connection.query_row("SELECT count(*) FROM prompt_history", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        }).await;
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
                        timestamp: 1.0,
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
    async fn message_end_ignores_removed_cache_notice_setting() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (mode, _terminal, _faux, _temp) = build_mode().await;
                let _events = start(&mode).await;
                let settings = crate::core::settings_manager::Settings::from_iter([(
                    "showCacheMissNotices".into(),
                    JsValue::Bool(true),
                )]);
                mode.session().settings_manager().apply_overrides(&settings);
                let previous = faux_assistant_message(
                    "previous",
                    pi_ai::providers::faux::FauxAssistantMessageOptions {
                        timestamp: Some(0.0),
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
                        timestamp: Some(600_000.0),
                        ..Default::default()
                    },
                );
                current.provider = "test-provider".to_owned();
                current.model = "test-model".to_owned();
                current.usage.cache_write = 110_000.0;
                current.usage.cost.cache_write = 0.4125;
                current.usage.cost.total = 0.4125;
                mode.handle_event(AgentSessionEvent::Agent(AgentEvent::MessageStart {
                    message: AgentMessage::Assistant(Box::new(current.clone())),
                }))
                .await;
                mode.handle_event(AgentSessionEvent::Agent(AgentEvent::MessageEnd {
                    message: AgentMessage::Assistant(Box::new(current)),
                }))
                .await;
                let chat = chat_text(&mode);
                assert!(chat.contains("current"), "{chat:?}");
                assert!(
                    !chat.contains("Cache miss"),
                    "removed setting must not create old-only notices: {chat:?}"
                );
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
                    is_error: false,
                    timestamp: 0.0,
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
                        .join(encode_node_utf8(crate::config::config_dir_name()))
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
        fn get_text(&self) -> Result<JsString, CallbackError> {
            Ok(JsString::default())
        }

        fn set_text(&mut self, _text: &JsString) -> Result<(), CallbackError> {
            Ok(())
        }
    }

    struct TestDetachHandle {
        detached: Arc<std::sync::atomic::AtomicBool>,
    }

    impl crate::core::tool_detach_registry::ToolDetachHandle for TestDetachHandle {
        fn detach(&self) -> Result<bool, CallbackError> {
            self.detached.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(true)
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
                assert!(output.contains("Press ctrl+o to show full startup help and loaded resources."));
                terminal.borrow_mut().send_input("\u{f}");
                mode.ui.process_events();
                let output = pump(&mode, &mut events, &terminal, |output| {
                    output.contains("ctrl+b to background tools")
                })
                .await;
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
                    let runtime = mode.runtime.clone();
                    runtime.set_process_restarter(Arc::new(move |options| {
                        let calls = Arc::clone(&calls);
                        Box::pin(async move {
                            calls
                                .lock()
                                .unwrap()
                                .push((options.prompt.clone(), options.session_file.clone()));
                            Err(CallbackError::new("restart failed"))
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
                    mode.ui.flush_pending().unwrap();
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
            TestModeSetup::default(),
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
                    timestamp: 1.0,
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

                mode.stop().unwrap();

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
        let theme_dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent")).join("theme");
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
    fn compact_extension_labels_follow_source_abbreviations() {
        let extensions = vec![
            (JsString::from("/tmp/extensions/answer.ts"), None),
            (JsString::from("/tmp/extensions/btw.ts"), None),
        ];
        assert_eq!(
            get_compact_extension_labels(&extensions).unwrap(),
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
            get_compact_extension_labels(&extensions).unwrap(),
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
            )
            .unwrap(),
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
            )
            .unwrap(),
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
        let output = format_diagnostics(&diagnostics, &HashMap::new()).unwrap();
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
            format_extension_display_path(&JsString::from("/tmp/extensions/plan-mode/index.ts")).unwrap(),
            "/tmp/extensions/plan-mode"
        );
        assert_eq!(
            format_extension_display_path(&JsString::from("/tmp/extensions/plan-mode/index.js")).unwrap(),
            "/tmp/extensions/plan-mode"
        );
        assert_eq!(
            format_extension_display_path(&JsString::from("/tmp/extensions/webfetch.ts")).unwrap(),
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
            get_compact_display_path_segments(&JsString::from("/tmp/x/index.ts")).unwrap(),
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
    fn callable_status_uncaught_error_codes_follow_frozen_crash_policy() {
        const CODE: &str = "PI_STATUS_CRASH_CODE";
        const KIND: &str = "PI_STATUS_CRASH_KIND";
        if let (Ok(code), Ok(kind)) = (std::env::var(CODE), std::env::var(KIND)) {
            let shutting_down = AtomicBool::new(true);
            let native = pi_js_compat::node_error::NativeNodeError::from_constructor(
                pi_js_compat::node_error::NativeErrorClass::TypeError,
                "status policy",
            )
            .with_property("code", JsValue::String(code.clone().into()));
            match kind.as_str() {
                "native" => InteractiveMode::uncaught_crash(&native, &shutting_down, &[], None),
                "callback" => {
                    let callback: CallbackError = native.into();
                    InteractiveMode::uncaught_crash(&callback, &shutting_down, &[], None)
                }
                "raw" => {
                    let callback = CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Value(
                        pi_js_compat::js_json!({"code":code,"message":"status policy"}),
                    ));
                    InteractiveMode::uncaught_crash(&callback, &shutting_down, &[], None)
                }
                "io" => {
                    let error = match code.as_str() {
                        "EIO" => std::io::Error::from_raw_os_error(rustix::io::Errno::IO.raw_os_error()),
                        "EPIPE" => std::io::Error::from_raw_os_error(rustix::io::Errno::PIPE.raw_os_error()),
                        "ENOTCONN" => std::io::Error::from_raw_os_error(rustix::io::Errno::NOTCONN.raw_os_error()),
                        _ => std::io::Error::other("status policy"),
                    };
                    InteractiveMode::uncaught_crash(&error, &shutting_down, &[], None)
                }
                _ => panic!("unknown private crash case"),
            }
        }
        let mut failures = Vec::new();
        for kind in ["native", "callback", "raw", "io"] {
            for code in ["EIO", "EPIPE", "ENOTCONN", "ESTATUS"] {
                let lock = crate::test_oracle::read_source_lock();
                crate::test_oracle::verify_source_head(&lock);
                let mut source = crate::test_oracle::spawn_oracle_process(
                    &lock,
                    "interactive-status-crash-oracle.mts",
                    "packages/coding-agent/src",
                    &[(CODE, code.into()), (KIND, kind.into())],
                    std::process::Stdio::piped(),
                );
                drop(source.stdin.take());
                let source = source.wait_with_output().unwrap();
                let native = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "modes::interactive::interactive_mode::tests::callable_status_uncaught_error_codes_follow_frozen_crash_policy", "--nocapture"])
                    .env(CODE, code).env(KIND, kind).output().unwrap();
                let expected = if code == "ESTATUS" { 1 } else { 129 };
                assert_eq!(
                    source.status.code(),
                    Some(expected),
                    "source {kind}/{code}: {}",
                    String::from_utf8_lossy(&source.stderr)
                );
                if native.status.code() != source.status.code() {
                    failures.push(format!(
                        "{kind}/{code}: source {:?}, native {:?}",
                        source.status.code(),
                        native.status.code()
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
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
        include!(concat!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../pi-coding-agent"),
            "/tests/support/oracle.rs"
        ));
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
        context
            .set_status(&key, Some(ExtensionStatus::Text(text.clone())))
            .unwrap();
        match receiver.recv().unwrap() {
            UiRequest::SetStatus {
                key: actual,
                text: Some(ExtensionStatus::Text(value)),
            } => {
                assert_eq!(actual, key);
                assert_eq!(value, text);
            }
            _ => panic!("status not delivered"),
        }
        context
            .set_widget(&key, Some(std::slice::from_ref(&text)), None)
            .unwrap();
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
        context.set_title(&text).unwrap();
        match receiver.recv().unwrap() {
            UiRequest::SetTitle(value) => assert_eq!(value, text),
            _ => panic!("title not delivered"),
        }
        let selected = context.select(&key, std::slice::from_ref(&text), None);
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
