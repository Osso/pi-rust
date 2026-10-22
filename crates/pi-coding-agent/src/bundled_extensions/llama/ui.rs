//! Port of `packages/coding-agent/src/extensions/llama/ui.ts`.
//!
//! The llama.cpp manager is a thread-safe component: `pi-tui` components are
//! `Rc`-based, so every render rebuilds them from the declarative view state.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use pi_ai::request::{BoxFuture, CallbackError};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::number::{js_number_to_string, parse_number};
use pi_js_compat::string::{js_trim, utf16_len};
use pi_tui::components::input::InputOptions;
use pi_tui::fuzzy::fuzzy_filter;
use pi_tui::keybindings::get_keybindings;
use pi_tui::tui::ComponentRef;
use pi_tui::utils::truncate_to_width_with;
use pi_tui::{
    Component, Container, DynamicBorder, Focusable, Input, SelectItem, SelectList, SelectListLayoutOptions,
    SelectListTheme, Spacer, Text, visible_width,
};
use tokio::sync::{Notify, oneshot};
use tokio_util::sync::CancellationToken;

use crate::bundled_extensions::llama::client::{LlamaModelInfo, LlamaProgress};
use crate::bundled_extensions::llama::huggingface::HuggingFaceModel;
use crate::core::extensions::types::{
    ExtensionCommandContext, ExtensionUiComponent, ExtensionUiCustomFactory, ExtensionUiDone, ExtensionUiTui,
    NotifyType,
};
use crate::core::keybindings::KeybindingsManager;
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::theme::{Theme, ThemeColor};

const DOWNLOAD_VALUE: &str = "\0download";

/// `LlamaManagerAction`.
#[derive(Debug, Clone, PartialEq)]
pub enum LlamaManagerAction {
    Model(Box<LlamaModelInfo>),
    Download,
    Close,
}

/// `LlamaUi.connectionError` result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionErrorChoice {
    Retry,
    Close,
}

/// `ProgressState`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressState {
    pub title: String,
    pub model: String,
    pub message: String,
    pub ratio: Option<f64>,
    pub detail: Option<String>,
}

pub type LlamaProgressCallback = Arc<dyn Fn(LlamaProgress) + Send + Sync>;
pub type HuggingFaceSearchFn = Arc<
    dyn Fn(String, CancellationToken) -> BoxFuture<'static, Result<Vec<HuggingFaceModel>, CallbackError>> + Send + Sync,
>;

/// `LlamaUi` interface.
pub trait LlamaUi: Send + Sync {
    fn show_models(&self, server_url: String, models: Vec<LlamaModelInfo>) -> BoxFuture<'_, LlamaManagerAction>;
    fn select(&self, title: String, options: Vec<String>) -> BoxFuture<'_, Option<String>>;
    fn confirm(&self, title: String, message: String) -> BoxFuture<'_, bool>;
    fn connection_error(&self, server_url: String, message: JsString) -> BoxFuture<'_, ConnectionErrorChoice>;
    fn search_models(&self, search: HuggingFaceSearchFn) -> BoxFuture<'_, Option<String>>;
    fn show_status(&self, title: String, message: String);
    fn progress(&self, state: ProgressState) -> BoxFuture<'_, ()>;
    fn update_progress(&self, state: &ProgressState);
}

fn format_context(context: f64) -> String {
    if context >= 1000.0 {
        format!("{}k", js_number_to_string((context / 1000.0).round()))
    } else {
        js_number_to_string(context)
    }
}

/// `contextLabel`.
fn context_label(model: &LlamaModelInfo) -> Option<String> {
    let context = model.meta.as_ref().and_then(|meta| meta.n_ctx.or(meta.n_ctx_train));
    if let Some(context) = context.filter(|value| *value != 0.0 && !value.is_nan()) {
        return Some(format_context(context));
    }
    let args = model.status.args.as_deref().unwrap_or(&[]);
    for window in args.windows(2) {
        if !matches!(window[0].as_str(), "--ctx-size" | "-c" | "-ctx") {
            continue;
        }
        let value = parse_number(&window[1]);
        if value.is_finite() && value > 0.0 {
            return Some(format_context(value));
        }
    }
    None
}

