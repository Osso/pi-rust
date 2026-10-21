//! Behavioral tests for the llama.cpp manager component (`extensions/llama/ui.ts`).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_coding_agent::bundled_extensions::llama::client::LlamaModelInfo;
use pi_coding_agent::bundled_extensions::llama::huggingface::HuggingFaceModel;
use pi_coding_agent::bundled_extensions::llama::ui::{
    ConnectionErrorChoice, HuggingFaceSearchFn, LlamaManagerAction, LlamaUi, LlamaView, ProgressState,
};
use pi_coding_agent::core::extensions::types::{ExtensionUiTui, ExtensionUiTuiBackend};
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_coding_agent::utils::ansi::strip_ansi;
use pi_tui::tui::TuiMode;
use serde_json::json;

struct TestBackend {
    renders: AtomicUsize,
}

impl ExtensionUiTuiBackend for TestBackend {
    fn show_overlay(
        &self,
        _component: Box<dyn pi_coding_agent::core::extensions::ExtensionUiComponent>,
        _options: Option<pi_coding_agent::core::extensions::ExtensionUiOverlayOptions>,
    ) -> Arc<dyn pi_coding_agent::core::extensions::ExtensionUiOverlayHandle> {
        panic!("this fixture does not mount nested overlays")
    }

    fn request_render_force(&self) {
        panic!("fixture does not force redraw")
    }
    fn stop(&self) {
        panic!("fixture does not release terminal")
    }
    fn start(&self) -> Result<(), String> {
        panic!("fixture does not acquire terminal")
    }

    fn create_fixed_cell(&self) -> std::rc::Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("this component fixture must not construct an editor")
    }
    fn request_render(&self) {
        self.renders.fetch_add(1, Ordering::SeqCst);
    }

    fn terminal_rows(&self) -> usize {
        40
    }

    fn terminal_columns(&self) -> usize {
        80
    }

    fn mode(&self) -> TuiMode {
        TuiMode::Regular
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
    set_theme_instance(Arc::clone(&theme));
    theme
}

fn view() -> Arc<LlamaView> {
    let theme = install_theme();
    let backend = Arc::new(TestBackend {
        renders: AtomicUsize::new(0),
    });
    Arc::new(LlamaView::new(
        ExtensionUiTui::new(backend),
        theme,
        KeybindingsManager::new(Vec::new(), None),
    ))
}

fn text(lines: &[String]) -> Vec<String> {
    lines.iter().map(|line| strip_ansi(line)).collect()
}

fn model(id: &str, status: &str) -> LlamaModelInfo {
    serde_json::from_value(json!({"id": id, "status": {"value": status}})).unwrap()
}

fn assert_contains(lines: &[String], needle: &str) {
    assert!(
        lines.iter().any(|line| line.contains(needle)),
        "missing {needle:?} in {lines:#?}"
    );
}

#[tokio::test]
async fn renders_models_and_resolves_selection() {
    let view = view();
    let mut loaded = model("loaded", "loaded");
    loaded.meta = serde_json::from_value(json!({"n_ctx": 65536})).unwrap();
    let future = Box::pin(view.show_models(
        "http://127.0.0.1:8080".to_owned(),
        vec![model("unloaded", "unloaded"), loaded],
    ));
    let lines = text(&view.render(80));
    assert_contains(&lines, "llama.cpp models");
    assert_contains(&lines, "http://127.0.0.1:8080");
    assert_contains(&lines, "loaded");
    assert_contains(&lines, "66k context");
    assert_contains(&lines, "Download model…");

    view.handle_input("\x1b[B");
    view.handle_input("\r");
    match future.await {
        LlamaManagerAction::Model(model) => assert_eq!(model.id, "unloaded"),
        other => panic!("expected the unloaded model, got {other:?}"),
    }
}

#[tokio::test]
async fn download_row_and_cancel_resolve_actions() {
    let view = view();
    let future = Box::pin(view.show_models("http://127.0.0.1:8080".to_owned(), vec![model("m", "loaded")]));
    view.handle_input("\x1b[B");
    view.handle_input("\r");
    assert_eq!(future.await, LlamaManagerAction::Download);

    let future = Box::pin(view.show_models("http://127.0.0.1:8080".to_owned(), vec![model("m", "loaded")]));
    view.handle_input("\x1b");
    assert_eq!(future.await, LlamaManagerAction::Close);
}

