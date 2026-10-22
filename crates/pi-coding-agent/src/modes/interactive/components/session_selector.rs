//! Ported from `coding-agent/src/modes/interactive/components/session-selector.ts`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pi_ai::request::BoxFuture;
use pi_js_compat::string::js_trim;
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, Focusable};
use pi_tui::utils::{truncate_to_width_with, visible_width};

use super::keybinding_hints::{key_hint, key_text};
use super::session_selector_search::{
    NameFilter, SortMode, filter_and_sort_sessions, has_session_messages, has_session_name,
};
use crate::core::keybindings::KeybindingsManager;
use crate::core::session_control_db::{archive_session, remove_session_metadata};
use crate::core::session_manager::SessionInfo;
use crate::modes::interactive::theme::{ThemeBg, ThemeColor, with_theme};
use crate::utils::child_process::node_spawn_error_message;
use crate::utils::paths::canonicalize_path;

/// `(loaded, total)` progress callback.
pub type ProgressCallback = Rc<dyn Fn(usize, usize)>;

/// Async session-list loader (`SessionListProgress => Promise<SessionInfo[]>`).
///
/// The interactive TUI runs on a single-threaded local set, so the loader
/// future carries no `Send` bound (a TS promise has none either).
pub type SessionListLoader =
    Rc<dyn Fn(Option<ProgressCallback>) -> Pin<Box<dyn Future<Output = Result<Vec<SessionInfo>, String>> + 'static>>>;

/// Async rename callback (`(sessionPath, currentName) => Promise<void>`).
pub type RenameSessionFn = Rc<dyn Fn(String, Option<String>) -> BoxFuture<'static, Result<(), String>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionScope {
    Current,
    All,
    Archived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoadReason {
    Initial,
    Refresh,
    Toggle,
}

fn truncate(text: &str, width: usize, ellipsis: &str) -> String {
    truncate_to_width_with(text, width, ellipsis, false)
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

fn shorten_path(path: &str) -> String {
    let home = pi_js_compat::process_env::get("HOME")
        .or_else(|| pi_js_compat::process_env::get("USERPROFILE"))
        .unwrap_or_default();
    if !path.is_empty() && !home.is_empty() && path.starts_with(&home) {
        return format!("~{}", &path[home.len()..]);
    }
    path.to_owned()
}

fn format_session_date(modified_ms: i64, now_ms: i64) -> String {
    let diff_ms = now_ms - modified_ms;
    let diff_mins = diff_ms.div_euclid(60_000);
    let diff_hours = diff_ms.div_euclid(3_600_000);
    let diff_days = diff_ms.div_euclid(86_400_000);
    if diff_mins < 1 {
        return "now".to_owned();
    }
    if diff_mins < 60 {
        return format!("{diff_mins}m");
    }
    if diff_hours < 24 {
        return format!("{diff_hours}h");
    }
    if diff_days < 7 {
        return format!("{diff_days}d");
    }
    if diff_days < 30 {
        return format!("{}w", diff_days.div_euclid(7));
    }
    if diff_days < 365 {
        return format!("{}mo", diff_days.div_euclid(30));
    }
    format!("{}y", diff_days.div_euclid(365))
}

fn canonicalize(path: Option<&str>) -> Option<String> {
    path.filter(|path| !path.is_empty()).map(canonicalize_path)
}

/// Transient header status kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Error,
}

/// Transient header status message.
#[derive(Clone, Debug)]
pub struct StatusMessage {
    pub kind: StatusKind,
    pub message: String,
}

/// Header row with scope/sort/name state, hints, and transient status.
pub struct SessionSelectorHeader {
    scope: SessionScope,
    sort_mode: SortMode,
    name_filter: NameFilter,
    request_render: Rc<dyn Fn()>,
    loading: bool,
    load_progress: Option<(usize, usize)>,
    show_path: bool,
    confirming_delete_path: Option<String>,
    status_message: Option<StatusMessage>,
    status_deadline: Option<Instant>,
    show_rename_hint: bool,
}

impl SessionSelectorHeader {
    pub fn new(
        scope: SessionScope,
        sort_mode: SortMode,
        name_filter: NameFilter,
        request_render: Rc<dyn Fn()>,
    ) -> Self {
        Self {
            scope,
            sort_mode,
            name_filter,
            request_render,
            loading: false,
            load_progress: None,
            show_path: false,
            confirming_delete_path: None,
            status_message: None,
            status_deadline: None,
            show_rename_hint: false,
        }
    }

    pub fn set_scope(&mut self, scope: SessionScope) {
        self.scope = scope;
    }

    pub fn set_sort_mode(&mut self, sort_mode: SortMode) {
        self.sort_mode = sort_mode;
    }

    pub fn set_name_filter(&mut self, name_filter: NameFilter) {
        self.name_filter = name_filter;
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
        self.load_progress = None;
    }

    pub fn set_progress(&mut self, loaded: usize, total: usize) {
        self.load_progress = Some((loaded, total));
    }

    pub fn set_show_path(&mut self, show_path: bool) {
        self.show_path = show_path;
    }

    pub fn set_show_rename_hint(&mut self, show: bool) {
        self.show_rename_hint = show;
    }

    pub fn set_confirming_delete_path(&mut self, path: Option<String>) {
        self.confirming_delete_path = path;
    }

    pub fn set_status_message(&mut self, message: Option<StatusMessage>, auto_hide_ms: Option<u64>) {
        self.status_deadline = None;
        self.status_message = message;
        if self.status_message.is_some()
            && let Some(auto_hide_ms) = auto_hide_ms
        {
            self.status_deadline = Some(Instant::now() + Duration::from_millis(auto_hide_ms));
        }
    }

    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_ref().map(|status| status.message.as_str())
    }

    /// Clear the transient status after its auto-hide delay.
    pub fn poll(&mut self, now: Instant) -> bool {
        if let Some(deadline) = self.status_deadline
            && now >= deadline
        {
            self.status_message = None;
            self.status_deadline = None;
            (self.request_render)();
            return true;
        }
        false
    }
}