/// `modelDescription`.
fn model_description(model: &LlamaModelInfo) -> String {
    let mut details = Vec::new();
    let loaded = model.status.value == "loaded" || model.status.value == "sleeping";
    if loaded {
        details.push("loaded".to_owned());
    } else if model.status.value != "unloaded" {
        details.push(model.status.value.clone());
    }
    if loaded && let Some(context) = context_label(model) {
        details.push(format!("{context} context"));
    }
    details.join(" · ")
}

/// `compactCount`.
fn compact_count(value: f64) -> String {
    if value >= 1_000_000.0 {
        return format!(
            "{}M",
            pi_js_compat::number::to_fixed(value / 1_000_000.0, if value >= 10_000_000.0 { 0 } else { 1 })
        );
    }
    if value >= 1_000.0 {
        return format!(
            "{}k",
            pi_js_compat::number::to_fixed(value / 1_000.0, if value >= 100_000.0 { 0 } else { 1 })
        );
    }
    js_number_to_string(value)
}

fn component(component: impl Component + 'static) -> ComponentRef {
    Rc::new(RefCell::new(component))
}

fn accent_border(theme: &Arc<Theme>) -> DynamicBorder {
    let theme = Arc::clone(theme);
    DynamicBorder::new(Rc::new(move |text: &str| theme.fg(ThemeColor::Accent, text)))
}

/// `frame`.
fn frame(theme: &Arc<Theme>, title: &str, body: Vec<ComponentRef>, footer: Option<String>) -> Container {
    let mut container = Container::new();
    container.add_child(component(accent_border(theme)));
    container.add_child(component(Text::new(
        theme.fg(ThemeColor::Accent, &theme.bold(title)),
        1,
        0,
    )));
    for child in body {
        container.add_child(child);
    }
    if let Some(footer) = footer {
        container.add_child(component(Spacer::new(1)));
        container.add_child(component(Text::new(theme.fg(ThemeColor::Dim, &footer), 1, 0)));
    }
    container.add_child(component(accent_border(theme)));
    container
}

/// `selectTheme`.
fn select_theme(theme: &Arc<Theme>) -> SelectListTheme {
    let selected_prefix = Arc::clone(theme);
    let selected_text = Arc::clone(theme);
    let description = Arc::clone(theme);
    let scroll_info = Arc::clone(theme);
    let no_match = Arc::clone(theme);
    SelectListTheme {
        selected_prefix: Rc::new(move |text: &str| selected_prefix.fg(ThemeColor::Accent, text)),
        selected_text: Rc::new(move |text: &str| selected_text.fg(ThemeColor::Accent, text)),
        description: Rc::new(move |text: &str| description.fg(ThemeColor::Muted, text)),
        scroll_info: Rc::new(move |text: &str| scroll_info.fg(ThemeColor::Dim, text)),
        no_match: Rc::new(move |text: &str| no_match.fg(ThemeColor::Warning, text)),
    }
}

fn render_list(
    theme: &Arc<Theme>,
    items: Vec<SelectItem>,
    selected: usize,
    max_visible: usize,
    layout: SelectListLayoutOptions,
) -> ComponentRef {
    let mut list = SelectList::new(items, max_visible, select_theme(theme), layout);
    list.set_selected_index(selected as isize);
    component(list)
}

#[derive(Default)]
struct ProgressSignal {
    cancelled: AtomicBool,
    notify: Notify,
}

struct ModelList {
    server_url: String,
    items: Vec<SelectItem>,
    by_id: HashMap<String, LlamaModelInfo>,
    selected: usize,
    sender: Option<oneshot::Sender<LlamaManagerAction>>,
}

struct OptionList {
    title: JsString,
    options: Vec<String>,
    selected: usize,
    sender: Option<oneshot::Sender<Option<String>>>,
}

struct SearchState {
    query: String,
    cursor: usize,
    results: Vec<HuggingFaceModel>,
    filtered: Vec<HuggingFaceModel>,
    selected: usize,
    status: String,
    debounce: Option<CancellationToken>,
    request: Option<CancellationToken>,
    request_id: u64,
    closed: bool,
    sender: Option<oneshot::Sender<Option<String>>>,
    search: HuggingFaceSearchFn,
}

