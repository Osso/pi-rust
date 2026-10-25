//! Ported from `coding-agent/src/modes/interactive/components/session-selector.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use pi_ai::request::BoxFuture;
use pi_tui::components::dynamic_border::DynamicBorder;
use pi_tui::components::input::{Input, InputOptions};
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::{Component, ComponentRef, Container, Focusable, component};
use pi_tui::utils::{truncate_to_width_with, visible_width};
use pi_tui::{Spacer, Text};

use super::keybinding_hints::{key_hint, key_text};
use super::session_selector_search::{
    NameFilter, SortMode, filter_and_sort_sessions, has_session_messages, has_session_name,
};
use crate::core::keybindings::KeybindingsManager;
use crate::core::session_archive_storage::archive_persisted_session;
use crate::core::session_delete::{SessionDeleteResult, delete_session_tree};
use crate::core::session_manager::SessionInfo;
use crate::modes::interactive::theme::{ThemeBg, ThemeColor, with_theme};
use crate::utils::paths::canonicalize_path;
use crate::utils::uncaught::{ERROR, throw_uncaught};

/// `(loaded, total)` progress callback.
pub type ProgressCallback = Rc<dyn Fn(f64, f64)>;

/// Async session-list loader (`SessionListProgress => Promise<SessionInfo[]>`).
///
/// The interactive TUI runs on a single-threaded local set, so the loader
/// future carries no `Send` bound (a TS promise has none either).
pub type SessionListLoader =
    Rc<dyn Fn(Option<ProgressCallback>) -> Pin<Box<dyn Future<Output = Result<Vec<SessionInfo>, JsString>> + 'static>>>;

/// Async rename callback (`(sessionPath, currentName) => Promise<void>`).
pub type RenameSessionFn = Rc<dyn Fn(JsString, Option<JsString>) -> BoxFuture<'static, Result<(), JsString>>>;

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

fn selector_border() -> ComponentRef {
    component(DynamicBorder::new(Rc::new(|text| {
        with_theme(|theme| theme.fg(ThemeColor::Accent, text))
    })))
}

