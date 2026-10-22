//! Behavioral tests for `modes/interactive/components/model-selector.ts`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pi_ai::auth::InMemoryCredentialStore;
use pi_ai::auth::types::CredentialStore;
use pi_ai::types::Model;
use pi_coding_agent::core::model_resolver::ScopedModel;
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::provider_composer::ProviderConfigInput;
use pi_coding_agent::core::settings_manager::{
    Settings, SettingsManager, SettingsManagerCreateOptions, SharedSettingsManager, lock_settings_manager,
};
use pi_coding_agent::modes::interactive::components::model_selector::{ModelSelectorComponent, ModelSelectorUi};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_tui::utils::strip_terminal_sequences;
use pi_tui::{Component, KeybindingsManager, set_keybindings};
use serde_json::json;

struct TestUi {
    renders: RefCell<usize>,
}

impl ModelSelectorUi for TestUi {
    fn request_render(&self) {
        *self.renders.borrow_mut() += 1;
    }
}

fn install_theme() -> Arc<Theme> {
    let colors: Vec<(String, ColorValue)> = REQUIRED_COLORS
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                key.to_string(),
                ColorValue::Text(format!(
                    "#{:02x}{:02x}{:02x}",
                    (16 + 7 * index) % 256,
                    (32 + 11 * index) % 256,
                    (48 + 13 * index) % 256
                )),
            )
        })
        .collect();
    let theme = Arc::new(Theme::new(&colors, &colors, ColorMode::Truecolor, None, None).expect("test theme"));
    set_theme_instance(theme.clone());
    theme
}

fn model(id: &str, provider: &str) -> Model {
    serde_json::from_value(json!({
        "id": id,
        "name": format!("{id} display"),
        "api": "openai-completions",
        "provider": provider,
        "baseUrl": "https://example.test/v1",
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 10000,
        "maxTokens": 1000
    }))
    .expect("model")
}

async fn runtime() -> ModelRuntime {
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(InMemoryCredentialStore::new()) as Arc<dyn CredentialStore>),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .expect("runtime");
    for provider in ["zeta", "alpha"] {
        runtime
            .register_provider(
                provider,
                ProviderConfigInput {
                    name: Some(provider.to_string()),
                    base_url: Some("https://example.test/v1".to_string()),
                    api_key: Some("literal-key".to_string()),
                    api: Some("openai-completions".to_string()),
                    models: Some(vec![model(&format!("{provider}-one"), provider)]),
                    ..Default::default()
                },
            )
            .expect("provider");
    }
    runtime
}

fn settings() -> SharedSettingsManager {
    std::sync::Arc::new(std::sync::Mutex::new(SettingsManager::in_memory(
        Settings::new(),
        SettingsManagerCreateOptions::default(),
    )))
}

fn selector(
    runtime: ModelRuntime,
    current_model: Option<Model>,
    scoped_models: Vec<ScopedModel>,
    on_select: Rc<dyn Fn(&Model)>,
    on_cancel: Rc<dyn Fn()>,
) -> (ModelSelectorComponent, Rc<TestUi>, SharedSettingsManager) {
    set_keybindings(KeybindingsManager::new(
        pi_coding_agent::core::keybindings::keybindings(),
        Vec::new(),
    ));
    let ui = Rc::new(TestUi {
        renders: RefCell::new(0),
    });
    let settings = settings();
    let component = ModelSelectorComponent::new(
        ui.clone(),
        current_model,
        settings.clone(),
        runtime,
        scoped_models,
        on_select,
        on_cancel,
        None,
        install_theme(),
    );
    (component, ui, settings)
}