enum ViewContent {
    Loading,
    Models(ModelList),
    Select(OptionList),
    Search(SearchState),
    Status { title: String, message: String },
    Progress(ProgressState),
}

struct ViewState {
    content: ViewContent,
    progress_signal: Option<Arc<ProgressSignal>>,
    showing_progress: bool,
}

/// `LlamaView`: the custom component returned to the interactive mode.
pub struct LlamaView {
    tui: ExtensionUiTui,
    theme: Arc<Theme>,
    keybindings: KeybindingsManager,
    state: Mutex<ViewState>,
    search_cache: Mutex<HashMap<String, Vec<HuggingFaceModel>>>,
    next_search_id: AtomicU64,
}

impl LlamaView {
    pub fn new(tui: ExtensionUiTui, theme: Arc<Theme>, keybindings: KeybindingsManager) -> Self {
        Self {
            tui,
            theme,
            keybindings,
            state: Mutex::new(ViewState {
                content: ViewContent::Loading,
                progress_signal: None,
                showing_progress: false,
            }),
            search_cache: Mutex::new(HashMap::new()),
            next_search_id: AtomicU64::new(0),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ViewState> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn set_content(&self, content: ViewContent) {
        {
            let mut state = self.lock();
            state.progress_signal = None;
            state.showing_progress = false;
            state.content = content;
        }
        self.tui.request_render();
    }

    fn select_raw_title(&self, title: JsString, options: Vec<String>) -> BoxFuture<'_, Option<String>> {
        let (sender, receiver) = oneshot::channel();
        self.set_content(ViewContent::Select(OptionList {
            title,
            options,
            selected: 0,
            sender: Some(sender),
        }));
        Box::pin(async move { receiver.await.unwrap_or(None) })
    }

    fn models_content(&self, list: &ModelList) -> Container {
        let body = vec![
            component(Text::new(self.theme.fg(ThemeColor::Dim, &list.server_url), 1, 0)),
            component(Spacer::new(1)),
            render_list(
                &self.theme,
                list.items.clone(),
                list.selected,
                list.items.len().min(12),
                SelectListLayoutOptions {
                    min_primary_column_width: Some(36),
                    max_primary_column_width: Some(56),
                    truncate_primary: None,
                },
            ),
        ];
        let footer = format!(
            "{} • {}",
            key_hint("tui.select.confirm", "load/unload/download"),
            key_hint("tui.select.cancel", "close")
        );
        frame(&self.theme, "llama.cpp models", body, Some(footer))
    }

    fn select_content(&self, list: &OptionList) -> Container {
        let items = list
            .options
            .iter()
            .map(|option| SelectItem::new(option.clone(), option.clone()))
            .collect::<Vec<_>>();
        let body = vec![
            component(Spacer::new(1)),
            render_list(
                &self.theme,
                items,
                list.selected,
                list.options.len().min(12),
                SelectListLayoutOptions::default(),
            ),
        ];
        let footer = format!(
            "{} • {}",
            key_hint("tui.select.confirm", "select"),
            key_hint("tui.select.cancel", "cancel")
        );
        frame(&self.theme, &encode_node_utf8(&list.title), body, Some(footer))
    }

    fn search_results(&self, search: &SearchState) -> Container {
        let mut container = Container::new();
        let max_visible = 10_isize;
        let length = search.filtered.len() as isize;
        let start = 0_isize.max((search.selected as isize - max_visible / 2).min(length - max_visible)) as usize;
        let end = ((start as isize + max_visible).min(length)) as usize;
        for index in start..end {
            let Some(model) = search.filtered.get(index) else {
                continue;
            };
            let prefix = if index == search.selected { "→ " } else { "  " };
            let details = format!("{} downloads", compact_count(model.downloads));
            let text = if index == search.selected {
                self.theme
                    .fg(ThemeColor::Accent, &format!("{prefix}{}  {details}", model.id))
            } else {
                format!(
                    "{prefix}{}{}",
                    model.id,
                    self.theme.fg(ThemeColor::Muted, &format!("  {details}"))
                )
            };
            container.add_child(component(Text::new(text, 0, 0)));
        }
        if start > 0 || end < search.filtered.len() {
            container.add_child(component(Text::new(
                self.theme.fg(
                    ThemeColor::Dim,
                    &format!("  ({}/{})", search.selected + 1, search.filtered.len()),
                ),
                0,
                0,
            )));
        }
        if search.filtered.is_empty() || search.status == "Searching Hugging Face…" {
            container.add_child(component(Text::new(
                self.theme.fg(ThemeColor::Dim, &format!("  {}", search.status)),
                0,
                0,
            )));
        }
        container
    }

    fn search_content(&self, search: &SearchState) -> Container {
        let mut input = Input::new(InputOptions { mask: false });
        input.set_value(&search.query);
        input.set_cursor(search.cursor);
        input.set_focused(true);
        let body = vec![
            component(Text::new(
                self.theme.fg(ThemeColor::Dim, "Model name or owner/repository[:quant]"),
                1,
                0,
            )),
            component(input),
            component(Spacer::new(1)),
            component(self.search_results(search)),
        ];
        let footer = format!(
            "{} • {}",
            key_hint("tui.select.confirm", "select"),
            key_hint("tui.select.cancel", "back")
        );
        frame(&self.theme, "Download model", body, Some(footer))
    }

    fn status_content(&self, title: &str, message: &str) -> Container {
        frame(
            &self.theme,
            title,
            vec![
                component(Spacer::new(1)),
                component(Text::new(self.theme.fg(ThemeColor::Muted, message), 1, 0)),
            ],
            None,
        )
    }

    fn progress_content(&self, state: &ProgressState) -> Container {
        let mut body = vec![
            component(Text::new(self.theme.fg(ThemeColor::Text, &state.model), 1, 0)),
            component(Spacer::new(1)),
            component(Text::new(self.theme.fg(ThemeColor::Muted, &state.message), 1, 0)),
        ];
        if let Some(ratio) = state.ratio {
            let available = 40;
            let filled = ((ratio.clamp(0.0, 1.0)) * available as f64).round() as usize;
            let bar = format!(
                "{}{} {}%",
                "█".repeat(filled),
                "─".repeat(available - filled),
                (ratio * 100.0).round()
            );
            body.push(component(Text::new(self.theme.fg(ThemeColor::Accent, &bar), 1, 0)));
        }
        if let Some(detail) = state.detail.as_ref().filter(|detail| !detail.is_empty()) {
            body.push(component(Text::new(self.theme.fg(ThemeColor::Dim, detail), 1, 0)));
        }
        frame(
            &self.theme,
            &state.title,
            body,
            Some(key_hint("tui.select.cancel", "stop")),
        )
    }
}

impl LlamaUi for LlamaView {
    fn show_models(&self, server_url: String, models: Vec<LlamaModelInfo>) -> BoxFuture<'_, LlamaManagerAction> {
        let (sender, receiver) = oneshot::channel();
        let mut sorted = models;
        sorted.sort_by(|left, right| {
            let left_loaded = u8::from(left.status.value == "loaded");
            let right_loaded = u8::from(right.status.value == "loaded");
            right_loaded
                .cmp(&left_loaded)
                .then_with(|| pi_js_compat::collator::locale_compare(&left.id, &right.id))
        });
        let items = sorted
            .iter()
            .map(|model| SelectItem::new(model.id.clone(), model.id.clone()).with_description(model_description(model)))
            .chain(std::iter::once(
                SelectItem::new(DOWNLOAD_VALUE, "Download model…")
                    .with_description("Hugging Face owner/repository[:quant]"),
            ))
            .collect::<Vec<_>>();
        let by_id = sorted
            .into_iter()
            .map(|model| (model.id.clone(), model))
            .collect::<HashMap<_, _>>();
        self.set_content(ViewContent::Models(ModelList {
            server_url,
            items,
            by_id,
            selected: 0,
            sender: Some(sender),
        }));
        Box::pin(async move { receiver.await.unwrap_or(LlamaManagerAction::Close) })
    }