fn truncate(text: &JsString, width: usize, ellipsis: &str) -> JsString {
    truncate_to_width_with(text, width, ellipsis, false)
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

fn shorten_path(path: &JsString) -> Result<JsString, pi_js_compat::node_error::NativeNodeError> {
    let home = pi_js_compat::node_os::homedir()?;
    if !path.is_empty() && path.starts_with(&home) {
        return Ok(js_format!("~{}", path.slice(home.len(), path.len())));
    }
    Ok(path.to_owned())
}

fn format_session_date(modified_ms: f64, now_ms: i64) -> JsString {
    let diff_ms = now_ms as f64 - modified_ms;
    let diff_mins = (diff_ms / 60_000.0).floor();
    let diff_hours = (diff_ms / 3_600_000.0).floor();
    let diff_days = (diff_ms / 86_400_000.0).floor();
    if diff_mins < 1.0 {
        return JsString::from("now");
    }
    if diff_mins < 60.0 {
        return js_format!("{}m", pi_js_compat::number::js_number_to_string(diff_mins));
    }
    if diff_hours < 24.0 {
        return js_format!("{}h", pi_js_compat::number::js_number_to_string(diff_hours));
    }
    if diff_days < 7.0 {
        return js_format!("{}d", pi_js_compat::number::js_number_to_string(diff_days));
    }
    if diff_days < 30.0 {
        return js_format!(
            "{}w",
            pi_js_compat::number::js_number_to_string((diff_days / 7.0).floor())
        );
    }
    if diff_days < 365.0 {
        return js_format!(
            "{}mo",
            pi_js_compat::number::js_number_to_string((diff_days / 30.0).floor())
        );
    }
    js_format!(
        "{}y",
        pi_js_compat::number::js_number_to_string((diff_days / 365.0).floor())
    )
}

fn canonicalize(path: Option<&JsString>) -> Option<JsString> {
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
    pub message: JsString,
}

/// Header row with scope/sort/name state, hints, and transient status.
pub struct SessionSelectorHeader {
    scope: SessionScope,
    sort_mode: SortMode,
    name_filter: NameFilter,
    request_render: Rc<dyn Fn()>,
    loading: bool,
    load_progress: Option<(f64, f64)>,
    show_path: bool,
    confirming_delete_path: Option<JsString>,
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

    pub fn set_progress(&mut self, loaded: f64, total: f64) {
        self.load_progress = Some((loaded, total));
    }

    pub fn set_show_path(&mut self, show_path: bool) {
        self.show_path = show_path;
    }

    pub fn set_show_rename_hint(&mut self, show: bool) {
        self.show_rename_hint = show;
    }

    pub fn set_confirming_delete_path(&mut self, path: Option<JsString>) {
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

    pub fn status_message(&self) -> Option<&JsString> {
        self.status_message.as_ref().map(|status| &status.message)
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
    fn render(&mut self, width: usize) -> Vec<JsString> {
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
            let progress_text = self.load_progress.map_or_else(
                || JsString::from("..."),
                |(loaded, total)| {
                    js_format!(
                        "{}/{}",
                        pi_js_compat::number::js_number_to_string(loaded),
                        pi_js_compat::number::js_number_to_string(total)
                    )
                },
            );
            with_theme(|theme| theme.fg(ThemeColor::Muted, "○ Current Folder | "))
                + &with_theme(|theme| theme.fg(ThemeColor::Accent, &js_format!("Loading {}", progress_text)))
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

        let right_text = truncate(&js_format!("{}  {}  {}", scope_text, name_text, sort_text), width, "");
        let available_left = width.saturating_sub(visible_width(&right_text)).saturating_sub(1);
        let left = truncate(&left_text, available_left, "");
        let spacing = width
            .saturating_sub(visible_width(&left))
            .saturating_sub(visible_width(&right_text));

        let (hint_line_1, hint_line_2) = if let Some(_path) = &self.confirming_delete_path {
            let confirm_hint = js_format!(
                "Delete session? {} · {}",
                key_hint("tui.select.confirm", "confirm"),
                key_hint("tui.select.cancel", "cancel")
            );
            (
                with_theme(|theme| theme.fg(ThemeColor::Error, truncate(&confirm_hint, width, "…"))),
                JsString::default(),
            )
        } else if let Some(status) = &self.status_message {
            let color = if status.kind == StatusKind::Error {
                ThemeColor::Error
            } else {
                ThemeColor::Accent
            };
            (
                with_theme(|theme| theme.fg(color, truncate(&status.message, width, "…"))),
                JsString::default(),
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
                key_hint("app.session.togglePath", &js_format!("path {}", path_state)),
            ];
            if self.show_rename_hint {
                hint_2_parts.push(key_hint("app.session.rename", "rename"));
            }
            let hint_2 = hint_2_parts
                .into_iter()
                .reduce(|left, right| left + &sep + right)
                .unwrap_or_default();
            (truncate(&hint_1, width, "…"), truncate(&hint_2, width, "…"))
        };

        vec![
            js_format!("{}{}{}", left, " ".repeat(spacing), right_text),
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
    let mut by_path: HashMap<JsString, usize> = HashMap::new();
    let mut latest_activity: Vec<f64> = sessions.iter().map(|session| session.modified).collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); sessions.len()];
    for (index, session) in sessions.iter().enumerate() {
        let session_path = canonicalize(Some(&session.path)).unwrap_or_else(|| session.path.clone());
        by_path.insert(session_path, index);
    }

    let mut roots: Vec<usize> = Vec::new();
    for session in sessions {
        let session_path = canonicalize(Some(&session.path)).unwrap_or_else(|| session.path.clone());
        let index = by_path[&session_path];
        let parent_path = canonicalize(session.parent_session_path.as_ref());
        if let Some(parent_index) = parent_path.and_then(|path| by_path.get(&path).copied()) {
            children[parent_index].push(index);
        } else {
            roots.push(index);
        }
    }

    fn update_latest_activity(index: usize, latest: &mut [f64], children: &[Vec<usize>]) -> f64 {
        let mut activity = latest[index];
        for child in &children[index] {
            let child_activity = update_latest_activity(*child, latest, children);
            activity = if activity.is_nan() || child_activity.is_nan() {
                f64::NAN
            } else {
                activity.max(child_activity)
            };
        }
        latest[index] = activity;
        activity
    }

    fn sort_nodes(index: usize, sessions: &[SessionInfo], latest: &[f64], children: &mut [Vec<usize>]) {
        let mut node_children = children[index].clone();
        pi_js_compat::array_sort::js_array_sort(&mut node_children, |left, right| {
            compare_session_ids(*left, *right, sessions, latest)
        });
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
    pi_js_compat::array_sort::js_array_sort(&mut roots, |left, right| {
        compare_session_ids(*left, *right, sessions, &latest_activity)
    });
    roots
        .iter()
        .map(|root| materialize(*root, sessions, &children))
        .collect()
}

fn compare_session_ids(left: usize, right: usize, sessions: &[SessionInfo], latest_activity: &[f64]) -> f64 {
    let left_named = has_session_name(&sessions[left]);
    let right_named = has_session_name(&sessions[right]);
    if left_named != right_named {
        return if left_named { -1.0 } else { 1.0 };
    }
    latest_activity[right] - latest_activity[left]
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
    pub on_select: Option<Rc<dyn Fn(JsString)>>,
    pub on_cancel: Option<Rc<dyn Fn()>>,
    pub on_exit: Option<Rc<dyn Fn()>>,
    pub on_toggle_scope: Option<Rc<dyn Fn()>>,
    pub on_toggle_sort: Option<Rc<dyn Fn()>>,
    pub on_toggle_name_filter: Option<Rc<dyn Fn()>>,
    pub on_toggle_path: Option<Rc<dyn Fn(bool)>>,
    pub on_delete_confirmation_change: Option<Rc<dyn Fn(Option<JsString>)>>,
    pub on_delete_session: Option<Rc<dyn Fn(JsString)>>,
    pub on_archive_session: Option<Rc<dyn Fn(JsString)>>,
    pub on_rename_session: Option<Rc<dyn Fn(JsString)>>,
    pub on_error: Option<Rc<dyn Fn(JsString)>>,
}

/// Custom session list component with multi-line items and search.
pub struct SessionList {
    all_sessions: Vec<SessionInfo>,
    filtered_sessions: Vec<FlatSessionNode>,
    selected_index: isize,
    search_input: Input,
    show_cwd: bool,
    sort_mode: SortMode,
    name_filter: NameFilter,
    keybindings: KeybindingsManager,
    show_path: bool,
    confirming_delete_path: Option<JsString>,
    current_session_canonical_path: Option<JsString>,
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
        current_session_file_path: Option<&JsString>,
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
        list.filter_sessions(&JsString::default());
        list
    }

    fn selected_node(&self) -> Option<&FlatSessionNode> {
        let index = usize::try_from(self.selected_index).ok()?;
        self.filtered_sessions.get(index)
    }

    pub fn get_selected_session_path(&self) -> Option<&JsString> {
        self.selected_node().map(|node| &node.session.path)
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

    pub fn set_confirming_delete_path(&mut self, path: Option<JsString>) {
        self.confirming_delete_path = path.clone();
        if let Some(callback) = &self.callbacks.on_delete_confirmation_change {
            callback(path);
        }
    }

    pub fn search_value(&self) -> &JsString {
        self.search_input.get_value()
    }

    fn filter_sessions(&mut self, query: &JsString) {
        let trimmed = query.trim();
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
        self.selected_index = self
            .selected_index
            .min(self.filtered_sessions.len().saturating_sub(1) as isize);
    }

    fn start_delete_confirmation_for_selected_session(&mut self) {
        let Some(selected) = self.selected_node() else {
            return;
        };
        if self.is_current_session_path(&selected.session.path) {
            if let Some(on_error) = &self.callbacks.on_error {
                on_error(JsString::from("Cannot delete the currently active session"));
            }
            return;
        }
        let path = selected.session.path.clone();
        self.set_confirming_delete_path(Some(path));
    }

    fn is_current_session_path(&self, path: &JsString) -> bool {
        let Some(current) = &self.current_session_canonical_path else {
            return false;
        };
        canonicalize(Some(path)).unwrap_or_else(|| path.to_owned()) == *current
    }

    fn build_tree_prefix(&self, node: &FlatSessionNode) -> JsString {
        if node.depth == 0 {
            return JsString::default();
        }
        let parts: JsString = JsString::join(
            node.ancestor_continues
                .iter()
                .map(|continues| if *continues { "│  " } else { "   " }),
            "",
        );
        parts + if node.is_last { "└─ " } else { "├─ " }
    }

    pub fn handle_input(&mut self, key_data: &JsString) {
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
            if let Some(selected) = self.selected_node()
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
            if let Some(selected) = self.selected_node()
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
            self.selected_index = (self.selected_index - 1).max(0);
        } else if keybindings.matches(key_data, "tui.select.down") {
            self.selected_index = (self.selected_index + 1).min(self.filtered_sessions.len() as isize - 1);
        } else if keybindings.matches(key_data, "tui.select.pageUp") {
            self.selected_index = (self.selected_index - self.max_visible as isize).max(0);
        } else if keybindings.matches(key_data, "tui.select.pageDown") {
            self.selected_index =
                (self.selected_index + self.max_visible as isize).min(self.filtered_sessions.len() as isize - 1);
        } else if keybindings.matches(key_data, "tui.select.confirm") {
            if let Some(selected) = self.selected_node()
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

impl SessionList {
    fn render_rows(&mut self, width: usize) -> Result<Vec<JsString>, pi_js_compat::node_error::NativeNodeError> {
        let mut lines = Vec::new();
        lines.extend(self.search_input.render(width));
        lines.push(JsString::default());

        if self.filtered_sessions.is_empty() {
            let empty_message = if self.name_filter == NameFilter::Named {
                let toggle_key = key_text("app.session.toggleNamedFilter");
                if self.show_cwd {
                    js_format!("  No named sessions found. Press {} to show all.", toggle_key)
                } else {
                    js_format!(
                        "  No named sessions in current folder. Press {} to show all, or Tab to view all.",
                        toggle_key
                    )
                }
            } else if self.show_cwd {
                JsString::from("  No sessions found")
            } else {
                JsString::from("  No sessions in current folder. Press Tab to view all.")
            };
            lines.push(with_theme(|theme| {
                theme.fg(ThemeColor::Muted, truncate(&empty_message, width, "…"))
            }));
            return Ok(lines);
        }

        let start_index = (self.selected_index - (self.max_visible / 2) as isize)
            .min(self.filtered_sessions.len() as isize - self.max_visible as isize)
            .max(0) as usize;
        let end_index = (start_index + self.max_visible).min(self.filtered_sessions.len());
        let now = now_millis();

        for index in start_index..end_index {
            let node = &self.filtered_sessions[index];
            let session = &node.session;
            let is_selected = index as isize == self.selected_index;
            let is_confirming_delete = Some(session.path.clone()) == self.confirming_delete_path;
            let is_current = self.is_current_session_path(&session.path);

            let prefix = self.build_tree_prefix(node);
            let has_name = session.name.as_ref().is_some_and(|name| !name.is_empty());
            let display_text = session.name.clone().unwrap_or_else(|| session.first_message.clone());
            let normalized_message = JsString::from_utf16(
                display_text
                    .units()
                    .iter()
                    .map(|unit| if *unit < 0x20 || *unit == 0x7f { 0x20 } else { *unit })
                    .collect(),
            )
            .trim();

            let age = format_session_date(session.modified, now);
            let message_count = session.message_count.to_string();
            let mut right_part = js_format!("{} {}", message_count, age);
            if self.show_cwd && !session.cwd.is_empty() {
                right_part = js_format!("{} {}", shorten_path(&session.cwd)?, right_part);
            }
            if self.show_path {
                right_part = js_format!("{} {}", shorten_path(&session.path)?, right_part);
            }

            let cursor = if is_selected {
                with_theme(|theme| theme.fg(ThemeColor::Accent, "› "))
            } else {
                JsString::from("  ")
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
            lines.push(truncate(&line, width, "..."));
        }

        if start_index > 0 || end_index < self.filtered_sessions.len() {
            let scroll_text = js_format!(
                "  ({}/{})",
                (self.selected_index + 1) as i64,
                self.filtered_sessions.len()
            );
            lines.push(with_theme(|theme| {
                theme.fg(ThemeColor::Muted, truncate(&scroll_text, width, ""))
            }));
        }

        Ok(lines)
    }
}

impl Component for SessionList {
    fn handle_input(&mut self, data: &JsString) {
        SessionList::handle_input(self, data);
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.render_rows(width)
            .unwrap_or_else(|error| crate::utils::uncaught::throw_native_uncaught(error))
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
    select: Option<JsString>,
    cancel: bool,
    exit: bool,
    toggle_scope: bool,
    toggle_sort: bool,
    toggle_name_filter: bool,
    toggle_path: Option<bool>,
    delete_confirmation: Option<Option<JsString>>,
    error: Option<JsString>,
    delete_session: Option<JsString>,
    archive_session: Option<JsString>,
    rename_session: Option<JsString>,
}

#[derive(Clone, Copy)]
enum LoadContinuation {
    None,
    Render,
    ExitRename,
}
struct LoadContext {
    scope: SessionScope,
    reason: LoadReason,
    seq: Option<u64>,
    continuation: LoadContinuation,
}
enum SelectorCompletion {
    Load(LoadContext, Result<Vec<SessionInfo>, JsString>),
    Rename(Result<(), JsString>),
    Delete(JsString, SessionDeleteResult),
    Archive(Result<(), JsString>),
}
type SelectorTask = Pin<Box<dyn Future<Output = SelectorCompletion>>>;

/// Frozen selector with independent driver-polled promises and inherited children.
pub struct SessionSelectorComponent {
    container: Container,
    session_list: Rc<RefCell<SessionList>>,
    header: Rc<RefCell<SessionSelectorHeader>>,
    on_select_callback: Rc<dyn Fn(JsString)>,
    on_cancel_callback: Rc<dyn Fn()>,
    on_exit_callback: Rc<dyn Fn()>,
    scope: Rc<Cell<SessionScope>>,
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
    control_db_path: Option<JsString>,
    current_loading: bool,
    all_loading: bool,
    archived_loading: bool,
    all_load_seq: Rc<Cell<u64>>,
    archived_load_seq: Rc<Cell<u64>>,
    mode: SelectorMode,
    rename_input: Rc<RefCell<Input>>,
    rename_target_path: Option<JsString>,
    focused: bool,
    events: Rc<RefCell<SelectorEvents>>,
    tasks: Vec<SelectorTask>,
}

impl SessionSelectorComponent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        current_sessions_loader: SessionListLoader,
        all_sessions_loader: SessionListLoader,
        on_select: Rc<dyn Fn(JsString)>,
        on_cancel: Rc<dyn Fn()>,
        on_exit: Rc<dyn Fn()>,
        request_render: Rc<dyn Fn()>,
        rename_session: Option<RenameSessionFn>,
        show_rename_hint: Option<bool>,
        keybindings: Option<KeybindingsManager>,
        control_db_path: Option<JsString>,
        archived_sessions_loader: Option<SessionListLoader>,
        current_session_file_path: Option<&JsString>,
    ) -> Self {
        let keybindings = keybindings.unwrap_or_else(|| KeybindingsManager::new(Default::default(), None));
        let session_list = SessionList::new(
            Vec::new(),
            false,
            SortMode::Recent,
            NameFilter::All,
            keybindings,
            current_session_file_path,
        );
        let mut header = SessionSelectorHeader::new(
            SessionScope::Current,
            SortMode::Recent,
            NameFilter::All,
            request_render.clone(),
        );
        header.set_show_rename_hint(show_rename_hint.unwrap_or(rename_session.is_some()));
        let events = Rc::new(RefCell::new(SelectorEvents::default()));
        let mut selector = Self {
            container: Container::new(),
            session_list: Rc::new(RefCell::new(session_list)),
            header: Rc::new(RefCell::new(header)),
            on_select_callback: on_select,
            on_cancel_callback: on_cancel,
            on_exit_callback: on_exit,
            scope: Rc::new(Cell::new(SessionScope::Current)),
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
            all_load_seq: Rc::new(Cell::new(0)),
            archived_load_seq: Rc::new(Cell::new(0)),
            mode: SelectorMode::List,
            rename_input: Rc::new(RefCell::new(Input::new(InputOptions::default()))),
            rename_target_path: None,
            focused: false,
            events,
            tasks: Vec::new(),
        };
        selector.install_list_callbacks();
        selector.build_base_layout();
        selector.begin_load_scope(SessionScope::Current, LoadReason::Initial, LoadContinuation::None);
        selector
    }

    fn install_list_callbacks(&mut self) {
        let mut list = self.session_list.borrow_mut();
        let events = self.events.clone();
        list.callbacks.on_select = Some(Rc::new(move |path| events.borrow_mut().select = Some(path)));
        let events = self.events.clone();
        list.callbacks.on_cancel = Some(Rc::new(move || events.borrow_mut().cancel = true));
        let events = self.events.clone();
        list.callbacks.on_exit = Some(Rc::new(move || events.borrow_mut().exit = true));
        let events = self.events.clone();
        list.callbacks.on_toggle_scope = Some(Rc::new(move || events.borrow_mut().toggle_scope = true));
        let events = self.events.clone();
        list.callbacks.on_toggle_sort = Some(Rc::new(move || events.borrow_mut().toggle_sort = true));
        let events = self.events.clone();
        list.callbacks.on_toggle_name_filter = Some(Rc::new(move || events.borrow_mut().toggle_name_filter = true));
        let events = self.events.clone();
        list.callbacks.on_toggle_path = Some(Rc::new(move |show| events.borrow_mut().toggle_path = Some(show)));
        let events = self.events.clone();
        list.callbacks.on_delete_confirmation_change = Some(Rc::new(move |path| {
            events.borrow_mut().delete_confirmation = Some(path)
        }));
        let events = self.events.clone();
        list.callbacks.on_error = Some(Rc::new(move |message| events.borrow_mut().error = Some(message)));
        let events = self.events.clone();
        list.callbacks.on_delete_session = Some(Rc::new(move |path| events.borrow_mut().delete_session = Some(path)));
        let events = self.events.clone();
        list.callbacks.on_archive_session = Some(Rc::new(move |path| events.borrow_mut().archive_session = Some(path)));
        let events = self.events.clone();
        list.callbacks.on_rename_session = Some(Rc::new(move |path| events.borrow_mut().rename_session = Some(path)));
    }

    fn build_base_layout(&mut self) {
        self.container.clear();
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(selector_border());
        self.container.add_child(component(Spacer::new(1)));
        if self.mode == SelectorMode::List {
            self.container.add_child(self.header.clone());
            self.container.add_child(component(Spacer::new(1)));
            self.container.add_child(self.session_list.clone());
        } else {
            self.container.add_child(component(self.rename_panel()));
        }
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(selector_border());
    }

    fn rename_panel(&self) -> Container {
        let mut panel = Container::new();
        panel.add_child(component(Text::new(
            with_theme(|theme| theme.bold("Rename Session")),
            1,
            0,
        )));
        panel.add_child(component(Spacer::new(1)));
        panel.add_child(self.rename_input.clone());
        panel.add_child(component(Spacer::new(1)));
        let hint = js_format!(
            "{} to save · empty to unname · {} to cancel",
            key_text("tui.select.confirm"),
            key_text("tui.select.cancel")
        );
        panel.add_child(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Muted, &hint)),
            1,
            0,
        )));
        panel
    }

    pub fn session_list(&self) -> Rc<RefCell<SessionList>> {
        self.session_list.clone()
    }
    pub fn header(&self) -> Ref<'_, SessionSelectorHeader> {
        self.header.borrow()
    }
    pub fn scope(&self) -> SessionScope {
        self.scope.get()
    }
    pub fn sort_mode(&self) -> SortMode {
        self.sort_mode
    }
    pub fn name_filter(&self) -> NameFilter {
        self.name_filter
    }
    pub fn show_error(&mut self, message: &JsString) {
        self.show_status(StatusKind::Error, message.clone(), 4000);
    }

    fn show_status(&self, kind: StatusKind, message: JsString, delay: u64) {
        self.header
            .borrow_mut()
            .set_status_message(Some(StatusMessage { kind, message }), Some(delay));
        (self.request_render)();
    }

    /// Await the constructor's already-started load; never issue a second initial load.
    pub async fn load_current_sessions(&mut self) {
        self.run_pending_action().await;
    }

    fn begin_load_scope(&mut self, scope: SessionScope, reason: LoadReason, continuation: LoadContinuation) {
        let loader = match scope {
            SessionScope::Current => self.current_sessions_loader.clone(),
            SessionScope::All => self.all_sessions_loader.clone(),
            SessionScope::Archived => {
                let Some(loader) = self.archived_sessions_loader.clone() else {
                    self.finish_load_continuation(continuation);
                    return;
                };
                loader
            }
        };
        self.set_loading(scope, true);
        let seq_cell = match scope {
            SessionScope::All => Some(self.all_load_seq.clone()),
            SessionScope::Archived => Some(self.archived_load_seq.clone()),
            SessionScope::Current => None,
        };
        let seq = seq_cell.as_ref().map(|seq| {
            let next = seq.get() + 1;
            seq.set(next);
            next
        });
        self.header.borrow_mut().set_scope(scope);
        self.header.borrow_mut().set_loading(true);
        (self.request_render)();
        let active_scope = self.scope.clone();
        let header = self.header.clone();
        let request_render = self.request_render.clone();
        let progress = Rc::new(move |loaded, total| {
            if scope != active_scope.get() || seq_cell.as_ref().is_some_and(|cell| Some(cell.get()) != seq) {
                return;
            }
            header.borrow_mut().set_progress(loaded, total);
            request_render();
        });
        let future = loader(Some(progress));
        let context = LoadContext {
            scope,
            reason,
            seq,
            continuation,
        };
        self.queue_task(Box::pin(async move { SelectorCompletion::Load(context, future.await) }));
    }

    fn set_loading(&mut self, scope: SessionScope, loading: bool) {
        match scope {
            SessionScope::Current => self.current_loading = loading,
            SessionScope::All => self.all_loading = loading,
            SessionScope::Archived => self.archived_loading = loading,
        }
    }

    fn apply_load_result(&mut self, context: &LoadContext, result: Result<Vec<SessionInfo>, JsString>) {
        self.set_loading(context.scope, false);
        if let Ok(sessions) = &result {
            match context.scope {
                SessionScope::Current => self.current_sessions = Some(sessions.clone()),
                SessionScope::All => self.all_sessions = Some(sessions.clone()),
                SessionScope::Archived => self.archived_sessions = Some(sessions.clone()),
            }
        }
        let current_seq = match context.scope {
            SessionScope::Current => None,
            SessionScope::All => Some(self.all_load_seq.get()),
            SessionScope::Archived => Some(self.archived_load_seq.get()),
        };
        if context.scope != self.scope.get() || context.seq != current_seq {
            return;
        }
        self.header.borrow_mut().set_loading(false);
        match result {
            Ok(sessions) => self
                .session_list
                .borrow_mut()
                .set_sessions(sessions, context.scope != SessionScope::Current),
            Err(message) => {
                self.header.borrow_mut().set_status_message(
                    Some(StatusMessage {
                        kind: StatusKind::Error,
                        message: js_format!("Failed to load sessions: {}", message),
                    }),
                    Some(4000),
                );
                if context.reason == LoadReason::Initial {
                    self.session_list
                        .borrow_mut()
                        .set_sessions(Vec::new(), context.scope != SessionScope::Current);
                }
            }
        }
        (self.request_render)();
    }

    fn finish_load_continuation(&mut self, continuation: LoadContinuation) {
        match continuation {
            LoadContinuation::None => {}
            LoadContinuation::Render => (self.request_render)(),
            LoadContinuation::ExitRename => self.exit_rename_mode(),
        }
    }

    fn toggle_sort_mode(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::Recent => SortMode::Relevance,
            SortMode::Relevance => SortMode::Threaded,
            SortMode::Threaded => SortMode::Recent,
        };
        self.header.borrow_mut().set_sort_mode(self.sort_mode);
        self.session_list.borrow_mut().set_sort_mode(self.sort_mode);
        (self.request_render)();
    }
    fn toggle_name_filter(&mut self) {
        self.name_filter = match self.name_filter {
            NameFilter::All => NameFilter::Named,
            NameFilter::Named => NameFilter::All,
        };
        self.header.borrow_mut().set_name_filter(self.name_filter);
        self.session_list.borrow_mut().set_name_filter(self.name_filter);
        (self.request_render)();
    }
    fn toggle_scope(&mut self) {
        let next = match self.scope.get() {
            SessionScope::Current => SessionScope::All,
            SessionScope::All => SessionScope::Archived,
            SessionScope::Archived => SessionScope::Current,
        };
        self.scope.set(next);
        self.header.borrow_mut().set_scope(next);
        let (cached, loading) = match next {
            SessionScope::Current => (&self.current_sessions, self.current_loading),
            SessionScope::All => (&self.all_sessions, self.all_loading),
            SessionScope::Archived => (&self.archived_sessions, self.archived_loading),
        };
        if let Some(sessions) = cached {
            self.header.borrow_mut().set_loading(false);
            self.session_list
                .borrow_mut()
                .set_sessions(sessions.clone(), next != SessionScope::Current);
            (self.request_render)();
            return;
        }
        self.header.borrow_mut().set_loading(loading);
        if !loading {
            self.begin_load_scope(next, LoadReason::Toggle, LoadContinuation::None);
        }
    }

    fn enter_rename_mode(&mut self, path: &JsString, current_name: Option<&JsString>) {
        self.mode = SelectorMode::Rename;
        self.rename_target_path = Some(path.clone());
        self.rename_input
            .borrow_mut()
            .set_value(current_name.cloned().unwrap_or_default());
        self.rename_input.borrow_mut().set_focused(true);
        self.build_base_layout();
        (self.request_render)();
    }
    fn exit_rename_mode(&mut self) {
        self.mode = SelectorMode::List;
        self.rename_target_path = None;
        self.build_base_layout();
        (self.request_render)();
    }
    fn begin_rename(&mut self, value: &JsString) {
        let Some((path, rename)) = self.rename_target_path.clone().zip(self.rename_session.clone()) else {
            self.exit_rename_mode();
            return;
        };
        let next = value.trim().to_owned();
        let future = rename(path, (!next.is_empty()).then_some(next));
        self.queue_task(Box::pin(async move { SelectorCompletion::Rename(future.await) }));
    }
    pub async fn confirm_rename(&mut self, value: &JsString) -> Result<(), JsString> {
        self.begin_rename(value);
        std::future::poll_fn(|cx| self.poll_tasks(cx)).await
    }

    fn begin_delete(&mut self, path: JsString) {
        let db = self.control_db_path.clone();
        self.queue_task(Box::pin(async move {
            let result = delete_session_tree(&path, db.as_ref()).await;
            SelectorCompletion::Delete(path, result)
        }));
    }
    pub async fn delete_session(&mut self, path: &JsString) {
        self.begin_delete(path.clone());
        self.run_pending_action().await;
    }
    fn apply_delete_result(&mut self, path: &JsString, result: SessionDeleteResult) {
        let method = match result {
            SessionDeleteResult::Failure { error, .. } => {
                self.show_status(StatusKind::Error, js_format!("Failed to delete: {}", error), 3000);
                return;
            }
            SessionDeleteResult::Success { method, .. } => method,
        };
        if let Some(sessions) = &mut self.current_sessions {
            sessions.retain(|session| &session.path != path);
        }
        if let Some(sessions) = &mut self.all_sessions {
            sessions.retain(|session| &session.path != path);
        }
        let sessions = self.cached_sessions().to_vec();
        self.session_list
            .borrow_mut()
            .set_sessions(sessions, self.scope.get() != SessionScope::Current);
        let message = if method == "trash" {
            "Session moved to trash"
        } else {
            "Session deleted"
        };
        self.header.borrow_mut().set_status_message(
            Some(StatusMessage {
                kind: StatusKind::Info,
                message: message.into(),
            }),
            Some(2000),
        );
        self.begin_load_scope(self.scope.get(), LoadReason::Refresh, LoadContinuation::Render);
    }
    fn begin_archive(&mut self, path: &JsString) {
        let Some(db) = self.control_db_path.clone().filter(|db| !db.is_empty()) else {
            self.show_status(
                StatusKind::Error,
                "Session archive requires a control database.".into(),
                3000,
            );
            return;
        };
        let selected = self
            .current_sessions
            .iter()
            .chain(self.all_sessions.iter())
            .chain(self.archived_sessions.iter())
            .flatten()
            .find(|session| &session.path == path);
        if selected.is_some_and(|session| session.is_archived.unwrap_or(false)) {
            self.show_status(StatusKind::Info, "Session is already archived.".into(), 2000);
            return;
        }
        let result = archive_persisted_session(&db, path)
            .map(|_| ())
            .map_err(|error| error.message().clone());
        self.queue_task(Box::pin(std::future::ready(SelectorCompletion::Archive(result))));
    }
    pub async fn archive_selected_session(&mut self, path: &JsString) {
        self.begin_archive(path);
        self.run_pending_action().await;
    }

    fn cached_sessions(&self) -> &[SessionInfo] {
        match self.scope.get() {
            SessionScope::Current => self.current_sessions.as_deref(),
            SessionScope::All => self.all_sessions.as_deref(),
            SessionScope::Archived => self.archived_sessions.as_deref(),
        }
        .unwrap_or(&[])
    }

    fn queue_task(&mut self, mut future: SelectorTask) {
        // Start the synchronous prefix immediately, but await continuations on the
        // next driver turn, like JS promise microtasks after the current stack.
        if let Poll::Ready(completion) = future.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            future = Box::pin(std::future::ready(completion));
        }
        self.tasks.push(future);
    }
    fn complete_task(&mut self, completion: SelectorCompletion) -> Result<(), JsString> {
        match completion {
            SelectorCompletion::Load(context, result) => {
                self.apply_load_result(&context, result);
                self.finish_load_continuation(context.continuation);
            }
            SelectorCompletion::Rename(Ok(())) => {
                self.begin_load_scope(self.scope.get(), LoadReason::Refresh, LoadContinuation::ExitRename)
            }
            SelectorCompletion::Rename(Err(error)) => {
                self.exit_rename_mode();
                return Err(error);
            }
            SelectorCompletion::Delete(path, result) => self.apply_delete_result(&path, result),
            SelectorCompletion::Archive(result) => {
                result?;
                self.header.borrow_mut().set_status_message(
                    Some(StatusMessage {
                        kind: StatusKind::Info,
                        message: "Session archived".into(),
                    }),
                    Some(2000),
                );
                self.begin_load_scope(self.scope.get(), LoadReason::Refresh, LoadContinuation::Render);
            }
        }
        Ok(())
    }
    fn poll_tasks(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), JsString>> {
        loop {
            let mut completed = false;
            let mut error = None;
            for mut task in std::mem::take(&mut self.tasks) {
                match task.as_mut().poll(cx) {
                    Poll::Pending => self.tasks.push(task),
                    Poll::Ready(completion) => {
                        completed = true;
                        if let Err(cause) = self.complete_task(completion) {
                            error = Some(cause);
                        }
                    }
                }
            }
            if let Some(error) = error {
                return Poll::Ready(Err(error));
            }
            if self.tasks.is_empty() {
                return Poll::Ready(Ok(()));
            }
            if !completed {
                return Poll::Pending;
            }
        }
    }
    pub async fn run_pending_action(&mut self) {
        if let Err(error) = std::future::poll_fn(|cx| self.poll_tasks(cx)).await {
            throw_uncaught(ERROR, error);
        }
    }
    pub fn has_pending_action(&self) -> bool {
        !self.tasks.is_empty()
    }
    pub fn poll(&mut self, now: Instant) -> bool {
        self.drain_events();
        if let Poll::Ready(Err(error)) = self.poll_tasks(&mut Context::from_waker(Waker::noop())) {
            throw_uncaught(ERROR, error);
        }
        self.header.borrow_mut().poll(now)
    }

    fn drain_events(&mut self) {
        let events = std::mem::take(&mut *self.events.borrow_mut());
        if let Some(path) = events.select {
            self.header.borrow_mut().set_status_message(None, None);
            (self.on_select_callback)(path);
        }
        if events.cancel {
            self.header.borrow_mut().set_status_message(None, None);
            (self.on_cancel_callback)();
        }
        if events.exit {
            self.header.borrow_mut().set_status_message(None, None);
            (self.on_exit_callback)();
        }
        if events.toggle_scope {
            self.toggle_scope();
        }
        if events.toggle_sort {
            self.toggle_sort_mode();
        }
        if events.toggle_name_filter {
            self.toggle_name_filter();
        }
        if let Some(show) = events.toggle_path {
            self.header.borrow_mut().set_show_path(show);
            (self.request_render)();
        }
        if let Some(path) = events.delete_confirmation {
            self.header.borrow_mut().set_confirming_delete_path(path);
            (self.request_render)();
        }
        if let Some(message) = events.error {
            self.show_status(StatusKind::Error, message, 3000);
        }
        if let Some(path) = events.rename_session {
            self.handle_rename_request(&path);
        }
        if let Some(path) = events.delete_session {
            self.begin_delete(path);
        }
        if let Some(path) = events.archive_session {
            self.begin_archive(&path);
        }
    }
    fn handle_rename_request(&mut self, path: &JsString) {
        let loading = match self.scope.get() {
            SessionScope::Current => self.current_loading,
            SessionScope::All => self.all_loading,
            SessionScope::Archived => self.archived_loading,
        };
        if self.rename_session.is_none() || loading {
            return;
        }
        let name = self
            .cached_sessions()
            .iter()
            .find(|session| &session.path == path)
            .and_then(|session| session.name.clone());
        self.enter_rename_mode(path, name.as_ref());
    }
}

impl Deref for SessionSelectorComponent {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.container
    }
}
impl DerefMut for SessionSelectorComponent {
    fn deref_mut(&mut self) -> &mut Container {
        &mut self.container
    }
}
impl Component for SessionSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }
    fn handle_input(&mut self, data: &JsString) {
        if self.mode == SelectorMode::Rename {
            let keybindings = get_keybindings();
            if keybindings.matches(data, "tui.select.cancel") {
                self.exit_rename_mode();
                return;
            }
            if keybindings.matches(data, "tui.select.confirm") {
                let value = self.rename_input.borrow().get_value().clone();
                self.begin_rename(&value);
                return;
            }
            self.rename_input.borrow_mut().handle_input(data);
            return;
        }
        self.session_list.borrow_mut().handle_input(data);
        self.drain_events();
    }
    fn invalidate(&mut self) {
        self.container.invalidate();
    }
    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
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
        self.session_list.borrow_mut().set_focused(focused);
        self.rename_input.borrow_mut().set_focused(focused);
    }
}