impl Component for SessionSelectorHeader {
    fn render(&mut self, width: usize) -> Vec<String> {
        let title = match self.scope {
            SessionScope::Current => "Resume Session (Current Folder)",
            SessionScope::All => "Resume Session (All)",
            SessionScope::Archived => "Resume Session (Archived)",
        };
        let left_text = with_theme(|theme| theme.bold(title));

        let sort_label = match self.sort_mode {
            SortMode::Threaded => "Threaded",
            SortMode::Recent => "Recent",
            SortMode::Relevance => "Fuzzy",
        };
        let sort_text = with_theme(|theme| theme.fg(ThemeColor::Muted, "Sort: "))
            + &with_theme(|theme| theme.fg(ThemeColor::Accent, sort_label));

        let name_label = if self.name_filter == NameFilter::All {
            "All"
        } else {
            "Named"
        };
        let name_text = with_theme(|theme| theme.fg(ThemeColor::Muted, "Name: "))
            + &with_theme(|theme| theme.fg(ThemeColor::Accent, name_label));

        let scope_text = if self.loading {
            let progress_text = self
                .load_progress
                .map_or_else(|| "...".to_owned(), |(loaded, total)| format!("{loaded}/{total}"));
            with_theme(|theme| theme.fg(ThemeColor::Muted, "○ Current Folder | "))
                + &with_theme(|theme| theme.fg(ThemeColor::Accent, &format!("Loading {progress_text}")))
        } else {
            match self.scope {
                SessionScope::Current => {
                    with_theme(|theme| theme.fg(ThemeColor::Accent, "◉ Current Folder"))
                        + &with_theme(|theme| theme.fg(ThemeColor::Muted, " | ○ All | ○ Archived"))
                }
                SessionScope::All => {
                    with_theme(|theme| theme.fg(ThemeColor::Muted, "○ Current Folder | "))
                        + &with_theme(|theme| theme.fg(ThemeColor::Accent, "◉ All"))
                        + &with_theme(|theme| theme.fg(ThemeColor::Muted, " | ○ Archived"))
                }
                SessionScope::Archived => {
                    with_theme(|theme| theme.fg(ThemeColor::Muted, "○ Current Folder | ○ All | "))
                        + &with_theme(|theme| theme.fg(ThemeColor::Accent, "◉ Archived"))
                }
            }
        };

        let right_text = truncate(&format!("{scope_text}  {name_text}  {sort_text}"), width, "");
        let available_left = width.saturating_sub(visible_width(&right_text)).saturating_sub(1);
        let left = truncate(&left_text, available_left, "");
        let spacing = width
            .saturating_sub(visible_width(&left))
            .saturating_sub(visible_width(&right_text));

        let (hint_line_1, hint_line_2) = if let Some(_path) = &self.confirming_delete_path {
            let confirm_hint = format!(
                "Delete session? {} · {}",
                key_hint("tui.select.confirm", "confirm"),
                key_hint("tui.select.cancel", "cancel")
            );
            (
                with_theme(|theme| theme.fg(ThemeColor::Error, &truncate(&confirm_hint, width, "…"))),
                String::new(),
            )
        } else if let Some(status) = &self.status_message {
            let color = if status.kind == StatusKind::Error {
                ThemeColor::Error
            } else {
                ThemeColor::Accent
            };
            (
                with_theme(|theme| theme.fg(color, &truncate(&status.message, width, "…"))),
                String::new(),
            )
        } else {
            let path_state = if self.show_path { "(on)" } else { "(off)" };
            let sep = with_theme(|theme| theme.fg(ThemeColor::Muted, " · "));
            let hint_1 = key_hint("tui.input.tab", "scope")
                + &sep
                + &with_theme(|theme| theme.fg(ThemeColor::Muted, "re:<pattern> regex · \"phrase\" exact"));
            let mut hint_2_parts = vec![
                key_hint("app.session.toggleSort", "sort"),
                key_hint("app.session.toggleNamedFilter", "named"),
                key_hint("app.session.delete", "delete"),
                key_hint("app.session.archive", "archive"),
                key_hint("app.session.togglePath", &format!("path {path_state}")),
            ];
            if self.show_rename_hint {
                hint_2_parts.push(key_hint("app.session.rename", "rename"));
            }
            let hint_2 = hint_2_parts.join(&sep);
            (truncate(&hint_1, width, "…"), truncate(&hint_2, width, "…"))
        };

        vec![
            format!("{left}{}{right_text}", " ".repeat(spacing)),
            hint_line_1,
            hint_line_2,
        ]
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

#[derive(Clone, Debug)]
struct SessionTreeNode {
    session: SessionInfo,
    children: Vec<SessionTreeNode>,
}

#[derive(Clone, Debug)]
struct FlatSessionNode {
    session: SessionInfo,
    depth: usize,
    is_last: bool,
    ancestor_continues: Vec<bool>,
}

fn build_session_tree(sessions: &[SessionInfo]) -> Vec<SessionTreeNode> {
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut latest_activity: Vec<i64> = sessions.iter().map(|session| session.modified).collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); sessions.len()];
    for (index, session) in sessions.iter().enumerate() {
        let session_path = canonicalize(Some(&session.path)).unwrap_or_else(|| session.path.clone());
        by_path.insert(session_path, index);
    }

    let mut roots: Vec<usize> = Vec::new();
    for (index, session) in sessions.iter().enumerate() {
        let parent_path = canonicalize(session.parent_session_path.as_deref());
        if let Some(parent_index) = parent_path.and_then(|path| by_path.get(&path).copied()) {
            children[parent_index].push(index);
        } else {
            roots.push(index);
        }
    }

    fn update_latest_activity(index: usize, latest: &mut [i64], children: &[Vec<usize>]) -> i64 {
        let mut activity = latest[index];
        for child in &children[index] {
            activity = activity.max(update_latest_activity(*child, latest, children));
        }
        latest[index] = activity;
        activity
    }

    fn sort_nodes(index: usize, sessions: &[SessionInfo], latest: &[i64], children: &mut [Vec<usize>]) {
        let mut node_children = children[index].clone();
        node_children.sort_by(|left, right| compare_session_ids(*left, *right, sessions, latest));
        for child in &node_children {
            sort_nodes(*child, sessions, latest, children);
        }
        children[index] = node_children;
    }

    fn materialize(index: usize, sessions: &[SessionInfo], children: &[Vec<usize>]) -> SessionTreeNode {
        SessionTreeNode {
            session: sessions[index].clone(),
            children: children[index]
                .iter()
                .map(|child| materialize(*child, sessions, children))
                .collect(),
        }
    }