    fn select(&self, title: String, options: Vec<String>) -> BoxFuture<'_, Option<String>> {
        self.select_raw_title(title.into(), options)
    }

    fn confirm(&self, title: String, message: String) -> BoxFuture<'_, bool> {
        Box::pin(async move {
            self.select(format!("{title}\n{message}"), vec!["Yes".to_owned(), "No".to_owned()])
                .await
                .as_deref()
                == Some("Yes")
        })
    }

    fn connection_error(&self, server_url: String, message: JsString) -> BoxFuture<'_, ConnectionErrorChoice> {
        Box::pin(async move {
            let choice = self
                .select_raw_title(
                    JsString::from("llama.cpp unavailable\n") + server_url + "\n\n" + message,
                    vec!["Retry".to_owned(), "Close".to_owned()],
                )
                .await;
            if choice.as_deref() == Some("Retry") {
                ConnectionErrorChoice::Retry
            } else {
                ConnectionErrorChoice::Close
            }
        })
    }

    fn search_models(&self, search: HuggingFaceSearchFn) -> BoxFuture<'_, Option<String>> {
        let (sender, receiver) = oneshot::channel();
        self.set_content(ViewContent::Search(SearchState {
            query: String::new(),
            cursor: 0,
            results: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            status: "Type at least 2 characters".to_owned(),
            debounce: None,
            request: None,
            request_id: 0,
            closed: false,
            sender: Some(sender),
            search,
        }));
        Box::pin(async move { receiver.await.unwrap_or(None) })
    }

    fn show_status(&self, title: String, message: String) {
        self.set_content(ViewContent::Status { title, message });
    }

    fn progress(&self, state: ProgressState) -> BoxFuture<'_, ()> {
        let signal = {
            let mut view = self.lock();
            let signal = Arc::clone(
                view.progress_signal
                    .get_or_insert_with(|| Arc::new(ProgressSignal::default())),
            );
            view.showing_progress = true;
            view.content = ViewContent::Progress(state);
            signal
        };
        self.tui.request_render();
        Box::pin(async move {
            if signal.cancelled.load(Ordering::SeqCst) {
                return;
            }
            signal.notify.notified().await;
        })
    }

    fn update_progress(&self, state: &ProgressState) {
        {
            let mut view = self.lock();
            if !view.showing_progress {
                return;
            }
            view.content = ViewContent::Progress(state.clone());
        }
        self.tui.request_render();
    }
}

