//! Whole common-pin `components/model-selector.ts`: synchronous registry refresh,
//! deferred model publication and inherited Container/Input behavior.

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use pi_ai::{models::models_are_equal, types::Model};
use pi_js_compat::{js_format, js_json::JsString, locale_compare};
use pi_tui::components::input::InputOptions;
use pi_tui::tui::component;
use pi_tui::{Component, Container, Focusable, Input, Spacer, Text, fuzzy_filter};

use crate::core::{model_registry::ModelRegistry, model_resolver::ScopedModel};
use crate::modes::interactive::{
    components::{dynamic_border::dynamic_border, keybinding_hints::key_hint},
    model_search::{ModelSearchItem, get_model_search_candidates},
    theme::{ThemeColor, with_theme},
};

/// The selector's sole TUI dependency.
pub trait ModelSelectorUi {
    fn request_render(&self);
}

#[derive(Clone)]
struct ModelItem {
    provider: String,
    id: String,
    model: Model,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModelScope {
    All,
    Scoped,
}

/// Shared with Input.onSubmit so submitting the actual child never reborrows its parent.
#[derive(Default)]
struct Selection {
    models: Vec<ModelItem>,
    index: usize,
}
impl Selection {
    fn selected_model(&self) -> Option<Model> {
        self.models.get(self.index).map(|item| item.model.clone())
    }
}

pub struct ModelSelectorComponent {
    container: Container,
    search_input: Rc<RefCell<Input>>,
    list_container: Rc<RefCell<Container>>,
    all_models: Vec<ModelItem>,
    scoped_model_items: Vec<ModelItem>,
    active_models: Vec<ModelItem>,
    selection: Rc<RefCell<Selection>>,
    current_model: Option<Model>,
    model_registry: ModelRegistry,
    on_select: Rc<dyn Fn(&Model)>,
    on_cancel: Rc<dyn Fn()>,
    error_message: Option<JsString>,
    ui: Rc<dyn ModelSelectorUi>,
    scoped_models: Vec<ScopedModel>,
    scope: ModelScope,
    scope_text: Option<Rc<RefCell<Text>>>,
    scope_hint_text: Option<Rc<RefCell<Text>>>,
    focused: bool,
}

impl ModelSelectorComponent {
    pub fn new(
        ui: Rc<dyn ModelSelectorUi>,
        current_model: Option<Model>,
        model_registry: ModelRegistry,
        scoped_models: Vec<ScopedModel>,
        on_select: Rc<dyn Fn(&Model)>,
        on_cancel: Rc<dyn Fn()>,
        initial_search_input: Option<&JsString>,
    ) -> Rc<RefCell<Self>> {
        let search_input = Rc::new(RefCell::new(Input::new(InputOptions::default())));
        if let Some(initial) = initial_search_input.filter(|value| !value.is_empty()) {
            search_input.borrow_mut().set_value(initial);
        }
        let selection = Rc::new(RefCell::new(Selection::default()));
        let submitted = selection.clone();
        let select = on_select.clone();
        search_input.borrow_mut().on_submit = Some(Rc::new(move |_| {
            let model = submitted.borrow().selected_model();
            if let Some(model) = model {
                select(&model);
            }
        }));
        let scope = if scoped_models.is_empty() {
            ModelScope::All
        } else {
            ModelScope::Scoped
        };
        let mut selector = Self {
            container: Container::new(),
            search_input,
            list_container: Rc::new(RefCell::new(Container::new())),
            all_models: vec![],
            scoped_model_items: vec![],
            active_models: vec![],
            selection,
            current_model,
            model_registry,
            on_select,
            on_cancel,
            error_message: None,
            ui,
            scoped_models,
            scope,
            scope_text: None,
            scope_hint_text: None,
            focused: false,
        };
        selector.mount_children();
        // Frozen138 refresh is outside the getter catch. Its rejection is
        // unhandled after construction; eager work still precedes the first await.
        let models = selector.model_registry.refresh().map(|()| {
            selector.error_message = selector.model_registry.get_error().filter(|error| !error.is_empty());
            selector.model_registry.get_available()
        });
        let selector = Rc::new(RefCell::new(selector));
        let pending = selector.clone();
        let initial = initial_search_input.cloned();
        tokio::task::spawn_local(async move {
            tokio::task::yield_now().await;
            let models = match models {
                Ok(models) => models,
                Err(error) => crate::utils::uncaught::throw_callback_uncaught(error),
            };
            let ui = {
                let mut selector = pending.borrow_mut();
                match models {
                    Ok(models) => {
                        selector.publish_models(models);
                        if let Some(initial) = initial.filter(|value| !value.is_empty()) {
                            selector.filter_models(&initial);
                        } else {
                            selector.update_list();
                        }
                    }
                    Err(error) => {
                        selector.error_message = Some(error.message);
                        selector.update_list();
                    }
                }
                selector.ui.clone()
            };
            ui.request_render();
        });
        selector
    }

