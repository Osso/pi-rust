//! Ported from `coding-agent/src/modes/interactive/components/model-selector.ts`.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use pi_ai::models::models_are_equal;
use pi_ai::models::{ModelsRefreshOptions, ModelsRefreshResult};
use pi_ai::types::Model;
use pi_js_compat::{js_format, js_json::JsString, locale_compare};
use pi_tui::components::input::InputOptions;
use pi_tui::tui::component;
use pi_tui::{Component, Container, DynamicBorder, Focusable, Input, Spacer, Text, fuzzy_filter};
use tokio_util::sync::CancellationToken;

use crate::core::model_resolver::ScopedModel;
use crate::core::model_runtime::{ModelRuntime, ModelRuntimeError};
use crate::core::settings_manager::{SharedSettingsManager, lock_settings_manager};
use crate::modes::interactive::components::dynamic_border::dynamic_border;
use crate::modes::interactive::components::keybinding_hints::key_hint;
use crate::modes::interactive::model_search::{ModelSearchItem, get_model_search_candidates};
use crate::modes::interactive::theme::{Theme, ThemeColor};

const REFRESH_TIMEOUT: Duration = Duration::from_secs(15);

/// TUI capability the selector needs (`TUI.requestRender`).
pub trait ModelSelectorUi {
    fn request_render(&self);
}