    for root in &roots {
        update_latest_activity(*root, &mut latest_activity, &children);
    }
    for root in &roots {
        sort_nodes(*root, sessions, &latest_activity, &mut children);
    }
    roots.sort_by(|left, right| compare_session_ids(*left, *right, sessions, &latest_activity));
    roots
        .iter()
        .map(|root| materialize(*root, sessions, &children))
        .collect()
}

fn compare_session_ids(
    left: usize,
    right: usize,
    sessions: &[SessionInfo],
    latest_activity: &[i64],
) -> std::cmp::Ordering {
    let left_named = has_session_name(&sessions[left]);
    let right_named = has_session_name(&sessions[right]);
    if left_named != right_named {
        return if left_named {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        };
    }
    latest_activity[right].cmp(&latest_activity[left])
}

fn flatten_session_tree(roots: &[SessionTreeNode]) -> Vec<FlatSessionNode> {
    let mut result = Vec::new();
    fn walk(
        node: &SessionTreeNode,
        depth: usize,
        ancestor_continues: Vec<bool>,
        is_last: bool,
        result: &mut Vec<FlatSessionNode>,
    ) {
        result.push(FlatSessionNode {
            session: node.session.clone(),
            depth,
            is_last,
            ancestor_continues: ancestor_continues.clone(),
        });
        for (index, child) in node.children.iter().enumerate() {
            let child_is_last = index == node.children.len() - 1;
            let continues = depth > 0 && !is_last;
            let mut child_ancestors = ancestor_continues.clone();
            child_ancestors.push(continues);
            walk(child, depth + 1, child_ancestors, child_is_last, result);
        }
    }
    for (index, root) in roots.iter().enumerate() {
        walk(root, 0, Vec::new(), index == roots.len() - 1, &mut result);
    }
    result
}

/// Callbacks fired by the session list.
#[derive(Default)]
pub struct SessionListCallbacks {
    pub on_select: Option<Rc<dyn Fn(String)>>,
    pub on_cancel: Option<Rc<dyn Fn()>>,
    pub on_exit: Option<Rc<dyn Fn()>>,
    pub on_toggle_scope: Option<Rc<dyn Fn()>>,
    pub on_toggle_sort: Option<Rc<dyn Fn()>>,
    pub on_toggle_name_filter: Option<Rc<dyn Fn()>>,
    pub on_toggle_path: Option<Rc<dyn Fn(bool)>>,
    pub on_delete_confirmation_change: Option<Rc<dyn Fn(Option<String>)>>,
    pub on_delete_session: Option<Rc<dyn Fn(String)>>,
    pub on_archive_session: Option<Rc<dyn Fn(String)>>,
    pub on_rename_session: Option<Rc<dyn Fn(String)>>,
    pub on_error: Option<Rc<dyn Fn(String)>>,
}

/// Custom session list component with multi-line items and search.
pub struct SessionList {
    all_sessions: Vec<SessionInfo>,
    filtered_sessions: Vec<FlatSessionNode>,
    selected_index: usize,
    search_input: Input,
    show_cwd: bool,
    sort_mode: SortMode,
    name_filter: NameFilter,
    keybindings: KeybindingsManager,
    show_path: bool,
    confirming_delete_path: Option<String>,
    current_session_canonical_path: Option<String>,
    max_visible: usize,
    pub callbacks: SessionListCallbacks,
    focused: bool,
}

impl SessionList {
    pub fn new(
        sessions: Vec<SessionInfo>,
        show_cwd: bool,
        sort_mode: SortMode,
        name_filter: NameFilter,
        keybindings: KeybindingsManager,
        current_session_file_path: Option<&str>,
    ) -> Self {
        let mut list = Self {
            all_sessions: sessions,
            filtered_sessions: Vec::new(),
            selected_index: 0,
            search_input: Input::new(InputOptions::default()),
            show_cwd,
            sort_mode,
            name_filter,
            keybindings,
            show_path: false,
            confirming_delete_path: None,
            current_session_canonical_path: canonicalize(current_session_file_path),
            max_visible: 10,
            callbacks: SessionListCallbacks::default(),
            focused: false,
        };
        list.filter_sessions("");
        list
    }

    pub fn get_selected_session_path(&self) -> Option<&str> {
        self.filtered_sessions
            .get(self.selected_index)
            .map(|node| node.session.path.as_str())
    }

    pub fn set_sort_mode(&mut self, sort_mode: SortMode) {
        self.sort_mode = sort_mode;
        let query = self.search_input.get_value().to_owned();
        self.filter_sessions(&query);
    }

    pub fn set_name_filter(&mut self, name_filter: NameFilter) {
        self.name_filter = name_filter;
        let query = self.search_input.get_value().to_owned();
        self.filter_sessions(&query);
    }

    pub fn set_sessions(&mut self, sessions: Vec<SessionInfo>, show_cwd: bool) {
        self.all_sessions = sessions;
        self.show_cwd = show_cwd;
        let query = self.search_input.get_value().to_owned();
        self.filter_sessions(&query);
    }

    pub fn set_confirming_delete_path(&mut self, path: Option<String>) {
        self.confirming_delete_path = path.clone();
        if let Some(callback) = &self.callbacks.on_delete_confirmation_change {
            callback(path);
        }
    }

    pub fn search_value(&self) -> &str {
        self.search_input.get_value()
    }

    fn filter_sessions(&mut self, query: &str) {
        let trimmed = js_trim(query);
        let sessions_with_messages: Vec<SessionInfo> = self
            .all_sessions
            .iter()
            .filter(|session| has_session_messages(session))
            .cloned()
            .collect();
        let name_filtered: Vec<SessionInfo> = match self.name_filter {
            NameFilter::All => sessions_with_messages,
            NameFilter::Named => sessions_with_messages.into_iter().filter(has_session_name).collect(),
        };

        if self.sort_mode == SortMode::Threaded && trimmed.is_empty() {
            let roots = build_session_tree(&name_filtered);
            self.filtered_sessions = flatten_session_tree(&roots);
        } else {
            let filtered = filter_and_sort_sessions(&name_filtered, query, self.sort_mode, NameFilter::All);
            self.filtered_sessions = filtered
                .into_iter()
                .map(|session| FlatSessionNode {
                    session,
                    depth: 0,
                    is_last: true,
                    ancestor_continues: Vec::new(),
                })
                .collect();
        }
        self.selected_index = self.selected_index.min(self.filtered_sessions.len().saturating_sub(1));
    }

