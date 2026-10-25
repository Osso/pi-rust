//! Composition proof: opt-in example loaded through the real native extension API.
#[path = "../examples/extensions/custom_header.rs"]
mod custom_header;
use crate::shared_extensions as extensions;
use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::extensions::loader::{ExtensionFactory, ExtensionRegistry, load_extension_from_factory};
use pi_coding_agent::core::extensions::types::{
    ExtensionMode, ExtensionUiContext, ExtensionUiFactoryContext, ExtensionUiHeaderFactory, ExtensionUiTui,
    ExtensionUiTuiBackend, NotifyType, SessionStartEvent, SessionStartReason,
};
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, Theme};
use serde_json::{Value, json};

const WIDTHS: [usize; 5] = [0, 1, 16, 80, 200];

struct Terminal;
impl ExtensionUiTuiBackend for Terminal {
    fn request_component_render(&self, _identity: usize) {}
    fn show_overlay(
        &self,
        _component: Box<dyn pi_coding_agent::core::extensions::ExtensionUiComponent>,
        _options: Option<pi_coding_agent::core::extensions::ExtensionUiOverlayOptions>,
    ) -> Rc<dyn pi_coding_agent::core::extensions::ExtensionUiOverlayHandle> {
        panic!("this fixture does not mount nested overlays")
    }

    fn request_render_force(&self) {
        panic!("fixture does not force redraw")
    }
    fn stop(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not release terminal")
    }
    fn start(&self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        panic!("fixture does not acquire terminal")
    }

    fn create_fixed_cell(&self) -> std::rc::Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("this component fixture must not construct an editor")
    }
    fn request_render(&self) {
        panic!("static header must not request rendering");
    }
    fn terminal_rows(&self) -> usize {
        40
    }
    fn terminal_columns(&self) -> usize {
        80
    }
}

struct CaptureUi {
    theme: Arc<Theme>,
    calls: Mutex<Vec<Value>>,
}
impl ExtensionUiContext for CaptureUi {
    fn set_header(&self, factory: Option<ExtensionUiHeaderFactory>) -> Result<(), pi_ai::request::CallbackError> {
        let mut calls = self.calls.lock().unwrap();
        calls.push(json!([if factory.is_some() { "setHeader" } else { "clearHeader" }]));
        let Some(factory) = factory else {
            return Ok(());
        };
        let component = factory(ExtensionUiFactoryContext {
            tui: ExtensionUiTui::new(Arc::new(Terminal)),
            theme: Arc::clone(&self.theme),
            keybindings: KeybindingsManager::new(Vec::new(), None),
        })?
        .component;
        calls.push(json!(["factory"]));
        for width in WIDTHS {
            calls.push(json!(["render", width, component.render(width)]));
        }
        component.invalidate();
        calls.push(json!(["invalidate"]));
        calls.push(json!(["render", 1, component.render(1)]));
        Ok(())
    }
    fn notify(
        &self,
        message: &JsString,
        level: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        assert_eq!(level, Some(NotifyType::Info));
        self.calls.lock().unwrap().push(json!(["notify", message, "info"]));
        Ok(())
    }
}

fn theme(colors: &Value) -> Arc<Theme> {
    let colors = colors
        .as_object()
        .unwrap()
        .iter()
        .map(|(role, rgb)| {
            let rgb = rgb.as_array().unwrap();
            (
                JsString::from(role.clone()),
                ColorValue::Text(JsString::from(format!(
                    "#{:02x}{:02x}{:02x}",
                    rgb[0].as_u64().unwrap(),
                    rgb[1].as_u64().unwrap(),
                    rgb[2].as_u64().unwrap()
                ))),
            )
        })
        .collect::<Vec<_>>();
    Arc::new(Theme::new(&colors, &[], ColorMode::Truecolor, None, None).unwrap())
}