    fn mount_children(&mut self) {
        self.container.add_child(component(dynamic_border()));
        self.container.add_child(component(Spacer::new(1)));
        if self.scoped_models.is_empty() {
            self.container.add_child(component(Text::new(
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Warning,
                        "Only showing models from configured providers. Use /login to add providers.",
                    )
                }),
                0,
                0,
            )));
        } else {
            let scope = Rc::new(RefCell::new(Text::new(self.get_scope_text(), 0, 0)));
            let hint = Rc::new(RefCell::new(Text::new(self.get_scope_hint_text(), 0, 0)));
            self.container.add_child(scope.clone());
            self.container.add_child(hint.clone());
            self.scope_text = Some(scope);
            self.scope_hint_text = Some(hint);
        }
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(self.search_input.clone());
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(self.list_container.clone());
        self.container.add_child(component(Spacer::new(1)));
        self.container.add_child(component(dynamic_border()));
    }

    fn publish_models(&mut self, models: Vec<Model>) {
        self.all_models = self.sort_models(models.into_iter().map(ModelItem::from).collect());
        for scoped in &mut self.scoped_models {
            if let Some(model) = self.model_registry.find(&scoped.model.provider, &scoped.model.id) {
                scoped.model = model;
            }
        }
        self.scoped_model_items = self
            .scoped_models
            .iter()
            .map(|scoped| ModelItem::from(scoped.model.clone()))
            .collect();
        self.active_models = if self.scope == ModelScope::Scoped {
            self.scoped_model_items.clone()
        } else {
            self.all_models.clone()
        };
        let mut selection = self.selection.borrow_mut();
        selection.models = self.active_models.clone();
        selection.index = self
            .current_index(&selection.models)
            .unwrap_or_else(|| selection.index.min(selection.models.len().saturating_sub(1)));
    }

    fn current_index(&self, models: &[ModelItem]) -> Option<usize> {
        models
            .iter()
            .position(|item| models_are_equal(self.current_model.as_ref(), Some(&item.model)))
    }

    fn sort_models(&self, mut models: Vec<ModelItem>) -> Vec<ModelItem> {
        models.sort_by(|a, b| {
            let current = |item: &ModelItem| models_are_equal(self.current_model.as_ref(), Some(&item.model));
            current(b)
                .cmp(&current(a))
                .then_with(|| locale_compare(&a.provider, &b.provider))
        });
        models
    }

    fn get_scope_text(&self) -> JsString {
        with_theme(|theme| {
            let all = theme.fg(
                if self.scope == ModelScope::All {
                    ThemeColor::Accent
                } else {
                    ThemeColor::Muted
                },
                "all",
            );
            let scoped = theme.fg(
                if self.scope == ModelScope::Scoped {
                    ThemeColor::Accent
                } else {
                    ThemeColor::Muted
                },
                "scoped",
            );
            js_format!(
                "{}{}{}{}",
                theme.fg(ThemeColor::Muted, "Scope: "),
                all,
                theme.fg(ThemeColor::Muted, " | "),
                scoped
            )
        })
    }
    fn get_scope_hint_text(&self) -> JsString {
        key_hint("tui.input.tab", "scope") + with_theme(|theme| theme.fg(ThemeColor::Muted, " (all/scoped)"))
    }
    fn set_scope(&mut self, scope: ModelScope) {
        if self.scope == scope {
            return;
        }
        self.scope = scope;
        self.active_models = if scope == ModelScope::Scoped {
            self.scoped_model_items.clone()
        } else {
            self.all_models.clone()
        };
        self.selection.borrow_mut().index = self.current_index(&self.active_models).unwrap_or(0);
        let query = self.search_input.borrow().get_value().clone();
        self.filter_models(&query);
        if let Some(text) = &self.scope_text {
            text.borrow_mut().set_text(self.get_scope_text());
        }
    }
    fn filter_models(&mut self, query: &JsString) {
        let filtered = if query.is_empty() {
            self.active_models.clone()
        } else {
            fuzzy_filter(&self.active_models, query, |item| {
                get_model_search_candidates(&ModelSearchItem {
                    id: item.id.clone(),
                    provider: item.provider.clone(),
                    name: Some(item.model.name.clone()),
                })
            })
        };
        {
            let mut selection = self.selection.borrow_mut();
            selection.models = filtered;
            selection.index = selection.index.min(selection.models.len().saturating_sub(1));
        }
        self.update_list();
    }

    fn model_line(&self, item: &ModelItem, index: usize, selected: usize) -> JsString {
        with_theme(|theme| {
            let current = models_are_equal(self.current_model.as_ref(), Some(&item.model));
            let badge = theme.fg(ThemeColor::Muted, js_format!("[{}]", item.provider));
            let check = if current {
                theme.fg(ThemeColor::Success, " ✓")
            } else {
                JsString::default()
            };
            let model = if index == selected {
                theme.fg(ThemeColor::Accent, "→ ") + theme.fg(ThemeColor::Accent, item.id.as_str())
            } else {
                js_format!("  {}", item.id)
            };
            js_format!("{} {}{}", model, badge, check)
        })
    }

    fn update_list(&mut self) {
        let selection = self.selection.borrow();
        let mut list = self.list_container.borrow_mut();
        list.clear();
        let max_visible = 10;
        let start = (selection.index as isize - 5)
            .min(selection.models.len() as isize - max_visible as isize)
            .max(0) as usize;
        let end = (start + max_visible).min(selection.models.len());
        for index in start..end {
            list.add_child(component(Text::new(
                self.model_line(&selection.models[index], index, selection.index),
                0,
                0,
            )));
        }
        if start > 0 || end < selection.models.len() {
            let line = with_theme(|theme| {
                theme.fg(
                    ThemeColor::Muted,
                    js_format!("  ({}/{})", selection.index + 1, selection.models.len()),
                )
            });
            list.add_child(component(Text::new(line, 0, 0)));
        }
        if let Some(error) = &self.error_message {
            for line in error.split('\n') {
                list.add_child(component(Text::new(
                    with_theme(|theme| theme.fg(ThemeColor::Error, line)),
                    0,
                    0,
                )));
            }
        } else if selection.models.is_empty() {
            list.add_child(component(Text::new(
                with_theme(|theme| theme.fg(ThemeColor::Muted, "  No matching models")),
                0,
                0,
            )));
        } else {
            list.add_child(component(Spacer::new(1)));
            list.add_child(component(Text::new(
                with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Muted,
                        js_format!("  Model Name: {}", selection.models[selection.index].model.name),
                    )
                }),
                0,
                0,
            )));
        }
    }

    pub fn get_search_input(&self) -> Rc<RefCell<Input>> {
        self.search_input.clone()
    }
    pub fn selected_index(&self) -> usize {
        self.selection.borrow().index
    }
    pub fn scope(&self) -> &'static str {
        if self.scope == ModelScope::All { "all" } else { "scoped" }
    }
}