impl LlamaView {
    /// Dispatches terminal input; used by the interactive adapter and tests.
    pub fn handle_input(self: &Arc<Self>, data: &str) {
        {
            let mut view = self.lock();
            if let Some(signal) = view.progress_signal.take() {
                if self.keybindings.matches(data, "tui.select.cancel") {
                    signal.cancelled.store(true, Ordering::SeqCst);
                    signal.notify.notify_one();
                    return;
                }
                view.progress_signal = Some(signal);
            }
            match &mut view.content {
                ViewContent::Models(list) => handle_model_input(list, data),
                ViewContent::Select(list) => handle_select_input(list, data),
                ViewContent::Search(search) => handle_search_input(self, search, data),
                ViewContent::Loading | ViewContent::Status { .. } | ViewContent::Progress(_) => {}
            }
        }
        self.tui.request_render();
    }

    /// Renders the current view at `width`.
    pub fn render(&self, width: usize) -> Vec<String> {
        let lines = {
            let view = self.lock();
            match &view.content {
                ViewContent::Loading => frame(
                    &self.theme,
                    "llama.cpp models",
                    vec![component(Text::new(self.theme.fg(ThemeColor::Muted, "Loading…"), 1, 1))],
                    None,
                )
                .render(width),
                ViewContent::Models(list) => self.models_content(list).render(width),
                ViewContent::Select(list) => self.select_content(list).render(width),
                ViewContent::Search(search) => self.search_content(search).render(width),
                ViewContent::Status { title, message } => self.status_content(title, message).render(width),
                ViewContent::Progress(state) => self.progress_content(state).render(width),
            }
        };
        lines
            .into_iter()
            .map(|line| {
                if visible_width(&line) > width {
                    truncate_to_width_with(&line, width, "", false)
                } else {
                    line
                }
            })
            .collect()
    }
}