#[derive(Clone)]
struct ModelItem {
    provider: String,
    id: String,
    model: Model,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelScope {
    All,
    Scoped,
}

/// Model selector with search and all/scoped scope switching.
pub struct ModelSelectorComponent {
    search_input: Input,
    list_container: Container,
    all_models: Vec<ModelItem>,
    scoped_model_items: Vec<ModelItem>,
    active_models: Vec<ModelItem>,
    filtered_models: Vec<ModelItem>,
    selected_index: usize,
    current_model: Option<Model>,
    settings_manager: SharedSettingsManager,
    model_runtime: ModelRuntime,
    on_select: Rc<dyn Fn(&Model)>,
    on_cancel: Rc<dyn Fn()>,
    error_message: Option<JsString>,
    refresh_status_message: JsString,
    refresh_status_success: bool,
    ui: Rc<dyn ModelSelectorUi>,
    scoped_models: Vec<ScopedModel>,
    scope: ModelScope,
    scope_text: Option<Text>,
    scope_hint_text: Option<Text>,
    refresh_abort: CancellationToken,
    closed: bool,
    focused: bool,
    top_border: DynamicBorder,
    hint_text: Option<Text>,
    bottom_border: DynamicBorder,
    theme: Arc<Theme>,
}

impl ModelSelectorComponent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ui: Rc<dyn ModelSelectorUi>,
        current_model: Option<Model>,
        settings_manager: SharedSettingsManager,
        model_runtime: ModelRuntime,
        scoped_models: Vec<ScopedModel>,
        on_select: Rc<dyn Fn(&Model)>,
        on_cancel: Rc<dyn Fn()>,
        initial_search_input: Option<&JsString>,
        theme: Arc<Theme>,
    ) -> Self {
        let scope = if scoped_models.is_empty() {
            ModelScope::All
        } else {
            ModelScope::Scoped
        };
        let mut search_input = Input::new(InputOptions::default());
        if let Some(initial) = initial_search_input {
            search_input.set_value(initial);
        }
        let mut selector = Self {
            search_input,
            list_container: Container::new(),
            all_models: Vec::new(),
            scoped_model_items: Vec::new(),
            active_models: Vec::new(),
            filtered_models: Vec::new(),
            selected_index: 0,
            current_model,
            settings_manager,
            model_runtime,
            on_select,
            on_cancel,
            error_message: None,
            refresh_status_message: "Refreshing model catalogs…".into(),
            refresh_status_success: false,
            ui,
            scoped_models,
            scope,
            scope_text: None,
            scope_hint_text: None,
            refresh_abort: CancellationToken::new(),
            closed: false,
            focused: false,
            top_border: dynamic_border(),
            hint_text: None,
            bottom_border: dynamic_border(),
            theme,
        };

        if selector.scoped_models.is_empty() {
            selector.hint_text = Some(Text::new(
                selector.theme.fg(
                    ThemeColor::Warning,
                    "Only showing models from configured providers. Use /login to add providers.",
                ),
                0,
                0,
            ));
        } else {
            selector.scope_text = Some(Text::new(selector.get_scope_text(), 0, 0));
            selector.scope_hint_text = Some(Text::new(selector.get_scope_hint_text(), 0, 0));
        }

        selector.load_models_from_snapshot();
        if let Some(initial) = initial_search_input {
            selector.filter_models(initial);
        } else {
            selector.update_list();
        }
        selector.ui.request_render();
        selector
    }

    fn load_models_from_snapshot(&mut self) {
        let models: Vec<ModelItem> = self
            .model_runtime
            .get_available_snapshot()
            .into_iter()
            .map(|model| ModelItem {
                provider: model.provider.clone(),
                id: model.id.clone(),
                model,
            })
            .collect();
        self.all_models = self.sort_models(models);
        self.scoped_models = self
            .scoped_models
            .iter()
            .map(|scoped| {
                let refreshed = self.model_runtime.get_model(&scoped.model.provider, &scoped.model.id);
                ScopedModel {
                    model: refreshed.unwrap_or_else(|| scoped.model.clone()),
                    thinking_level: scoped.thinking_level,
                }
            })
            .collect();
        self.scoped_model_items = self
            .scoped_models
            .iter()
            .map(|scoped| ModelItem {
                provider: scoped.model.provider.clone(),
                id: scoped.model.id.clone(),
                model: scoped.model.clone(),
            })
            .collect();
        self.active_models = if self.scope == ModelScope::Scoped {
            self.scoped_model_items.clone()
        } else {
            self.all_models.clone()
        };
        self.filtered_models = self.active_models.clone();
        let current_index = self
            .filtered_models
            .iter()
            .position(|item| models_are_equal(self.current_model.as_ref(), Some(&item.model)));
        self.selected_index =
            current_index.unwrap_or_else(|| self.selected_index.min(self.filtered_models.len().saturating_sub(1)));
    }

    /// `refreshModels`: refreshes catalogs with the 15s timeout and abort signal.
    pub async fn refresh_models(&mut self) {
        let runtime = self.model_runtime.clone();
        let abort = self.refresh_abort.clone();
        let options = ModelsRefreshOptions {
            signal: Some(abort.clone()),
            ..Default::default()
        };
        let refresh = runtime.refresh(options);
        tokio::pin!(refresh);
        let mut timed_out = false;
        let result: Result<ModelsRefreshResult, ModelRuntimeError> = tokio::select! {
            result = &mut refresh => result,
            _ = tokio::time::sleep(REFRESH_TIMEOUT) => {
                timed_out = true;
                abort.cancel();
                refresh.await
            }
        };
        if self.closed {
            return;
        }
        self.refresh_status_message = JsString::default();
        match result {
            Ok(result) => {
                if result.aborted && timed_out {
                    self.error_message = Some("Model refresh timed out; showing cached models.".into());
                } else if result.errors.len() == 1 {
                    let provider = result.errors.keys().next().cloned().unwrap_or_default();
                    self.error_message = Some(js_format!("Could not refresh {}; showing cached models.", provider));
                } else if result.errors.len() > 1 {
                    let providers: Vec<&str> = result.errors.keys().map(String::as_str).collect();
                    self.error_message = Some(js_format!(
                        "Could not refresh {} model catalogs ({}); showing cached models.",
                        result.errors.len(),
                        providers.join(", ")
                    ));
                } else {
                    self.error_message = self.model_runtime.get_error();
                    if self.error_message.is_none() {
                        self.refresh_status_message = "Model catalogs refreshed.".into();
                        self.refresh_status_success = true;
                    }
                }
                self.load_models_from_snapshot();
                let query = self.search_input.get_value().clone();
                self.filter_models(&query);
                self.ui.request_render();
            }
            Err(error) => {
                self.refresh_status_message = JsString::default();
                self.error_message = Some(if timed_out {
                    "Model refresh timed out; showing cached models.".into()
                } else {
                    js_format!("Could not refresh model catalogs: {}", error.message())
                });
                self.update_list();
                self.ui.request_render();
            }
        }
    }

    pub fn dispose(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.refresh_abort.cancel();
    }

    fn sort_models(&self, models: Vec<ModelItem>) -> Vec<ModelItem> {
        let mut sorted = models;
        sorted.sort_by(|a, b| {
            let a_is_current = models_are_equal(self.current_model.as_ref(), Some(&a.model));
            let b_is_current = models_are_equal(self.current_model.as_ref(), Some(&b.model));
            if a_is_current && !b_is_current {
                return std::cmp::Ordering::Less;
            }
            if !a_is_current && b_is_current {
                return std::cmp::Ordering::Greater;
            }
            locale_compare(&a.provider, &b.provider)
        });
        sorted
    }

    fn get_scope_text(&self) -> JsString {
        let all_text = if self.scope == ModelScope::All {
            self.theme.fg(ThemeColor::Accent, "all")
        } else {
            self.theme.fg(ThemeColor::Muted, "all")
        };
        let scoped_text = if self.scope == ModelScope::Scoped {
            self.theme.fg(ThemeColor::Accent, "scoped")
        } else {
            self.theme.fg(ThemeColor::Muted, "scoped")
        };
        js_format!(
            "{}{}{}{}",
            self.theme.fg(ThemeColor::Muted, "Scope: "),
            all_text,
            self.theme.fg(ThemeColor::Muted, " | "),
            scoped_text
        )
    }

    fn get_scope_hint_text(&self) -> JsString {
        key_hint("tui.input.tab", "scope") + &self.theme.fg(ThemeColor::Muted, " (all/scoped)")
    }

    fn set_scope(&mut self, scope: ModelScope) {
        if self.scope == scope {
            return;
        }
        self.scope = scope;
        self.active_models = if self.scope == ModelScope::Scoped {
            self.scoped_model_items.clone()
        } else {
            self.all_models.clone()
        };
        let current_index = self
            .active_models
            .iter()
            .position(|item| models_are_equal(self.current_model.as_ref(), Some(&item.model)));
        self.selected_index = current_index.unwrap_or(0);
        let query = self.search_input.get_value().clone();
        self.filter_models(&query);
        let scope_text_value = self.get_scope_text();
        if let Some(scope_text) = &mut self.scope_text {
            scope_text.set_text(scope_text_value);
        }
    }

    fn filter_models(&mut self, query: &JsString) {
        self.filtered_models = if query.is_empty() {
            self.active_models.clone()
        } else {
            fuzzy_filter(&self.active_models, query, |item| {
                get_model_search_candidates(&ModelSearchItem {
                    id: item.id.clone(),
                    provider: item.provider.clone(),
                    name: Some(item.model.name.clone()),
                })
                .into_iter()
                .map(JsString::from)
                .collect()
            })
        };
        self.selected_index = if query.is_empty() {
            self.selected_index.min(self.filtered_models.len().saturating_sub(1))
        } else {
            0
        };
        self.update_list();
    }

    fn update_list(&mut self) {
        self.list_container.clear();

        let max_visible = 10usize;
        let start_index = (self.selected_index as i64 - (max_visible / 2) as i64)
            .min(self.filtered_models.len() as i64 - max_visible as i64)
            .max(0) as usize;
        let end_index = (start_index + max_visible).min(self.filtered_models.len());

        for index in start_index..end_index {
            let item = &self.filtered_models[index];
            let is_selected = index == self.selected_index;
            let is_current = models_are_equal(self.current_model.as_ref(), Some(&item.model));
            let line = if is_selected {
                let prefix = self.theme.fg(ThemeColor::Accent, "→ ");
                let model_text = item.id.clone();
                let provider_badge = self.theme.fg(ThemeColor::Muted, &js_format!("[{}]", item.provider));
                let checkmark = if is_current {
                    self.theme.fg(ThemeColor::Success, " ✓")
                } else {
                    JsString::default()
                };
                js_format!(
                    "{} {}{}",
                    prefix + &self.theme.fg(ThemeColor::Accent, &model_text),
                    provider_badge,
                    checkmark
                )
            } else {
                let model_text = js_format!("  {}", item.id);
                let provider_badge = self.theme.fg(ThemeColor::Muted, &js_format!("[{}]", item.provider));
                let checkmark = if is_current {
                    self.theme.fg(ThemeColor::Success, " ✓")
                } else {
                    JsString::default()
                };
                js_format!("{} {}{}", model_text, provider_badge, checkmark)
            };
            self.list_container.add_child(component(Text::new(line, 0, 0)));
        }

        if start_index > 0 || end_index < self.filtered_models.len() {
            let scroll_info = self.theme.fg(
                ThemeColor::Muted,
                &js_format!("  ({}/{})", self.selected_index + 1, self.filtered_models.len()),
            );
            self.list_container.add_child(component(Text::new(scroll_info, 0, 0)));
        }

        if let Some(error_message) = &self.error_message {
            for line in error_message.split('\n') {
                self.list_container
                    .add_child(component(Text::new(self.theme.fg(ThemeColor::Error, line), 0, 0)));
            }
        } else if self.filtered_models.is_empty() {
            self.list_container.add_child(component(Text::new(
                self.theme.fg(ThemeColor::Muted, "  No matching models"),
                0,
                0,
            )));
        } else {
            let selected = &self.filtered_models[self.selected_index];
            self.list_container.add_child(component(Spacer::new(1)));
            self.list_container.add_child(component(Text::new(
                self.theme
                    .fg(ThemeColor::Muted, &js_format!("  Model Name: {}", selected.model.name)),
                0,
                0,
            )));
        }
        if !self.refresh_status_message.is_empty() {
            self.list_container.add_child(component(Spacer::new(1)));
            let color = if self.refresh_status_success {
                ThemeColor::Success
            } else {
                ThemeColor::Muted
            };
            self.list_container.add_child(component(Text::new(
                self.theme.fg(color, &js_format!("  {}", self.refresh_status_message)),
                0,
                0,
            )));
        }
    }

    fn handle_select(&mut self, model: &Model) {
        self.dispose();
        lock_settings_manager(&self.settings_manager).set_default_model_and_provider(&model.provider, &model.id);
        (self.on_select)(model);
    }

    pub fn get_search_input(&self) -> &Input {
        &self.search_input
    }

    pub fn get_search_input_mut(&mut self) -> &mut Input {
        &mut self.search_input
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn scope(&self) -> &'static str {
        match self.scope {
            ModelScope::All => "all",
            ModelScope::Scoped => "scoped",
        }
    }
}