impl From<Model> for ModelItem {
    fn from(model: Model) -> Self {
        Self {
            provider: model.provider.clone(),
            id: model.id.clone(),
            model,
        }
    }
}
impl Deref for ModelSelectorComponent {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.container
    }
}
impl DerefMut for ModelSelectorComponent {
    fn deref_mut(&mut self) -> &mut Container {
        &mut self.container
    }
}
impl Focusable for ModelSelectorComponent {
    fn focused(&self) -> bool {
        self.focused
    }
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        self.search_input.borrow_mut().set_focused(focused);
    }
}
impl Component for ModelSelectorComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }
    fn handle_input(&mut self, data: &JsString) {
        let kb = pi_tui::get_keybindings();
        if kb.matches(data, "tui.input.tab") {
            if !self.scoped_model_items.is_empty() {
                self.set_scope(if self.scope == ModelScope::All {
                    ModelScope::Scoped
                } else {
                    ModelScope::All
                });
                if let Some(text) = &self.scope_hint_text {
                    text.borrow_mut().set_text(self.get_scope_hint_text());
                }
            }
            return;
        }
        if kb.matches(data, "tui.select.up") || kb.matches(data, "tui.select.down") {
            {
                let mut selection = self.selection.borrow_mut();
                let length = selection.models.len();
                if length == 0 {
                    return;
                }
                selection.index = if kb.matches(data, "tui.select.up") {
                    if selection.index == 0 {
                        length - 1
                    } else {
                        selection.index - 1
                    }
                } else {
                    if selection.index == length - 1 {
                        0
                    } else {
                        selection.index + 1
                    }
                };
            }
            self.update_list();
        } else if kb.matches(data, "tui.select.confirm") {
            let model = self.selection.borrow().selected_model();
            if let Some(model) = model {
                (self.on_select)(&model);
            }
        } else if kb.matches(data, "tui.select.cancel") {
            (self.on_cancel)();
        } else {
            self.search_input.borrow_mut().handle_input(data);
            let query = self.search_input.borrow().get_value().clone();
            self.filter_models(&query);
        }
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
}