fn handle_model_input(list: &mut ModelList, data: &str) {
    let keybindings = get_keybindings();
    if keybindings.matches(data, "tui.select.up") {
        if !list.items.is_empty() {
            list.selected = if list.selected == 0 {
                list.items.len() - 1
            } else {
                list.selected - 1
            };
        }
        return;
    }
    if keybindings.matches(data, "tui.select.down") {
        if !list.items.is_empty() {
            list.selected = if list.selected == list.items.len() - 1 {
                0
            } else {
                list.selected + 1
            };
        }
        return;
    }
    if keybindings.matches(data, "tui.select.confirm") {
        let Some(item) = list.items.get(list.selected) else {
            return;
        };
        let action = if item.value == DOWNLOAD_VALUE {
            LlamaManagerAction::Download
        } else if let Some(model) = list.by_id.get(&item.value) {
            LlamaManagerAction::Model(Box::new(model.clone()))
        } else {
            return;
        };
        if let Some(sender) = list.sender.take() {
            let _ = sender.send(action);
        }
        return;
    }
    if keybindings.matches(data, "tui.select.cancel")
        && let Some(sender) = list.sender.take()
    {
        let _ = sender.send(LlamaManagerAction::Close);
    }
}

fn handle_select_input(list: &mut OptionList, data: &str) {
    let keybindings = get_keybindings();
    if keybindings.matches(data, "tui.select.up") {
        if !list.options.is_empty() {
            list.selected = if list.selected == 0 {
                list.options.len() - 1
            } else {
                list.selected - 1
            };
        }
        return;
    }
    if keybindings.matches(data, "tui.select.down") {
        if !list.options.is_empty() {
            list.selected = if list.selected == list.options.len() - 1 {
                0
            } else {
                list.selected + 1
            };
        }
        return;
    }
    if keybindings.matches(data, "tui.select.confirm") {
        let selected = list.options.get(list.selected).cloned();
        if let (Some(selected), Some(sender)) = (selected, list.sender.take()) {
            let _ = sender.send(Some(selected));
        }
        return;
    }
    if keybindings.matches(data, "tui.select.cancel")
        && let Some(sender) = list.sender.take()
    {
        let _ = sender.send(None);
    }
}

fn handle_search_input(view: &Arc<LlamaView>, search: &mut SearchState, data: &str) {
    let keybindings = &view.keybindings;
    if keybindings.matches(data, "tui.select.up") {
        if !search.filtered.is_empty() {
            search.selected = if search.selected == 0 {
                search.filtered.len() - 1
            } else {
                search.selected - 1
            };
        }
        return;
    }
    if keybindings.matches(data, "tui.select.down") {
        if !search.filtered.is_empty() {
            search.selected = if search.selected == search.filtered.len() - 1 {
                0
            } else {
                search.selected + 1
            };
        }
        return;
    }
    if keybindings.matches(data, "tui.select.confirm") {
        let selected = if exact_hugging_face_model(&search.query) {
            Some(search.query.clone())
        } else {
            search.filtered.get(search.selected).map(|model| model.id.clone())
        };
        if let Some(selected) = selected {
            close_search(search, Some(selected));
        }
        return;
    }
    if keybindings.matches(data, "tui.select.cancel") {
        close_search(search, None);
        return;
    }
    let mut input = Input::new(InputOptions { mask: false });
    input.set_value(&search.query);
    input.set_cursor(search.cursor);
    input.handle_input(data);
    let query = js_trim(input.get_value()).to_owned();
    search.cursor = input.cursor();
    if query == search.query {
        return;
    }
    search.query = query;
    schedule_search(view, search);
}

/// `/^[^/\s]+\/[^:\s]+(?::[^\s:]+)?$/u`
fn exact_hugging_face_model(query: &str) -> bool {
    let Some((owner, rest)) = query.split_once('/') else {
        return false;
    };
    if owner.is_empty() || owner.chars().any(char::is_whitespace) {
        return false;
    }
    let (repository, quantization) = match rest.split_once(':') {
        Some((repository, quantization)) => (repository, Some(quantization)),
        None => (rest, None),
    };
    if repository.is_empty()
        || repository
            .chars()
            .any(|character| character == ':' || character.is_whitespace())
    {
        return false;
    }
    match quantization {
        Some(quantization) => {
            !quantization.is_empty()
                && !quantization
                    .chars()
                    .any(|character| character == ':' || character.is_whitespace())
        }
        None => true,
    }
}

fn close_search(search: &mut SearchState, model: Option<String>) {
    if search.closed {
        return;
    }
    search.closed = true;
    if let Some(debounce) = search.debounce.take() {
        debounce.cancel();
    }
    if let Some(request) = search.request.take() {
        request.cancel();
    }
    if let Some(sender) = search.sender.take() {
        let _ = sender.send(model);
    }
}