fn plain(component: &mut ModelSelectorComponent) -> String {
    component
        .render(120)
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn sorts_the_current_model_first_then_by_provider_and_marks_it() {
    let runtime = runtime().await;
    let current = model("zeta-one", "zeta");
    let (mut component, ui, _settings) = selector(runtime, Some(current), vec![], Rc::new(|_| {}), Rc::new(|| {}));
    let output = plain(&mut component);
    assert!(output.contains("→ zeta-one [zeta] ✓"), "{output}");
    let zeta = output.find("→ zeta-one [zeta] ✓").expect("current row");
    assert!(output.contains("  alpha-one [alpha]"), "{output}");
    let alpha = output.find("  alpha-one [alpha]").expect("alpha row");
    assert!(zeta < alpha, "{output}");
    assert!(output.contains("Model Name: zeta-one display"), "{output}");
    assert!(
        output.contains("Only showing models from configured providers"),
        "{output}"
    );
    assert!(*ui.renders.borrow() >= 1);
}

#[tokio::test]
async fn search_filters_models_and_resets_selection_to_the_top() {
    let runtime = runtime().await;
    let current = model("zeta-one", "zeta");
    let (mut component, _ui, _settings) = selector(runtime, Some(current), vec![], Rc::new(|_| {}), Rc::new(|| {}));
    component.handle_input("\u{1b}[B");
    assert_eq!(component.selected_index(), 1);

    for character in ["a", "l", "p", "h", "a"] {
        component.handle_input(character);
    }
    assert_eq!(component.selected_index(), 0);
    let output = plain(&mut component);
    assert!(output.contains("→ alpha-one [alpha]"), "{output}");
    assert!(!output.contains("zeta-one"), "{output}");
}

#[tokio::test]
async fn tab_toggles_scope_and_marks_the_current_scope() {
    let runtime = runtime().await;
    let current = model("alpha-one", "alpha");
    let scoped = vec![ScopedModel {
        model: current.clone(),
        thinking_level: None,
    }];
    let (mut component, _ui, _settings) = selector(runtime, Some(current), scoped, Rc::new(|_| {}), Rc::new(|| {}));
    assert_eq!(component.scope(), "scoped");
    let output = plain(&mut component);
    assert!(output.contains("Scope:"), "{output}");
    assert!(output.contains("all | scoped"), "{output}");
    assert!(output.contains("scope"), "{output}");
    assert!(!output.contains("zeta-one"), "{output}");

    component.handle_input("\t");
    assert_eq!(component.scope(), "all");
    let output = plain(&mut component);
    assert!(output.contains("zeta-one"), "{output}");
}

#[tokio::test]
async fn confirm_saves_the_default_model_and_reports_the_selection() {
    let runtime = runtime().await;
    let current = model("zeta-one", "zeta");
    let selected = Rc::new(RefCell::new(Vec::new()));
    let (mut component, _ui, settings) = selector(
        runtime,
        Some(current),
        vec![],
        {
            let selected = selected.clone();
            Rc::new(move |model: &Model| {
                selected.borrow_mut().push((model.provider.clone(), model.id.clone()));
            })
        },
        Rc::new(|| {}),
    );
    component.handle_input("\u{1b}[B");
    component.handle_input("\r");
    assert_eq!(*selected.borrow(), vec![("alpha".to_string(), "alpha-one".to_string())]);
    assert_eq!(lock_settings_manager(&settings).get_default_provider(), Some("alpha"));
    assert_eq!(lock_settings_manager(&settings).get_default_model(), Some("alpha-one"));
}

#[tokio::test]
async fn cancel_reports_cancellation() {
    let runtime = runtime().await;
    let cancelled = Rc::new(RefCell::new(0));
    let (mut component, _ui, _settings) = selector(runtime, None, vec![], Rc::new(|_| {}), {
        let cancelled = cancelled.clone();
        Rc::new(move || *cancelled.borrow_mut() += 1)
    });
    component.handle_input("\u{1b}");
    assert_eq!(*cancelled.borrow(), 1);
}

#[tokio::test]
async fn renders_no_matching_models_when_the_query_matches_nothing() {
    let runtime = runtime().await;
    let (mut component, _ui, _settings) = selector(runtime, None, vec![], Rc::new(|_| {}), Rc::new(|| {}));
    for character in ["z", "z", "z"] {
        component.handle_input(character);
    }
    assert!(plain(&mut component).contains("No matching models"));
}

#[tokio::test]
async fn up_and_down_wrap_around_the_filtered_list() {
    let runtime = runtime().await;
    let (mut component, _ui, _settings) = selector(runtime, None, vec![], Rc::new(|_| {}), Rc::new(|| {}));
    component.handle_input("\u{1b}[A");
    assert_eq!(component.selected_index(), 1);
    component.handle_input("\u{1b}[B");
    assert_eq!(component.selected_index(), 0);
}

#[tokio::test]
async fn native_search_retains_unmatched_units_without_selecting_a_model() {
    use pi_js_compat::js_json::JsString;
    let (mut component, _, _) = selector(
        runtime().await,
        None,
        vec![],
        Rc::new(|_| panic!("unmatched query must not select")),
        Rc::new(|| {}),
    );
    let raw = JsString::from_utf16(vec![0xd800]);
    component.handle_input(&raw);
    assert_eq!(component.get_search_input().get_value(), raw);
    assert!(JsString::join(component.render(120), "\n").contains("No matching models"));
    component.handle_input(&"\r".into());
    component.dispose();
}