    fn start_delete_confirmation_for_selected_session(&mut self) {
        let Some(selected) = self.filtered_sessions.get(self.selected_index) else {
            return;
        };
        if self.is_current_session_path(&selected.session.path) {
            if let Some(on_error) = &self.callbacks.on_error {
                on_error("Cannot delete the currently active session".to_owned());
            }
            return;
        }
        let path = selected.session.path.clone();
        self.set_confirming_delete_path(Some(path));
    }

    fn is_current_session_path(&self, path: &str) -> bool {
        let Some(current) = &self.current_session_canonical_path else {
            return false;
        };
        canonicalize(Some(path)).unwrap_or_else(|| path.to_owned()) == *current
    }

    fn build_tree_prefix(&self, node: &FlatSessionNode) -> String {
        if node.depth == 0 {
            return String::new();
        }
        let parts: String = node
            .ancestor_continues
            .iter()
            .map(|continues| if *continues { "│  " } else { "   " })
            .collect();
        parts + if node.is_last { "└─ " } else { "├─ " }
    }

    pub fn handle_input(&mut self, key_data: &str) {
        let keybindings = get_keybindings();

        if self.confirming_delete_path.is_some() {
            if keybindings.matches(key_data, "tui.select.confirm") {
                if let Some(path) = self.confirming_delete_path.take() {
                    self.set_confirming_delete_path(None);
                    if let Some(on_delete) = &self.callbacks.on_delete_session {
                        on_delete(path);
                    }
                }
                return;
            }
            if keybindings.matches(key_data, "tui.select.cancel") {
                self.set_confirming_delete_path(None);
                return;
            }
            return;
        }

        if keybindings.matches(key_data, "tui.input.tab") {
            if let Some(callback) = &self.callbacks.on_toggle_scope {
                callback();
            }
            return;
        }
        if keybindings.matches(key_data, "app.session.toggleSort") {
            if let Some(callback) = &self.callbacks.on_toggle_sort {
                callback();
            }
            return;
        }
        if self.keybindings.matches(key_data, "app.session.toggleNamedFilter") {
            if let Some(callback) = &self.callbacks.on_toggle_name_filter {
                callback();
            }
            return;
        }
        if keybindings.matches(key_data, "app.session.togglePath") {
            self.show_path = !self.show_path;
            if let Some(callback) = &self.callbacks.on_toggle_path {
                callback(self.show_path);
            }
            return;
        }
        if keybindings.matches(key_data, "app.session.archive") {
            if let Some(selected) = self.filtered_sessions.get(self.selected_index)
                && let Some(callback) = &self.callbacks.on_archive_session
            {
                callback(selected.session.path.clone());
            }
            return;
        }
        if keybindings.matches(key_data, "app.session.delete") {
            self.start_delete_confirmation_for_selected_session();
            return;
        }
        if keybindings.matches(key_data, "app.session.rename") {
            if let Some(selected) = self.filtered_sessions.get(self.selected_index)
                && let Some(callback) = &self.callbacks.on_rename_session
            {
                callback(selected.session.path.clone());
            }
            return;
        }
        if keybindings.matches(key_data, "app.session.deleteNoninvasive") {
            if !self.search_input.get_value().is_empty() {
                self.search_input.handle_input(key_data);
                let query = self.search_input.get_value().to_owned();
                self.filter_sessions(&query);
                return;
            }
            self.start_delete_confirmation_for_selected_session();
            return;
        }
        if keybindings.matches(key_data, "tui.select.up") {
            self.selected_index = self.selected_index.saturating_sub(1);
        } else if keybindings.matches(key_data, "tui.select.down") {
            self.selected_index = (self.selected_index + 1).min(self.filtered_sessions.len().saturating_sub(1));
        } else if keybindings.matches(key_data, "tui.select.pageUp") {
            self.selected_index = self.selected_index.saturating_sub(self.max_visible);
        } else if keybindings.matches(key_data, "tui.select.pageDown") {
            self.selected_index =
                (self.selected_index + self.max_visible).min(self.filtered_sessions.len().saturating_sub(1));
        } else if keybindings.matches(key_data, "tui.select.confirm") {
            if let Some(selected) = self.filtered_sessions.get(self.selected_index)
                && let Some(on_select) = &self.callbacks.on_select
            {
                on_select(selected.session.path.clone());
            }
        } else if keybindings.matches(key_data, "tui.select.cancel") {
            if let Some(on_cancel) = &self.callbacks.on_cancel {
                on_cancel();
            }
        } else {
            self.search_input.handle_input(key_data);
            let query = self.search_input.get_value().to_owned();
            self.filter_sessions(&query);
        }
    }
}

impl Component for SessionList {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        lines.extend(self.search_input.render(width));
        lines.push(String::new());

        if self.filtered_sessions.is_empty() {
            let empty_message = if self.name_filter == NameFilter::Named {
                let toggle_key = key_text("app.session.toggleNamedFilter");
                if self.show_cwd {
                    format!("  No named sessions found. Press {toggle_key} to show all.")
                } else {
                    format!(
                        "  No named sessions in current folder. Press {toggle_key} to show all, or Tab to view all."
                    )
                }
            } else if self.show_cwd {
                "  No sessions found".to_owned()
            } else {
                "  No sessions in current folder. Press Tab to view all.".to_owned()
            };
            lines.push(truncate(
                &with_theme(|theme| theme.fg(ThemeColor::Muted, &empty_message)),
                width,
                "…",
            ));
            return lines;
        }

        let start_index = self
            .selected_index
            .saturating_sub(self.max_visible / 2)
            .min(self.filtered_sessions.len().saturating_sub(self.max_visible));
        let end_index = (start_index + self.max_visible).min(self.filtered_sessions.len());
        let now = now_millis();

