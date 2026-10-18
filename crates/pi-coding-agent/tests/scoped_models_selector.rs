//! Behavioral tests for `modes/interactive/components/scoped-models-selector.ts`.
//! Ports the component cases of regressions 3217 and 6949.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use pi_ai::types::Model;
use pi_coding_agent::modes::interactive::components::scoped_models_selector::{
    ModelsCallbacks, ModelsConfig, RefreshStatusKind, ScopedModelsSelectorComponent,
};
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_tui::{Component, KeybindingsManager, set_keybindings, strip_terminal_sequences};
use serde_json::json;

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

struct Recorder {
    changes: RefCell<Vec<Option<Vec<String>>>>,
    persisted: RefCell<Vec<Option<Vec<String>>>>,
    cancels: RefCell<usize>,
}

fn callbacks(recorder: Rc<Recorder>) -> ModelsCallbacks {
    let change_recorder = recorder.clone();
    let persist_recorder = recorder.clone();
    let cancel_recorder = recorder;
    ModelsCallbacks {
        on_change: Rc::new(move |ids| {
            change_recorder.changes.borrow_mut().push(ids.map(<[String]>::to_vec));
        }),
        on_persist: Rc::new(move |ids| {
            persist_recorder
                .persisted
                .borrow_mut()
                .push(ids.map(<[String]>::to_vec));
        }),
        on_cancel: Rc::new(move || *cancel_recorder.cancels.borrow_mut() += 1),
    }
}

fn selector(models: Vec<Model>, enabled: Option<Vec<String>>, recorder: Rc<Recorder>) -> ScopedModelsSelectorComponent {
    set_keybindings(KeybindingsManager::new(
        pi_coding_agent::core::keybindings::keybindings(),
        Vec::new(),
    ));
    ScopedModelsSelectorComponent::new(
        ModelsConfig {
            all_models: models,
            enabled_model_ids: enabled,
            refresh_status: None,
        },
        callbacks(recorder),
        install_theme(),
    )
}

