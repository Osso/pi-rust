use super::agent;
use pi_coding_agent::bundled_extensions::agents_status::create_agents_status_component;
use pi_coding_agent::core::extensions::types::*;
use pi_coding_agent::core::multi_agent_store::{AgentLifecycle, MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::modes::interactive::theme::get_theme_by_name;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::JsString;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Backend {
    full: Mutex<usize>,
    targets: Mutex<Vec<usize>>,
}
impl ExtensionUiTuiBackend for Backend {
    fn request_component_render(&self, identity: usize) {
        self.targets.lock().unwrap().push(identity);
    }
    fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("no editor")
    }
    fn request_render(&self) {
        *self.full.lock().unwrap() += 1;
    }
    fn request_render_force(&self) {
        panic!("no forced render")
    }
    fn stop(&self) -> Result<(), CallbackError> {
        panic!("no stop")
    }
    fn start(&self) -> Result<(), CallbackError> {
        panic!("no start")
    }
    fn terminal_rows(&self) -> usize {
        24
    }
    fn terminal_columns(&self) -> usize {
        80
    }
    fn show_overlay(
        &self,
        _component: Box<dyn ExtensionUiComponent>,
        _options: Option<ExtensionUiOverlayOptions>,
    ) -> Rc<dyn ExtensionUiOverlayHandle> {
        panic!("no overlay")
    }
}

use crate::shared_tool_context as tool_context;

#[tokio::test]
async fn status_extension_registers_widget_on_session_start_only_with_supplied_store() {
    use pi_coding_agent::bundled_extensions::agents_status::{AgentsStatusExtensionOptions, agents_status_extension};
    use pi_coding_agent::core::extensions::{create_extension_runtime, load_extension_from_factory};
    #[derive(Default)]
    struct Ui(Mutex<Vec<(JsString, Option<ExtensionUiComponentFactory>)>>);
    impl ExtensionUiContext for Ui {
        fn set_widget_component(
            &self,
            key: &JsString,
            factory: Option<ExtensionUiComponentFactory>,
            _options: Option<&ExtensionWidgetOptions>,
        ) {
            self.0.lock().unwrap().push((key.clone(), factory));
        }
    }
    for with_store in [false, true] {
        let options = AgentsStatusExtensionOptions {
            store: with_store.then(|| Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()))),
        };
        let extension = load_extension_from_factory(
            Arc::new(move |pi| {
                let result = agents_status_extension(pi, &options);
                Box::pin(async move { result })
            }),
            &"/tmp".into(),
            Arc::new(pi_coding_agent::core::event_bus::EventBusController::new()),
            create_extension_runtime(),
            &"<agents-status>".into(),
        )
        .await
        .unwrap();
        let hooks = extension.read().handlers.session_start.clone();
        assert_eq!(hooks.len(), usize::from(with_store));
        let ui = Arc::new(Ui::default());
        let (_runner, ctx) =
            tool_context::TestContextBuilder::new("/tmp", tool_context::in_memory_session("/tmp", "fixture"))
                .mode(ExtensionMode::Rpc)
                .ui(ui.clone())
                .build()
                .await;
        for hook in hooks {
            hook.call(
                SessionStartEvent {
                    reason: SessionStartReason::Startup,
                    previous_session_file: None,
                },
                ctx.clone(),
            )
            .await
            .unwrap();
        }
        let widgets = ui.0.lock().unwrap();
        assert_eq!(widgets.len(), usize::from(with_store));
        if with_store {
            assert_eq!(widgets[0].0, "agents-status");
            assert!(widgets[0].1.is_some());
        }
    }
}

#[tokio::test(start_paused = true)]
async fn status_component_refreshes_only_while_visible_and_releases_timer_and_subscription() {
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    let backend = Arc::new(Backend::default());
    let component = create_agents_status_component(
        Arc::clone(&store),
        ExtensionUiTui::new(backend.clone()),
        get_theme_by_name(&"dark".into()).unwrap(),
    );
    assert!(component.render(80).is_empty());
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert!(backend.targets.lock().unwrap().is_empty());
    let mut current = agent("visible", None, AgentLifecycle::Running, "2026-10-25T00:00:00.000Z");
    store.publish_lifecycle_coordinator_snapshot(&current).unwrap();
    current.revision += 1;
    store.publish_lifecycle_coordinator_snapshot(&current).unwrap();
    assert_eq!(backend.targets.lock().unwrap().len(), 1);
    assert!(JsString::join(component.render(80), "\n").contains("visible"));
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(backend.targets.lock().unwrap().len(), 2);
    current.lifecycle = AgentLifecycle::Completed;
    current.updated_at = "2000-01-01T00:00:00.000Z".into();
    store.publish_lifecycle_coordinator_snapshot(&current).unwrap();
    assert_eq!(
        backend.targets.lock().unwrap().len(),
        3,
        "last visible rows must be cleared"
    );
    assert!(component.render(80).is_empty());
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert_eq!(backend.targets.lock().unwrap().len(), 3);
    component.dispose();
    component.dispose();
    current.lifecycle = AgentLifecycle::Running;
    store.publish_lifecycle_coordinator_snapshot(&current).unwrap();
    tokio::time::advance(std::time::Duration::from_secs(2)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        backend.targets.lock().unwrap().len(),
        3,
        "disposed component must stop all refreshes"
    );
    assert_eq!(*backend.full.lock().unwrap(), 0, "requests must target this component");
    assert!(backend.targets.lock().unwrap().windows(2).all(|ids| ids[0] == ids[1]));
}