impl Focusable for ModelSelectorComponent {
    fn focused(&self) -> bool {
        self.focused
    }

    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.search_input.set_focused(focused);
    }
}

impl Component for ModelSelectorComponent {
    fn dispose(&mut self) {
        ModelSelectorComponent::dispose(self);
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = self.top_border.render(width);
        lines.extend(Spacer::new(1).render(width));
        if let Some(hint_text) = &mut self.hint_text {
            lines.extend(hint_text.render(width));
        }
        if let Some(scope_text) = &mut self.scope_text {
            lines.extend(scope_text.render(width));
        }
        if let Some(scope_hint_text) = &mut self.scope_hint_text {
            lines.extend(scope_hint_text.render(width));
        }
        lines.extend(Spacer::new(1).render(width));
        lines.extend(self.search_input.render(width));
        lines.extend(Spacer::new(1).render(width));
        lines.extend(self.list_container.render_children(width));
        lines.extend(Spacer::new(1).render(width));
        lines.extend(self.bottom_border.render(width));
        lines
    }

    fn handle_input(&mut self, data: &JsString) {
        let keybindings = pi_tui::get_keybindings();
        if keybindings.matches(data, "tui.input.tab") {
            if !self.scoped_model_items.is_empty() {
                let next_scope = if self.scope == ModelScope::All {
                    ModelScope::Scoped
                } else {
                    ModelScope::All
                };
                self.set_scope(next_scope);
                let scope_hint_value = self.get_scope_hint_text();
                if let Some(scope_hint_text) = &mut self.scope_hint_text {
                    scope_hint_text.set_text(scope_hint_value);
                }
            }
            return;
        }
        if keybindings.matches(data, "tui.select.up") {
            if self.filtered_models.is_empty() {
                return;
            }
            self.selected_index = if self.selected_index == 0 {
                self.filtered_models.len() - 1
            } else {
                self.selected_index - 1
            };
            self.update_list();
        } else if keybindings.matches(data, "tui.select.down") {
            if self.filtered_models.is_empty() {
                return;
            }
            self.selected_index = if self.selected_index == self.filtered_models.len() - 1 {
                0
            } else {
                self.selected_index + 1
            };
            self.update_list();
        } else if keybindings.matches(data, "tui.select.confirm") {
            if let Some(selected_model) = self.filtered_models.get(self.selected_index) {
                let model = selected_model.model.clone();
                self.handle_select(&model);
            }
        } else if keybindings.matches(data, "tui.select.cancel") {
            self.dispose();
            (self.on_cancel)();
        } else {
            self.search_input.handle_input(data);
            let query = self.search_input.get_value().clone();
            self.filter_models(&query);
        }
    }

    fn invalidate(&mut self) {
        if let Some(hint_text) = &mut self.hint_text {
            hint_text.invalidate();
        }
        if let Some(scope_text) = &mut self.scope_text {
            scope_text.invalidate();
        }
        if let Some(scope_hint_text) = &mut self.scope_hint_text {
            scope_hint_text.invalidate();
        }
        self.search_input.invalidate();
        self.list_container.invalidate_children();
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        Some(self)
    }
}