fn plain(component: &mut ScopedModelsSelectorComponent) -> String {
    component
        .render(100)
        .iter()
        .map(|line| strip_terminal_sequences(line))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn persists_scoped_model_choices_with_the_save_key() {
    let models = vec![model("faux-1", "One", "faux"), model("faux-2", "Two", "faux")];
    let ordered = vec!["faux/faux-1".to_string(), "faux/faux-2".to_string()];
    let recorder = Rc::new(Recorder {
        changes: RefCell::new(Vec::new()),
        persisted: RefCell::new(Vec::new()),
        cancels: RefCell::new(0),
    });
    let mut component = selector(models, Some(ordered.clone()), recorder.clone());
    component.handle_input("\u{13}");
    assert_eq!(*recorder.persisted.borrow(), vec![Some(ordered)]);
}

#[test]
fn propagates_reordered_scoped_models_back_to_session_state() {
    let models = vec![
        model("faux-1", "One", "faux"),
        model("faux-2", "Two", "faux"),
        model("faux-3", "Three", "faux"),
    ];
    let ordered = vec![
        "faux/faux-1".to_string(),
        "faux/faux-2".to_string(),
        "faux/faux-3".to_string(),
    ];
    let recorder = Rc::new(Recorder {
        changes: RefCell::new(Vec::new()),
        persisted: RefCell::new(Vec::new()),
        cancels: RefCell::new(0),
    });
    let mut component = selector(models, Some(ordered.clone()), recorder.clone());
    component.handle_input("\u{1b}[1;3B");
    assert_eq!(
        *recorder.changes.borrow(),
        vec![Some(vec![
            "faux/faux-2".to_string(),
            "faux/faux-1".to_string(),
            "faux/faux-3".to_string()
        ])]
    );
    assert_eq!(component.selected_index(), 1);
}

#[test]
fn shows_and_removes_an_enabled_model_without_a_catalog_entry() {
    let models = vec![model("available", "Available", "faux")];
    let available_id = "faux/available".to_string();
    let unavailable_id = "faux/unavailable".to_string();
    let recorder = Rc::new(Recorder {
        changes: RefCell::new(Vec::new()),
        persisted: RefCell::new(Vec::new()),
        cancels: RefCell::new(0),
    });
    let mut component = selector(
        models,
        Some(vec![unavailable_id.clone(), available_id.clone()]),
        recorder.clone(),
    );

    let output = plain(&mut component);
    assert!(
        output.contains(&format!("{unavailable_id} [unavailable] ✗")),
        "{output}"
    );
    assert!(output.contains("Model unavailable"), "{output}");

    component.handle_input("\r");
    assert_eq!(*recorder.changes.borrow(), vec![Some(vec![available_id.clone()])]);

    component.handle_input("\u{13}");
    assert_eq!(*recorder.persisted.borrow(), vec![Some(vec![available_id])]);
}

#[test]
fn toggles_enables_clears_and_toggles_providers() {
    let models = vec![
        model("one", "One", "alpha"),
        model("two", "Two", "alpha"),
        model("three", "Three", "beta"),
    ];
    let recorder = Rc::new(Recorder {
        changes: RefCell::new(Vec::new()),
        persisted: RefCell::new(Vec::new()),
        cancels: RefCell::new(0),
    });
    let mut component = selector(models, None, recorder.clone());

    component.handle_input("\r");
    assert_eq!(*recorder.changes.borrow(), vec![Some(vec!["alpha/one".to_string()])]);

    component.handle_input("\u{01}");
    assert_eq!(
        *recorder.changes.borrow(),
        vec![Some(vec!["alpha/one".to_string()]), None]
    );

    component.handle_input("\u{18}");
    assert_eq!(
        *recorder.changes.borrow(),
        vec![Some(vec!["alpha/one".to_string()]), None, Some(Vec::new())]
    );

    component.handle_input("\u{10}");
    assert_eq!(
        *recorder.changes.borrow(),
        vec![
            Some(vec!["alpha/one".to_string()]),
            None,
            Some(Vec::new()),
            Some(vec!["alpha/one".to_string(), "alpha/two".to_string()])
        ]
    );
}

#[test]
fn search_filters_items_and_footer_reports_counts_and_dirty_state() {
    let models = vec![model("one", "One", "alpha"), model("two", "Two", "beta")];
    let recorder = Rc::new(Recorder {
        changes: RefCell::new(Vec::new()),
        persisted: RefCell::new(Vec::new()),
        cancels: RefCell::new(0),
    });
    let mut component = selector(models, Some(vec!["alpha/one".to_string()]), recorder);

    let output = plain(&mut component);
    assert!(output.contains("1/2 enabled"), "{output}");
    assert!(output.contains("ctrl+s/ctrl+enter save"), "{output}");
    assert!(!output.contains("(unsaved)"), "{output}");

    for character in ["t", "w", "o"] {
        component.handle_input(character);
    }
    let output = plain(&mut component);
    assert!(output.contains("→ two [beta] ✗"), "{output}");
    assert!(!output.contains("one [alpha]"), "{output}");

    component.handle_input("\r");
    let output = plain(&mut component);
    assert!(output.contains("(unsaved)"), "{output}");
}

#[test]
fn escape_and_ctrl_c_cancel_or_clear_search() {
    let models = vec![model("one", "One", "alpha")];
    let recorder = Rc::new(Recorder {
        changes: RefCell::new(Vec::new()),
        persisted: RefCell::new(Vec::new()),
        cancels: RefCell::new(0),
    });
    let mut component = selector(models, None, recorder.clone());
    component.handle_input("a");
    component.handle_input("\u{03}");
    assert_eq!(*recorder.cancels.borrow(), 0);
    assert_eq!(component.get_search_input().get_value(), "");
    component.handle_input("\u{03}");
    assert_eq!(*recorder.cancels.borrow(), 1);
    component.handle_input("\u{1b}");
    assert_eq!(*recorder.cancels.borrow(), 2);
}

#[test]
fn update_models_keeps_selection_and_refresh_status_updates_text() {
    let models = vec![model("one", "One", "alpha"), model("two", "Two", "beta")];
    let recorder = Rc::new(Recorder {
        changes: RefCell::new(Vec::new()),
        persisted: RefCell::new(Vec::new()),
        cancels: RefCell::new(0),
    });
    let mut component = ScopedModelsSelectorComponent::new(
        ModelsConfig {
            all_models: models,
            enabled_model_ids: Some(vec!["beta/two".to_string()]),
            refresh_status: Some("loading".to_string()),
        },
        callbacks(recorder),
        install_theme(),
    );
    component.handle_input("\u{1b}[B");
    assert_eq!(component.selected_index(), 1);

    component.update_models(
        &[
            model("one", "One", "alpha"),
            model("two", "Two", "beta"),
            model("three", "Three", "gamma"),
        ],
        Some(Some(vec!["beta/two".to_string(), "gamma/three".to_string()])),
    );
    assert_eq!(component.selected_index(), 2);
    assert_eq!(
        component.enabled_model_ids(),
        Some(&["beta/two".to_string(), "gamma/three".to_string()][..])
    );

    component.set_refresh_status("refreshed", RefreshStatusKind::Success);
    assert!(plain(&mut component).contains("refreshed"));
}