#[tokio::test]
async fn select_and_confirm_use_yes_no_options() {
    let view = view();
    let future = Box::pin(view.select("Pick one".to_owned(), vec!["A".to_owned(), "B".to_owned()]));
    assert_contains(&text(&view.render(80)), "Pick one");
    view.handle_input("\x1b[B");
    view.handle_input("\r");
    assert_eq!(future.await, Some("B".to_owned()));

    let future = Box::pin(view.select("Pick one".to_owned(), vec!["A".to_owned()]));
    view.handle_input("\x1b");
    assert_eq!(future.await, None);

    let confirm_view = Arc::clone(&view);
    let confirm = tokio::spawn(async move { confirm_view.confirm("Unload model?".to_owned(), "m".to_owned()).await });
    tokio::task::yield_now().await;
    assert_contains(&text(&view.render(80)), "Unload model?");
    view.handle_input("\r");
    assert!(confirm.await.unwrap());

    let error_view = Arc::clone(&view);
    let error = tokio::spawn(async move {
        error_view
            .connection_error("http://x".to_owned(), "boom".to_owned())
            .await
    });
    tokio::task::yield_now().await;
    assert_contains(&text(&view.render(80)), "llama.cpp unavailable");
    view.handle_input("\r");
    assert_eq!(error.await.unwrap(), ConnectionErrorChoice::Retry);
}

fn search_fn(calls: Arc<Mutex<Vec<String>>>) -> HuggingFaceSearchFn {
    Arc::new(move |query, _signal| {
        calls.lock().unwrap().push(query);
        Box::pin(async {
            Ok(vec![
                HuggingFaceModel {
                    id: "owner/first-GGUF".to_owned(),
                    downloads: 1200.0,
                },
                HuggingFaceModel {
                    id: "owner/second-GGUF".to_owned(),
                    downloads: 7.0,
                },
            ])
        })
    })
}

#[tokio::test]
async fn search_debounces_and_uses_the_cache() {
    let view = view();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let future = Box::pin(view.search_models(search_fn(Arc::clone(&calls))));
    assert_contains(&text(&view.render(80)), "Type at least 2 characters");

    view.handle_input("o");
    view.handle_input("w");
    tokio::time::sleep(std::time::Duration::from_millis(650)).await;
    let lines = text(&view.render(80));
    assert_contains(&lines, "owner/first-GGUF");
    assert_contains(&lines, "1.2k downloads");
    assert_eq!(calls.lock().unwrap().as_slice(), ["ow".to_owned()]);

    view.handle_input("\x1b[B");
    view.handle_input("\r");
    assert_eq!(future.await, Some("owner/second-GGUF".to_owned()));

    let future = Box::pin(view.search_models(search_fn(Arc::clone(&calls))));
    view.handle_input("o");
    view.handle_input("w");
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_contains(&text(&view.render(80)), "owner/first-GGUF");
    assert_eq!(calls.lock().unwrap().len(), 1, "cached query must not search");
    view.handle_input("\x1b");
    assert_eq!(future.await, None);
}

#[tokio::test]
async fn progress_renders_bar_and_cancels() {
    let view = view();
    let future = Box::pin(view.progress(ProgressState {
        title: "Downloading model".to_owned(),
        model: "owner/repo:Q4_K_M".to_owned(),
        message: "Starting…".to_owned(),
        ratio: None,
        detail: None,
    }));
    view.update_progress(&ProgressState {
        title: "Downloading model".to_owned(),
        model: "owner/repo:Q4_K_M".to_owned(),
        message: "Downloading model".to_owned(),
        ratio: Some(0.5),
        detail: Some("512 B / 1.00 KiB".to_owned()),
    });
    let lines = text(&view.render(80));
    assert_contains(&lines, "50%");
    assert_contains(&lines, "512 B / 1.00 KiB");
    view.handle_input("\x1b");
    future.await;
}

#[test]
fn format_helpers_cover_context_and_descriptions() {
    let loaded: LlamaModelInfo = serde_json::from_value(json!({
        "id": "m",
        "status": {"value": "loaded", "args": ["--ctx-size", "4096"]},
        "meta": {"n_ctx": 0}
    }))
    .unwrap();
    let description = text(&{
        let view = view();
        let future = Box::pin(view.show_models("http://x".to_owned(), vec![loaded]));
        let lines = view.render(80);
        view.handle_input("\x1b");
        drop(future);
        lines
    });
    assert_contains(&description, "4k context");

    let unloaded = model("m", "unloaded");
    let lines = text(&{
        let view = view();
        let _future = Box::pin(view.show_models("http://x".to_owned(), vec![unloaded]));
        view.render(80)
    });
    assert_contains(&lines, "m");
    assert!(!lines.iter().any(|line| line.contains(" context")));
}