fn filter_results(search: &mut SearchState) {
    if !search.query.is_empty() {
        let matches: HashSet<String> = fuzzy_filter(&search.results, &search.query, |model| vec![model.id.clone()])
            .into_iter()
            .map(|model| model.id)
            .collect();
        search.filtered = search
            .results
            .iter()
            .filter(|model| matches.contains(&model.id))
            .cloned()
            .collect();
    } else {
        search.filtered = search.results.clone();
    }
    search.selected = search.selected.min(search.filtered.len().saturating_sub(1));
}

fn schedule_search(view: &Arc<LlamaView>, search: &mut SearchState) {
    if let Some(debounce) = search.debounce.take() {
        debounce.cancel();
    }
    if let Some(request) = search.request.take() {
        request.cancel();
    }
    if utf16_len(&search.query) < 2 {
        search.status = "Type at least 2 characters".to_owned();
        filter_results(search);
        return;
    }
    let cache_key = search.query.to_lowercase();
    let cached = view
        .search_cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&cache_key)
        .cloned();
    if let Some(cached) = cached {
        search.results = cached;
        search.status = if search.results.is_empty() {
            "No GGUF models found".to_owned()
        } else {
            String::new()
        };
        filter_results(search);
        return;
    }
    search.status = "Searching Hugging Face…".to_owned();
    filter_results(search);
    let debounce = CancellationToken::new();
    search.debounce = Some(debounce.clone());
    let view = Arc::clone(view);
    let query = search.query.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = debounce.cancelled() => {}
            _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                run_search(view, query, debounce).await;
            }
        }
    });
}

async fn run_search(view: Arc<LlamaView>, query: String, debounce: CancellationToken) {
    if debounce.is_cancelled() {
        return;
    }
    let request = CancellationToken::new();
    let (search_fn, request_id) = {
        let mut state = view.lock();
        let ViewContent::Search(search) = &mut state.content else {
            return;
        };
        search.request = Some(request.clone());
        search.request_id = view.next_search_id.fetch_add(1, Ordering::SeqCst) + 1;
        (Arc::clone(&search.search), search.request_id)
    };
    let result = search_fn(query.clone(), request.clone()).await;
    match result {
        Ok(results) => {
            view.search_cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(query.to_lowercase(), results.clone());
            let mut state = view.lock();
            let ViewContent::Search(search) = &mut state.content else {
                return;
            };
            if search.closed || request.is_cancelled() || search.query != query {
                if search.request_id == request_id {
                    search.request = None;
                }
                return;
            }
            search.results = results;
            search.selected = 0;
            search.status = if search.results.is_empty() {
                "No GGUF models found".to_owned()
            } else {
                String::new()
            };
            filter_results(search);
            if search.request_id == request_id {
                search.request = None;
            }
        }
        Err(error) => {
            let mut state = view.lock();
            let ViewContent::Search(search) = &mut state.content else {
                return;
            };
            if search.closed || request.is_cancelled() || search.query != query {
                if search.request_id == request_id {
                    search.request = None;
                }
                return;
            }
            search.results.clear();
            search.status = error.message;
            filter_results(search);
            if search.request_id == request_id {
                search.request = None;
            }
        }
    }
}

struct LlamaViewComponent {
    view: Arc<LlamaView>,
}

impl ExtensionUiComponent for LlamaViewComponent {
    fn render(&self, width: usize) -> Vec<pi_js_compat::js_json::JsString> {
        self.view.render(width)
    }

    fn handle_input(&self, data: &pi_js_compat::js_json::JsString) {
        self.view.handle_input(data);
    }
}

/// `runWithProgress`'s `run` callback.
pub type RunWithProgressRun<T> =
    Box<dyn FnOnce(CancellationToken, LlamaProgressCallback) -> BoxFuture<'static, Result<T, String>> + Send>;
/// `runWithProgress`'s `cancel` callback.
pub type RunWithProgressCancel = Box<dyn Fn() -> BoxFuture<'static, Result<(), String>> + Send + Sync>;

