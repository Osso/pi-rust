#[path = "support/oracle.rs"]
mod oracle;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pi_ai::types::Model;
use pi_coding_agent::modes::interactive::components::scoped_models_selector::{
    ModelsCallbacks, ModelsConfig, ScopedModelsSelectorComponent,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_tui::Component;
use serde_json::{Value, json};

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

fn model(id: &str, name: &str, provider: &str) -> Model {
    serde_json::from_value(json!({
        "id": id,
        "name": name,
        "api": "openai-completions",
        "provider": provider,
        "baseUrl": "https://example.test/v1",
        "reasoning": true,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 10000,
        "maxTokens": 1000
    }))
    .expect("model")
}

#[test]
fn scoped_models_selector_matches_the_pinned_typescript_source() {
    let theme = install_theme();
    pi_tui::set_keybindings(pi_tui::KeybindingsManager::new(
        pi_coding_agent::core::keybindings::keybindings(),
        Vec::new(),
    ));
    let mut oracle = oracle::TsOracle::spawn("interactive-components-oracle.ts", "packages/coding-agent/src");

    let models = vec![
        model("one", "One", "alpha"),
        model("two", "Two", "alpha"),
        model("three", "Three", "beta"),
    ];
    let request_models: Vec<Value> = models
        .iter()
        .map(|model| json!({"id": model.id, "name": model.name, "provider": model.provider}))
        .collect();

    let cases: Vec<(Option<Vec<String>>, Option<&str>, Vec<&str>)> = vec![
        (None, None, vec![]),
        (
            Some(vec!["alpha/one".to_string()]),
            Some("Refreshing…"),
            vec!["\u{1b}[B", "\r", "\u{13}"],
        ),
        (
            Some(vec!["alpha/two".to_string(), "alpha/one".to_string()]),
            None,
            vec!["\u{1b}[1;3B", "\u{01}", "\u{18}"],
        ),
        (
            Some(vec!["beta/three".to_string(), "alpha/one".to_string()]),
            None,
            vec!["t", "w", "\u{10}", "\u{1b}"],
        ),
    ];

    for (index, (enabled, refresh_status, inputs)) in cases.into_iter().enumerate() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut component = ScopedModelsSelectorComponent::new(
            ModelsConfig {
                all_models: models.clone(),
                enabled_model_ids: enabled.clone(),
                refresh_status: refresh_status.map(str::to_string),
            },
            ModelsCallbacks {
                on_change: {
                    let events = events.clone();
                    Rc::new(move |ids: Option<&[String]>| {
                        events.borrow_mut().push(json!({"change": ids}));
                    })
                },
                on_persist: {
                    let events = events.clone();
                    Rc::new(move |ids: Option<&[String]>| {
                        events.borrow_mut().push(json!({"persist": ids}));
                    })
                },
                on_cancel: {
                    let events = events.clone();
                    Rc::new(move || events.borrow_mut().push(json!({"cancel": true})))
                },
            },
            theme.clone(),
        );
        for input in &inputs {
            component.handle_input(input);
        }
        let lines = component.render(100);
        let expected = oracle.request(&json!({
            "id": 40 + index,
            "module": "modes/interactive/components/scoped-models-selector.ts",
            "operation": "scopedModels",
            "models": request_models,
            "enabledModelIds": enabled,
            "refreshStatus": refresh_status,
            "inputs": inputs,
        }));
        let actual = json!({"lines": lines, "events": events.borrow().clone()});
        assert_eq!(actual, expected["output"], "mismatch for case {index}");
    }
}
