//! Exercise the actual custom-UI factory/worker/done boundary without a terminal or provider.
use super::*;
use crate::core::extensions::*;
use pi_ai::request::CallbackError;
use std::sync::Mutex;

struct Request {
    factory: ExtensionUiCustomFactory,
    options: Option<ExtensionUiCustomOptions>,
    result: tokio::sync::oneshot::Sender<Option<JsValue>>,
}
struct Ui(tokio::sync::mpsc::UnboundedSender<Request>);
impl ExtensionUiContext for Ui {
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        options: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, CallbackError>> {
        Box::pin(async move {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            self.0
                .send(Request {
                    factory,
                    options,
                    result: sender,
                })
                .map_err(|error| CallbackError::new(error.to_string()))?;
            receiver.await.map_err(|error| CallbackError::new(error.to_string()))
        })
    }
}
struct Terminal(std::sync::atomic::AtomicUsize);
impl ExtensionUiTuiBackend for Terminal {
    fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("atlas does not create editor cells")
    }
    fn request_render(&self) {}
    fn request_component_render(&self, _: usize) {}
    fn request_render_force(&self) {}
    fn stop(&self) -> Result<(), CallbackError> {
        Ok(())
    }
    fn start(&self) -> Result<(), CallbackError> {
        Ok(())
    }
    fn terminal_rows(&self) -> usize {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn terminal_columns(&self) -> usize {
        160
    }
    fn show_overlay(
        &self,
        _: Box<dyn ExtensionUiComponent>,
        _: Option<ExtensionUiOverlayOptions>,
    ) -> Rc<dyn ExtensionUiOverlayHandle> {
        panic!("atlas has no nested overlays")
    }
}

fn theme() -> Arc<Theme> {
    use crate::modes::interactive::theme::{ColorMode, ColorValue};
    let colors: Vec<_> = [
        "accent",
        "muted",
        "dim",
        "text",
        "borderMuted",
        "border",
        "borderAccent",
        "success",
        "error",
        "warning",
    ]
    .map(|name| (JsString::from(name), ColorValue::Text("#abcdef".into())))
    .into();
    Arc::new(
        Theme::new(
            &colors,
            &[("selectedBg".into(), ColorValue::Text("#102030".into()))],
            ColorMode::Truecolor,
            None,
            None,
        )
        .unwrap(),
    )
}

fn context(ui: Option<Arc<dyn ExtensionUiContext>>) -> ExtensionCommandContext {
    use crate::core::{auth_storage::AuthStorage, model_registry::ModelRegistry, session_manager::SessionManager};
    let mut runner = ExtensionRunner::new(
        Vec::new(),
        create_extension_runtime(),
        "/tmp",
        Arc::new(Mutex::new(
            SessionManager::in_memory(Some(&"/tmp".into()), None).unwrap(),
        )),
        Arc::new(ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap()),
        None,
    );
    runner.set_ui_context(ui, ExtensionMode::Tui);
    runner.create_command_context()
}

async fn mount(request: Request) -> Box<dyn ExtensionUiComponent> {
    assert_eq!(request.options.as_ref().unwrap().overlay, Some(true));
    let options = request.options.unwrap().overlay_options.unwrap()();
    assert_eq!(options.min_width, Some(100));
    assert_eq!(options.width, Some(pi_tui::tui::SizeValue::Percent(50.0)));
    assert_eq!(options.max_height, Some(pi_tui::tui::SizeValue::Percent(80.0)));
    let result = Rc::new(RefCell::new(Some(request.result)));
    let done = ExtensionUiDone::new(Rc::new(move |value| {
        if let Some(sender) = result.borrow_mut().take() {
            sender.send(value).expect("command waits for done");
        }
    }));
    (request.factory)(
        ExtensionUiFactoryContext {
            tui: ExtensionUiTui::new(Arc::new(Terminal(std::sync::atomic::AtomicUsize::new(50)))),
            theme: theme(),
            keybindings: crate::core::keybindings::KeybindingsManager::new(Default::default(), None),
        },
        done,
    )
    .unwrap()
    .await
    .unwrap()
}

fn contains(lines: &[JsString], text: &str) -> bool {
    lines.iter().any(|line| line.contains(text))
}