/// `runWithProgress` options.
pub struct RunWithProgressOptions<T> {
    pub title: String,
    pub model: String,
    pub initial_message: String,
    pub cancel_title: String,
    pub cancel_message: String,
    pub run: RunWithProgressRun<T>,
    pub cancel: RunWithProgressCancel,
}

/// `runWithProgress` outcome.
#[derive(Debug, PartialEq, Eq)]
pub enum RunWithProgressOutcome<T> {
    Cancelled,
    Completed(T),
}

/// `runWithProgress`.
pub async fn run_with_progress<T: Send + 'static>(
    ui: Arc<dyn LlamaUi>,
    options: RunWithProgressOptions<T>,
) -> Result<RunWithProgressOutcome<T>, String> {
    let RunWithProgressOptions {
        title,
        model,
        initial_message,
        cancel_title,
        cancel_message,
        run,
        cancel,
    } = options;
    let controller = CancellationToken::new();
    let state = Arc::new(Mutex::new(ProgressState {
        title,
        model,
        message: initial_message,
        ratio: None,
        detail: None,
    }));
    let update_state = Arc::clone(&state);
    let update_ui = Arc::clone(&ui);
    let update: LlamaProgressCallback = Arc::new(move |progress| {
        let snapshot = {
            let mut state = update_state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            state.message = progress.message;
            state.ratio = progress.ratio;
            state.detail = progress.detail;
            state.clone()
        };
        update_ui.update_progress(&snapshot);
    });
    let completed = Arc::new(AtomicBool::new(false));
    let (sender, mut receiver) = oneshot::channel();
    let settled = run(controller.clone(), update);
    let run_completed = Arc::clone(&completed);
    tokio::spawn(async move {
        let result = settled.await;
        run_completed.store(true, Ordering::SeqCst);
        let _ = sender.send(result);
    });

    let result: Result<T, String> = loop {
        let snapshot = state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        tokio::select! {
            biased;
            received = &mut receiver => {
                break received.map_err(|_| "progress task ended".to_owned())?;
            }
            _ = ui.progress(snapshot) => {
                if completed.load(Ordering::SeqCst) {
                    continue;
                }
                let stop = ui.confirm(cancel_title.clone(), cancel_message.clone()).await;
                if !stop || completed.load(Ordering::SeqCst) {
                    continue;
                }
                let cancelled = (cancel)().await;
                controller.cancel();
                cancelled?;
                let _ = (&mut receiver).await;
                return Ok(RunWithProgressOutcome::Cancelled);
            }
        }
    };
    result.map(RunWithProgressOutcome::Completed)
}

/// `showLlamaUi`'s run callback.
pub type LlamaUiRun = Box<dyn FnOnce(Arc<dyn LlamaUi>) -> BoxFuture<'static, Result<(), JsString>> + Send>;

/// `showLlamaUi`.
pub async fn show_llama_ui(ctx: &ExtensionCommandContext, run: LlamaUiRun) -> Result<(), JsString> {
    let ui_context = ctx.ui().map_err(|error| error.message)?;
    show_llama_ui_with_context(ui_context, run).await
}

/// [`show_llama_ui`] against an explicit UI context (the interactive adapter).
pub async fn show_llama_ui_with_context(
    ui_context: Arc<dyn crate::core::extensions::types::ExtensionUiContext>,
    run: LlamaUiRun,
) -> Result<(), JsString> {
    let run = Arc::new(Mutex::new(Some(run)));
    let notify_context = Arc::clone(&ui_context);
    let factory: ExtensionUiCustomFactory = Arc::new(move |context, done: ExtensionUiDone| {
        let view = Arc::new(LlamaView::new(context.tui, context.theme, context.keybindings));
        let ui: Arc<dyn LlamaUi> = Arc::clone(&view) as Arc<dyn LlamaUi>;
        let run = run
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .expect("custom factory is called once");
        let notify_context = Arc::clone(&notify_context);
        tokio::task::spawn_local(async move {
            match run(ui).await {
                Ok(()) => done.call(None),
                Err(error) => {
                    notify_context.notify(&error, Some(NotifyType::Error));
                    done.call(None);
                }
            }
        });
        crate::core::extensions::ready_custom_component(Box::new(LlamaViewComponent { view }))
    });
    ui_context.custom(factory, None).await.map(|_| ()).map_err(Into::into)
}