#[tokio::test]
async fn pinned_custom_header_composition_matches_all_modes_and_colors() {
    // This binary owns its process-local package override and initializes VERSION first.
    let package = tempfile::tempdir().unwrap();
    std::fs::write(
        package.path().join("package.json"),
        r#"{"version":"8.7.6-header-fixture"}"#,
    )
    .unwrap();
    pi_js_compat::process_env::set("PI_PACKAGE_DIR", package.path().to_str().unwrap());
    let version = pi_coding_agent::config::version();
    assert_eq!(version, "8.7.6-header-fixture");
    let mut oracle = oracle::TsOracle::spawn_with_environment(
        "example-custom-header-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-header.ts",
        &[(
            "CUSTOM_HEADER_ORACLE_VERSION",
            pi_js_compat::js_json::encode_node_utf8(version).into(),
        )],
    );
    let model_registry = extensions::model_registry().await;
    for colors in [
        json!({"accent": [80, 180, 230], "dim": [90, 91, 92], "muted": [150, 151, 152]}),
        json!({"accent": [230, 100, 12], "dim": [30, 40, 50], "muted": [70, 80, 90]}),
    ] {
        for (mode_name, mode) in [
            ("tui", ExtensionMode::Tui),
            ("text", ExtensionMode::Print),
            ("rpc", ExtensionMode::Rpc),
            ("json", ExtensionMode::Json),
        ] {
            let runtime = extensions::runtime();
            let factory: ExtensionFactory = Arc::new(|api| {
                let result = custom_header::custom_header_extension(api);
                Box::pin(async move { result })
            });
            let mut registry = ExtensionRegistry::new();
            registry.register("<example:custom-header>", factory);
            let extension = load_extension_from_factory(
                registry
                    .get(&JsString::from("<example:custom-header>"))
                    .unwrap()
                    .clone(),
                &JsString::from("/tmp"),
                extensions::event_bus(),
                runtime.clone(),
                &JsString::from("<example:custom-header>"),
            )
            .await
            .unwrap();
            // RED boundary: an inert registration compiles but cannot install the header.
            assert_eq!(
                extension.read().handlers.session_start.len(),
                1,
                "example must register session_start"
            );
            assert_eq!(extension.read().commands.len(), 1);
            let command = extension
                .read()
                .commands
                .get(&pi_js_compat::js_json::JsString::from("builtin-header"))
                .unwrap()
                .clone();
            assert!(command.get_argument_completions.is_none());
            let registrations = json!([["on", "session_start"], ["command", command.name, command.description],]);
            let ui = Arc::new(CaptureUi {
                theme: theme(&colors),
                calls: Mutex::new(Vec::new()),
            });
            let mut runner = extensions::runner_for(vec![extension.clone()], "/tmp", runtime, model_registry.clone());
            extensions::bind(&mut runner);
            runner.set_ui_context(Some(ui.clone()), mode);
            assert!(
                ui.calls.lock().unwrap().is_empty(),
                "loading alone must not install a header"
            );
            let handler = extension.read().handlers.session_start[0].clone();
            handler
                .call(
                    SessionStartEvent {
                        reason: SessionStartReason::Startup,
                        previous_session_file: None,
                    },
                    runner.create_context(),
                )
                .await
                .unwrap();
            (command.handler)(JsString::from("ignored args"), runner.create_command_context())
                .await
                .unwrap();
            let actual = json!({"registrations": registrations, "calls": ui.calls.lock().unwrap().clone()});
            let expected =
                oracle.request(&json!({"id": mode_name, "mode": mode_name, "colors": colors, "widths": WIDTHS}));
            assert_eq!(expected["ok"], true, "{expected}");
            assert_eq!(actual, expected["output"], "mode {mode_name}, colors {colors}");
            if mode == ExtensionMode::Tui {
                let calls = ui.calls.lock().unwrap();
                let lines = calls[2][2].as_array().unwrap();
                assert_eq!(lines.len(), 9, "eight mascot lines plus subtitle");
                assert_eq!(lines[0], "");
                assert_eq!(lines[7], "");
            }
        }
    }
    pi_js_compat::process_env::clear();
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