        for index in start_index..end_index {
            let node = &self.filtered_sessions[index];
            let session = &node.session;
            let is_selected = index == self.selected_index;
            let is_confirming_delete = Some(session.path.clone()) == self.confirming_delete_path;
            let is_current = self.is_current_session_path(&session.path);

            let prefix = self.build_tree_prefix(node);
            let has_name = session.name.is_some();
            let display_text = session.name.clone().unwrap_or_else(|| session.first_message.clone());
            let normalized_message: String = js_trim(
                &display_text
                    .chars()
                    .map(|character| {
                        let code = character as u32;
                        if code < 0x20 || code == 0x7f { ' ' } else { character }
                    })
                    .collect::<String>(),
            )
            .to_owned();

            let age = format_session_date(session.modified, now);
            let message_count = session.message_count.to_string();
            let mut right_part = format!("{message_count} {age}");
            if self.show_cwd && !session.cwd.is_empty() {
                right_part = format!("{} {right_part}", shorten_path(&session.cwd));
            }
            if self.show_path {
                right_part = format!("{} {right_part}", shorten_path(&session.path));
            }

            let cursor = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "› "))
            } else {
                "  ".to_owned()
            };

            let prefix_width = visible_width(&prefix);
            let right_width = visible_width(&right_part) + 2;
            let available_for_message = (width as isize)
                .saturating_sub(2)
                .saturating_sub(prefix_width as isize)
                .saturating_sub(right_width as isize);
            let truncated_message = truncate(&normalized_message, (available_for_message.max(10)) as usize, "…");

            let message_color = if is_confirming_delete {
                Some(ThemeColor::Error)
            } else if is_current {
                Some(ThemeColor::Accent)
            } else if has_name {
                Some(ThemeColor::Warning)
            } else {
                None
            };
            let mut styled_message = message_color.map_or_else(
                || truncated_message.clone(),
                |color| with_theme(|theme| theme.fg(color, &truncated_message)),
            );
            if is_selected {
                styled_message = with_theme(|theme| theme.bold(&styled_message));
            }

            let left_part = cursor + &with_theme(|theme| theme.fg(ThemeColor::Dim, &prefix)) + &styled_message;
            let left_width = visible_width(&left_part);
            let spacing = width
                .saturating_sub(left_width)
                .saturating_sub(visible_width(&right_part))
                .max(1);
            let styled_right = with_theme(|theme| {
                theme.fg(
                    if is_confirming_delete {
                        ThemeColor::Error
                    } else {
                        ThemeColor::Dim
                    },
                    &right_part,
                )
            });

            let mut line = left_part + &" ".repeat(spacing) + &styled_right;
            if is_selected {
                line = with_theme(|theme| theme.bg(ThemeBg::SelectedBg, &line));
            }
            lines.push(truncate(&line, width, ""));
        }

        if start_index > 0 || end_index < self.filtered_sessions.len() {
            let scroll_text = format!("  ({}/{})", self.selected_index + 1, self.filtered_sessions.len());
            lines.push(with_theme(|theme| {
                theme.fg(ThemeColor::Muted, &truncate(&scroll_text, width, ""))
            }));
        }

        lines
    }

    fn wants_input(&self) -> bool {
        true
    }

    fn invalidate(&mut self) {}

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl Focusable for SessionList {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.search_input.set_focused(focused);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectorMode {
    List,
    Rename,
}

#[derive(Default)]
struct SelectorEvents {
    select: Option<String>,
    cancel: bool,
    exit: bool,
    toggle_scope: bool,
    toggle_sort: bool,
    toggle_name_filter: bool,
    toggle_path: Option<bool>,
    delete_confirmation: Option<Option<String>>,
    error: Option<String>,
    delete_session: Option<String>,
    archive_session: Option<String>,
    rename_session: Option<String>,
}

/// Component that renders a session selector.
pub struct SessionSelectorComponent {
    session_list: SessionList,
    header: SessionSelectorHeader,
    on_select_callback: Rc<dyn Fn(String)>,
    on_cancel_callback: Rc<dyn Fn()>,
    on_exit_callback: Rc<dyn Fn()>,
    scope: SessionScope,
    sort_mode: SortMode,
    name_filter: NameFilter,
    current_sessions: Option<Vec<SessionInfo>>,
    all_sessions: Option<Vec<SessionInfo>>,
    archived_sessions: Option<Vec<SessionInfo>>,
    current_sessions_loader: SessionListLoader,
    all_sessions_loader: SessionListLoader,
    archived_sessions_loader: Option<SessionListLoader>,
    request_render: Rc<dyn Fn()>,
    rename_session: Option<RenameSessionFn>,
    control_db_path: Option<String>,
    current_loading: bool,
    all_loading: bool,
    archived_loading: bool,
    all_load_seq: u64,
    archived_load_seq: u64,
    mode: SelectorMode,
    rename_input: Input,
    rename_target_path: Option<String>,
    focused: bool,
    events: Rc<RefCell<SelectorEvents>>,
    pending_delete: Option<String>,
    pending_archive: Option<String>,
    pending_rename: Option<String>,
    pending_load_scope: bool,
}

impl SessionSelectorComponent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        current_sessions_loader: SessionListLoader,
        all_sessions_loader: SessionListLoader,
        on_select: Rc<dyn Fn(String)>,
        on_cancel: Rc<dyn Fn()>,
        on_exit: Rc<dyn Fn()>,
        request_render: Rc<dyn Fn()>,
        rename_session: Option<RenameSessionFn>,
        show_rename_hint: Option<bool>,
        keybindings: Option<KeybindingsManager>,
        control_db_path: Option<String>,
        archived_sessions_loader: Option<SessionListLoader>,
        current_session_file_path: Option<&str>,
    ) -> Self {
        let keybindings = keybindings.unwrap_or_else(|| KeybindingsManager::new(Default::default(), None));
        let mut session_list = SessionList::new(
            Vec::new(),
            false,
            SortMode::Recent,
            NameFilter::All,
            keybindings,
            current_session_file_path,
        );
        let can_rename = rename_session.is_some();
        let mut header = SessionSelectorHeader::new(
            SessionScope::Current,
            SortMode::Recent,
            NameFilter::All,
            Rc::clone(&request_render),
        );
        header.set_show_rename_hint(show_rename_hint.unwrap_or(can_rename));

        let events = Rc::new(RefCell::new(SelectorEvents::default()));
        let event_sink = Rc::clone(&events);
        let cancel_sink = Rc::clone(&events);
        let exit_sink = Rc::clone(&events);
        session_list.callbacks.on_select = Some(Rc::new(move |path| {
            event_sink.borrow_mut().select = Some(path);
        }));
        session_list.callbacks.on_cancel = Some(Rc::new(move || {
            cancel_sink.borrow_mut().cancel = true;
        }));
        session_list.callbacks.on_exit = Some(Rc::new(move || {
            exit_sink.borrow_mut().exit = true;
        }));
        let toggle_scope_sink = Rc::clone(&events);
        session_list.callbacks.on_toggle_scope = Some(Rc::new(move || {
            toggle_scope_sink.borrow_mut().toggle_scope = true;
        }));
        let toggle_sort_sink = Rc::clone(&events);
        session_list.callbacks.on_toggle_sort = Some(Rc::new(move || {
            toggle_sort_sink.borrow_mut().toggle_sort = true;
        }));
        let toggle_name_sink = Rc::clone(&events);
        session_list.callbacks.on_toggle_name_filter = Some(Rc::new(move || {
            toggle_name_sink.borrow_mut().toggle_name_filter = true;
        }));
        let toggle_path_sink = Rc::clone(&events);
        session_list.callbacks.on_toggle_path = Some(Rc::new(move |show_path| {
            toggle_path_sink.borrow_mut().toggle_path = Some(show_path);
        }));
        let delete_confirm_sink = Rc::clone(&events);
        session_list.callbacks.on_delete_confirmation_change = Some(Rc::new(move |path| {
            delete_confirm_sink.borrow_mut().delete_confirmation = Some(path);
        }));
        let error_sink = Rc::clone(&events);
        session_list.callbacks.on_error = Some(Rc::new(move |message| {
            error_sink.borrow_mut().error = Some(message);
        }));
        let delete_sink = Rc::clone(&events);
        session_list.callbacks.on_delete_session = Some(Rc::new(move |path| {
            delete_sink.borrow_mut().delete_session = Some(path);
        }));
        let archive_sink = Rc::clone(&events);
        session_list.callbacks.on_archive_session = Some(Rc::new(move |path| {
            archive_sink.borrow_mut().archive_session = Some(path);
        }));
        let rename_sink = Rc::clone(&events);
        session_list.callbacks.on_rename_session = Some(Rc::new(move |path| {
            rename_sink.borrow_mut().rename_session = Some(path);
        }));

        Self {
            session_list,
            header,
            on_select_callback: on_select,
            on_cancel_callback: on_cancel,
            on_exit_callback: on_exit,
            scope: SessionScope::Current,
            sort_mode: SortMode::Recent,
            name_filter: NameFilter::All,
            current_sessions: None,
            all_sessions: None,
            archived_sessions: None,
            current_sessions_loader,
            all_sessions_loader,
            archived_sessions_loader,
            request_render,
            rename_session,
            control_db_path,
            current_loading: false,
            all_loading: false,
            archived_loading: false,
            all_load_seq: 0,
            archived_load_seq: 0,
            mode: SelectorMode::List,
            rename_input: Input::new(InputOptions::default()),
            rename_target_path: None,
            focused: false,
            events,
            pending_delete: None,
            pending_archive: None,
            pending_rename: None,
            pending_load_scope: false,
        }
    }

    pub fn session_list(&self) -> &SessionList {
        &self.session_list
    }

    /// `showError`: surface a validation error in the header.
    pub fn show_error(&mut self, message: &str) {
        self.header.set_status_message(
            Some(StatusMessage {
                kind: StatusKind::Error,
                message: message.to_owned(),
            }),
            Some(4000),
        );
        (self.request_render)();
    }

    pub fn header(&self) -> &SessionSelectorHeader {
        &self.header
    }

    pub fn scope(&self) -> SessionScope {
        self.scope
    }

    pub fn sort_mode(&self) -> SortMode {
        self.sort_mode
    }

    pub fn name_filter(&self) -> NameFilter {
        self.name_filter
    }

    /// `loadCurrentSessions`: the source starts this from the constructor.
    pub async fn load_current_sessions(&mut self) {
        self.load_scope(SessionScope::Current, LoadReason::Initial).await;
    }

    async fn load_scope(&mut self, scope: SessionScope, reason: LoadReason) {
        let show_cwd = scope != SessionScope::Current;
        if scope == SessionScope::Archived && self.archived_sessions_loader.is_none() {
            return;
        }

        match scope {
            SessionScope::Current => self.current_loading = true,
            SessionScope::All => self.all_loading = true,
            SessionScope::Archived => self.archived_loading = true,
        }
        let seq = match scope {
            SessionScope::All => {
                self.all_load_seq += 1;
                Some(self.all_load_seq)
            }
            SessionScope::Archived => {
                self.archived_load_seq += 1;
                Some(self.archived_load_seq)
            }
            SessionScope::Current => None,
        };
        self.header.set_scope(scope);
        self.header.set_loading(true);
        (self.request_render)();

        let progress_cell = Rc::new(RefCell::new(None::<(usize, usize)>));
        let progress_sink = Rc::clone(&progress_cell);
        let on_progress: ProgressCallback = Rc::new(move |loaded, total| {
            *progress_sink.borrow_mut() = Some((loaded, total));
        });

        let loader = match scope {
            SessionScope::Current => Rc::clone(&self.current_sessions_loader),
            SessionScope::All => Rc::clone(&self.all_sessions_loader),
            SessionScope::Archived => Rc::clone(self.archived_sessions_loader.as_ref().expect("loader")),
        };
        let result = loader(Some(on_progress)).await;
        if let Some((loaded, total)) = *progress_cell.borrow() {
            self.header.set_progress(loaded, total);
        }

        match result {
            Ok(sessions) => {
                match scope {
                    SessionScope::Current => {
                        self.current_sessions = Some(sessions.clone());
                        self.current_loading = false;
                    }
                    SessionScope::All => {
                        self.all_sessions = Some(sessions.clone());
                        self.all_loading = false;
                    }
                    SessionScope::Archived => {
                        self.archived_sessions = Some(sessions.clone());
                        self.archived_loading = false;
                    }
                }
                if scope != self.scope {
                    return;
                }
                if scope == SessionScope::All && seq != Some(self.all_load_seq) {
                    return;
                }
                if scope == SessionScope::Archived && seq != Some(self.archived_load_seq) {
                    return;
                }
                self.header.set_loading(false);
                self.session_list.set_sessions(sessions, show_cwd);
                (self.request_render)();
            }
            Err(message) => {
                match scope {
                    SessionScope::Current => self.current_loading = false,
                    SessionScope::All => self.all_loading = false,
                    SessionScope::Archived => self.archived_loading = false,
                }
                if scope != self.scope {
                    return;
                }
                if scope == SessionScope::All && seq != Some(self.all_load_seq) {
                    return;
                }
                if scope == SessionScope::Archived && seq != Some(self.archived_load_seq) {
                    return;
                }
                self.header.set_loading(false);
                self.header.set_status_message(
                    Some(StatusMessage {
                        kind: StatusKind::Error,
                        message: format!("Failed to load sessions: {message}"),
                    }),
                    Some(4000),
                );
                if reason == LoadReason::Initial {
                    self.session_list.set_sessions(Vec::new(), show_cwd);
                }
                (self.request_render)();
            }
        }
    }

    fn toggle_sort_mode(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::Recent => SortMode::Relevance,
            SortMode::Relevance => SortMode::Threaded,
            SortMode::Threaded => SortMode::Recent,
        };
        self.header.set_sort_mode(self.sort_mode);
        self.session_list.set_sort_mode(self.sort_mode);
        (self.request_render)();
    }

    fn toggle_name_filter(&mut self) {
        self.name_filter = match self.name_filter {
            NameFilter::All => NameFilter::Named,
            NameFilter::Named => NameFilter::All,
        };
        self.header.set_name_filter(self.name_filter);
        self.session_list.set_name_filter(self.name_filter);
        (self.request_render)();
    }

    async fn refresh_sessions_after_mutation(&mut self) {
        self.load_scope(self.scope, LoadReason::Refresh).await;
    }

    fn toggle_scope(&mut self) -> bool {
        self.scope = match self.scope {
            SessionScope::Current => SessionScope::All,
            SessionScope::All => SessionScope::Archived,
            SessionScope::Archived => SessionScope::Current,
        };
        self.header.set_scope(self.scope);

        let cached_sessions = match self.scope {
            SessionScope::Current => self.current_sessions.clone(),
            SessionScope::All => self.all_sessions.clone(),
            SessionScope::Archived => self.archived_sessions.clone(),
        };
        let loading = match self.scope {
            SessionScope::Current => self.current_loading,
            SessionScope::All => self.all_loading,
            SessionScope::Archived => self.archived_loading,
        };
        if let Some(sessions) = cached_sessions {
            self.header.set_loading(false);
            self.session_list
                .set_sessions(sessions, self.scope != SessionScope::Current);
            (self.request_render)();
            return false;
        }
        self.header.set_loading(loading);
        !loading
    }

    fn enter_rename_mode(&mut self, session_path: &str, current_name: Option<&str>) {
        self.mode = SelectorMode::Rename;
        self.rename_target_path = Some(session_path.to_owned());
        self.rename_input.set_value(current_name.unwrap_or_default());
        self.rename_input.set_focused(true);
    }

    fn exit_rename_mode(&mut self) {
        self.mode = SelectorMode::List;
        self.rename_target_path = None;
        (self.request_render)();
    }

    /// `confirmRename`: save the trimmed name (`None` when empty), refresh, then leave rename mode.
    pub async fn confirm_rename(&mut self, value: &str) -> Result<(), String> {
        let next = js_trim(value).to_owned();
        let Some(target) = self.rename_target_path.clone() else {
            self.exit_rename_mode();
            return Ok(());
        };
        let Some(rename_session) = self.rename_session.clone() else {
            self.exit_rename_mode();
            return Ok(());
        };
        let label = (!next.is_empty()).then_some(next);
        let result = rename_session(target, label).await;
        if result.is_ok() {
            self.refresh_sessions_after_mutation().await;
        }
        self.exit_rename_mode();
        result
    }

    /// Delete a session file, trying `trash` first and falling back to `unlink`.
    pub async fn delete_session(&mut self, session_path: &str) {
        let result = delete_session_file(session_path).await;
        if result.ok {
            if let Some(control_db_path) = &self.control_db_path {
                let _ = remove_session_metadata(control_db_path, session_path);
            }
            if let Some(current) = &mut self.current_sessions {
                current.retain(|session| session.path != session_path);
            }
            if let Some(all) = &mut self.all_sessions {
                all.retain(|session| session.path != session_path);
            }
            let sessions = match self.scope {
                SessionScope::Current => self.current_sessions.clone().unwrap_or_default(),
                SessionScope::All => self.all_sessions.clone().unwrap_or_default(),
                SessionScope::Archived => self.archived_sessions.clone().unwrap_or_default(),
            };
            self.session_list
                .set_sessions(sessions, self.scope != SessionScope::Current);
            let message = if result.method == DeleteMethod::Trash {
                "Session moved to trash"
            } else {
                "Session deleted"
            };
            self.header.set_status_message(
                Some(StatusMessage {
                    kind: StatusKind::Info,
                    message: message.to_owned(),
                }),
                Some(2000),
            );
            self.refresh_sessions_after_mutation().await;
        } else {
            let error = result.error.unwrap_or_else(|| "Unknown error".to_owned());
            self.header.set_status_message(
                Some(StatusMessage {
                    kind: StatusKind::Error,
                    message: format!("Failed to delete: {error}"),
                }),
                Some(3000),
            );
        }
        (self.request_render)();
    }

    /// Archive the selected session and refresh the current scope.
    pub async fn archive_selected_session(&mut self, session_path: &str) {
        let Some(control_db_path) = self.control_db_path.clone() else {
            self.header.set_status_message(
                Some(StatusMessage {
                    kind: StatusKind::Error,
                    message: "Session archive requires a control database.".to_owned(),
                }),
                Some(3000),
            );
            (self.request_render)();
            return;
        };
        let selected = self
            .current_sessions
            .iter()
            .chain(self.all_sessions.iter())
            .chain(self.archived_sessions.iter())
            .flat_map(|sessions| sessions.iter())
            .find(|session| session.path == session_path);
        if selected.is_some_and(|session| session.is_archived.unwrap_or(false)) {
            self.header.set_status_message(
                Some(StatusMessage {
                    kind: StatusKind::Info,
                    message: "Session is already archived.".to_owned(),
                }),
                Some(2000),
            );
            (self.request_render)();
            return;
        }
        let _ = archive_session(&control_db_path, session_path);
        self.header.set_status_message(
            Some(StatusMessage {
                kind: StatusKind::Info,
                message: "Session archived".to_owned(),
            }),
            Some(2000),
        );
        self.refresh_sessions_after_mutation().await;
        (self.request_render)();
    }

    /// Execute the async action queued by the last `handle_input` (delete/archive/scope load).
    pub async fn run_pending_action(&mut self) {
        if let Some(value) = self.pending_rename.take() {
            let _ = self.confirm_rename(&value).await;
            return;
        }
        if let Some(path) = self.pending_delete.take() {
            self.delete_session(&path).await;
            return;
        }
        if let Some(path) = self.pending_archive.take() {
            self.archive_selected_session(&path).await;
            return;
        }
        if std::mem::take(&mut self.pending_load_scope) {
            let scope = self.scope;
            self.load_scope(scope, LoadReason::Toggle).await;
        }
    }

    pub fn has_pending_action(&self) -> bool {
        self.pending_delete.is_some()
            || self.pending_archive.is_some()
            || self.pending_rename.is_some()
            || self.pending_load_scope
    }

    /// Clear transient header status after its auto-hide delay.
    pub fn poll(&mut self, now: Instant) -> bool {
        self.header.poll(now)
    }

    fn drain_events(&mut self) {
        let mut events = std::mem::take(&mut *self.events.borrow_mut());
        if let Some(path) = events.select.take() {
            self.header.set_status_message(None, None);
            (self.on_select_callback)(path);
        }
        if events.cancel {
            self.header.set_status_message(None, None);
            (self.on_cancel_callback)();
        }
        if events.exit {
            self.header.set_status_message(None, None);
            (self.on_exit_callback)();
        }
        if events.toggle_scope && self.toggle_scope() {
            self.pending_load_scope = true;
        }
        if events.toggle_sort {
            self.toggle_sort_mode();
        }
        if events.toggle_name_filter {
            self.toggle_name_filter();
        }
        if let Some(show_path) = events.toggle_path {
            self.header.set_show_path(show_path);
            (self.request_render)();
        }
        if let Some(path) = events.delete_confirmation {
            self.header.set_confirming_delete_path(path);
            (self.request_render)();
        }
        if let Some(message) = events.error {
            self.header.set_status_message(
                Some(StatusMessage {
                    kind: StatusKind::Error,
                    message,
                }),
                Some(3000),
            );
            (self.request_render)();
        }
        if let Some(path) = events.rename_session {
            self.handle_rename_request(&path);
        }
        if let Some(path) = events.delete_session {
            self.pending_delete = Some(path);
        }
        if let Some(path) = events.archive_session {
            self.pending_archive = Some(path);
        }
    }

    fn handle_rename_request(&mut self, session_path: &str) {
        if self.rename_session.is_none() {
            return;
        }
        if (self.scope == SessionScope::Current && self.current_loading)
            || (self.scope == SessionScope::All && self.all_loading)
            || (self.scope == SessionScope::Archived && self.archived_loading)
        {
            return;
        }
        let sessions = match self.scope {
            SessionScope::Current => self.current_sessions.as_deref().unwrap_or(&[]),
            SessionScope::All => self.all_sessions.as_deref().unwrap_or(&[]),
            SessionScope::Archived => self.archived_sessions.as_deref().unwrap_or(&[]),
        };
        let current_name = sessions
            .iter()
            .find(|session| session.path == session_path)
            .and_then(|session| session.name.clone());
        self.enter_rename_mode(session_path, current_name.as_deref());
    }
}