#[tokio::test]
async fn loads_disk_logs_then_opens_six_tab_dashboard_and_closes() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let dir = tempfile::tempdir().unwrap();
            let sessions = dir.path().join("sessions");
            std::fs::create_dir_all(&sessions).unwrap();
            let domain = js_json::parse_lossless(include_str!("fixtures/domain.json")).unwrap();
            std::fs::write(
                sessions.join("session.jsonl"),
                js_json::encode_node_utf8(domain["logs"][0]["input"].as_string().unwrap()),
            )
            .unwrap();
            let cache = dir.path().join("cache.json");
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            let ctx = context(Some(Arc::new(Ui(sender))));
            let task = tokio::spawn(run_atlas(
                ctx,
                Paths {
                    cache: cache.clone(),
                    sessions,
                },
            ));
            let loading = mount(receiver.recv().await.unwrap()).await;
            assert!(contains(&loading.render(100), "Parsing session logs..."));
            let request = tokio::time::timeout(Duration::from_secs(10), receiver.recv())
                .await
                .unwrap()
                .unwrap();
            let dashboard = mount(request).await;
            assert!(cache.exists());
            assert!(contains(&dashboard.render(100), "Total cost"));
            dashboard.handle_input(&"\x1b[C".into());
            assert!(contains(&dashboard.render(100), "by lines written"));
            dashboard.handle_input(&"r".into());
            assert!(contains(&dashboard.render(100), "Today"));
            dashboard.handle_input(&"q".into());
            task.await.unwrap().unwrap();
            dashboard.dispose();
            loading.dispose();
        })
        .await;
}

#[tokio::test]
async fn cancellation_closes_loading_but_worker_still_writes_cache() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let dir = tempfile::tempdir().unwrap();
            let cache = dir.path().join("cache.json");
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            let ctx = context(Some(Arc::new(Ui(sender))));
            let task = tokio::spawn(run_atlas(
                ctx,
                Paths {
                    cache: cache.clone(),
                    sessions: dir.path().join("missing"),
                },
            ));
            let loading = mount(receiver.recv().await.unwrap()).await;
            loading.handle_input(&"q".into());
            task.await.unwrap().unwrap();
            tokio::time::timeout(Duration::from_secs(10), async {
                while !cache.exists() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            assert!(receiver.try_recv().is_err());
        })
        .await;
}

#[tokio::test]
async fn headless_atlas_does_not_parse_or_write_files() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("cache.json");
    run_atlas(
        context(None),
        Paths {
            cache: cache.clone(),
            sessions: dir.path().join("missing"),
        },
    )
    .await
    .unwrap();
    assert!(!cache.exists());
}

#[tokio::test]
async fn scroll_ranges_and_resize_match_source_frames() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let fixture = js_json::parse_lossless(include_str!("fixtures/scenarios.json")).unwrap();
            let summaries: [StatsSummary; 4] = js_json::from_value::<Vec<StatsSummary>>(&fixture["summaries"])
                .unwrap()
                .try_into()
                .unwrap();
            let backend = Arc::new(Terminal(std::sync::atomic::AtomicUsize::new(15)));
            let dashboard = Dashboard {
                summaries,
                update: None,
                theme: theme(),
                tui: ExtensionUiTui::new(backend.clone()),
                done: ExtensionUiDone::new(Rc::new(|_| {})),
                state: RefCell::new(DashboardState::new(15)),
                cache: RefCell::new(None),
                marquee_start: Cell::new(None),
                timer: RefCell::new(None),
            };
            for (frame_index, frame) in fixture["frames"].as_array().unwrap().iter().enumerate() {
                backend.0.store(
                    frame["rows"].as_u64().unwrap() as usize,
                    std::sync::atomic::Ordering::SeqCst,
                );
                if let Some(key) = frame["key"].as_string() {
                    dashboard.handle_input(key);
                }
                let actual = dashboard.render(frame["width"].as_u64().unwrap() as usize);
                let expected: Vec<JsString> = js_json::from_value(&frame["lines"]).unwrap();
                assert_eq!(actual.len(), expected.len(), "frame {frame_index} line count");
                for (row, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                    assert_eq!(
                        js_json::stringify_lossless(&JsValue::String(actual.clone())),
                        js_json::stringify_lossless(&JsValue::String(expected)),
                        "frame {frame_index} row {row}"
                    );
                }
            }
            dashboard.dispose();
        })
        .await;
}