impl Component for SessionSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(String::new());
        lines.push(with_theme(|theme| {
            theme.fg(ThemeColor::Accent, &"─".repeat(width.max(1)))
        }));
        lines.push(String::new());
        if self.mode == SelectorMode::List {
            lines.extend(self.header.render(width));
            lines.push(String::new());
            lines.extend(self.session_list.render(width));
        } else {
            lines.push(with_theme(|theme| theme.bold("Rename Session")));
            lines.push(String::new());
            lines.extend(self.rename_input.render(width));
            lines.push(String::new());
            lines.push(with_theme(|theme| {
                theme.fg(
                    ThemeColor::Muted,
                    &format!(
                        "{} to save · empty to unname · {} to cancel",
                        key_text("tui.select.confirm"),
                        key_text("tui.select.cancel")
                    ),
                )
            }));
        }
        lines.push(String::new());
        lines.push(with_theme(|theme| {
            theme.fg(ThemeColor::Accent, &"─".repeat(width.max(1)))
        }));
        lines
    }

    fn handle_input(&mut self, data: &str) {
        if self.mode == SelectorMode::Rename {
            let keybindings = get_keybindings();
            if keybindings.matches(data, "tui.select.cancel") {
                self.exit_rename_mode();
                return;
            }
            if keybindings.matches(data, "tui.select.confirm") {
                self.pending_rename = Some(self.rename_input.get_value().to_owned());
                return;
            }
            self.rename_input.handle_input(data);
            return;
        }
        self.session_list.handle_input(data);
        self.drain_events();
    }

    fn invalidate(&mut self) {
        self.session_list.invalidate();
        self.header.invalidate();
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl Focusable for SessionSelectorComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.session_list.set_focused(focused);
        self.rename_input.set_focused(focused);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeleteMethod {
    Trash,
    Unlink,
}

struct DeleteResult {
    ok: bool,
    method: DeleteMethod,
    error: Option<String>,
}

async fn delete_session_file(session_path: &str) -> DeleteResult {
    let mut command = tokio::process::Command::new("trash");
    if session_path.starts_with('-') {
        command.arg("--");
    }
    command.arg(session_path);
    let trash_result = command.output().await;

    let mut trash_error: Option<String> = None;
    let status = match &trash_result {
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let mut parts = Vec::new();
            if !stderr.is_empty() {
                parts.push(stderr.lines().next().unwrap_or(&stderr).to_owned());
            }
            if !parts.is_empty() {
                let joined = parts.join(" · ");
                trash_error = Some(format!("trash: {}", slice_chars(&joined, 200)));
            }
            Some(output.status)
        }
        Err(error) => {
            let message = node_spawn_error_message("spawnSync", &pi_js_compat::js_json::JsString::from("trash"), error);
            trash_error = Some(format!("trash: {}", slice_chars(&message, 200)));
            None
        }
    };

    let exists = Path::new(session_path).exists();
    if status.is_some_and(|status| status.success()) || !exists {
        return DeleteResult {
            ok: true,
            method: DeleteMethod::Trash,
            error: None,
        };
    }

    match tokio::fs::remove_file(session_path).await {
        Ok(()) => DeleteResult {
            ok: true,
            method: DeleteMethod::Unlink,
            error: None,
        },
        Err(error) => {
            let unlink_error = error.to_string();
            let error = match trash_error {
                Some(hint) => format!("{unlink_error} ({hint})"),
                None => unlink_error,
            };
            DeleteResult {
                ok: false,
                method: DeleteMethod::Unlink,
                error: Some(error),
            }
        }
    }
}

fn slice_chars(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}
