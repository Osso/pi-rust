#[path = "../examples/extensions/interactive_shell.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;
use pi_coding_agent::core::{
    extensions::*,
    model_registry::ModelRegistry,
    model_runtime::{CreateModelRuntimeOptions, ModelRuntime},
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
type Trace = Arc<Mutex<Vec<Value>>>;
struct Backend(Trace);
impl ExtensionUiTuiBackend for Backend {
    fn create_fixed_cell(&self) -> std::rc::Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("no editor")
    }
    fn request_render(&self) {
        self.0.lock().unwrap().push(json!(["render", false]));
    }
    fn request_render_force(&self) {
        self.0.lock().unwrap().push(json!(["render", true]));
    }
    fn stop(&self) {
        self.0.lock().unwrap().push(json!(["stop"]));
    }
    fn start(&self) -> Result<(), String> {
        self.0.lock().unwrap().push(json!(["start"]));
        Ok(())
    }
    fn terminal_rows(&self) -> usize {
        24
    }
    fn terminal_columns(&self) -> usize {
        80
    }
    fn mode(&self) -> pi_tui::tui::TuiMode {
        pi_tui::tui::TuiMode::Regular
    }
}
struct Ui(Trace);
impl ExtensionUiContext for Ui {
    fn theme(&self) -> Arc<pi_coding_agent::modes::interactive::theme::Theme> {
        Arc::new(
            pi_coding_agent::modes::interactive::theme::load_theme_from_path(
                &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json"),
                Some(pi_coding_agent::modes::interactive::theme::ColorMode::Truecolor),
            )
            .unwrap(),
        )
    }
    fn custom<'a>(
        &'a self,
        factory: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<Value>, String>> {
        Box::pin(async move {
            self.0.lock().unwrap().push(json!(["custom"]));
            let value = Arc::new(Mutex::new(None));
            let result = value.clone();
            let trace = self.0.clone();
            let component = factory(
                ExtensionUiFactoryContext {
                    tui: ExtensionUiTui::new(Arc::new(Backend(self.0.clone()))),
                    theme: self.theme(),
                    keybindings: pi_coding_agent::core::keybindings::KeybindingsManager::new(Vec::new(), None),
                },
                ExtensionUiDone::new(Arc::new(move |v| {
                    *result.lock().unwrap() = v.clone();
                    trace.lock().unwrap().push(json!(["done", v]));
                })),
            );
            assert_eq!(component.render(80), Vec::<String>::new());
            component.invalidate();
            component.dispose();
            let result = value.lock().unwrap().clone();
            Ok(result)
        })
    }
}
async fn native(request: &Value) -> Value {
    let runtime = create_extension_runtime();
    let env = request["env"].clone();
    let extension = load_extension_from_factory(
        Arc::new(move |api| {
            let env = env.clone();
            let result = if env.is_null() {
                example::interactive_shell_extension(api)
            } else {
                example::interactive_shell_extension_with_environment(
                    api,
                    Arc::new(move |key| env[key].as_str().map(str::to_owned)),
                )
            };
            Box::pin(async move { result })
        }),
        "/fixture",
        support::event_bus(),
        runtime.clone(),
        "<example:interactive-shell>",
    )
    .await
    .unwrap();
    let models = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(pi_coding_agent::core::auth_storage::AuthStorage::in_memory(
            Default::default(),
        ))),
        models_path: Some(None),
        allow_model_network: false,
        ..Default::default()
    })
    .await
    .unwrap();
    let mut runner = support::runner_for(
        vec![extension],
        "/fixture",
        runtime,
        Arc::new(ModelRegistry::new(models)),
    );
    runner.bind_core(support::actions(), support::context_actions(), None);
    let trace: Trace = Arc::default();
    runner.set_ui_context(
        Some(Arc::new(Ui(trace.clone()))),
        serde_json::from_value(request["mode"].clone()).unwrap(),
    );
    let mut results = Vec::new();
    for command in request["commands"].as_array().unwrap() {
        trace.lock().unwrap().clear();
        let result = runner
            .emit_user_bash(&UserBashEvent {
                command: command.as_str().unwrap().into(),
                exclude_from_context: false,
                cwd: "/fixture".into(),
            })
            .await;
        results.push(json!({"result":result.and_then(|r|r.result),"trace":trace.lock().unwrap().clone()}));
    }
    json!(results)
}
#[tokio::test]
async fn detection_environment_prefix_exit_signal_and_spawn_failure_match() {
    let mut ts = oracle::TsOracle::spawn(
        "example-interactive-shell-oracle.ts",
        "packages/coding-agent/examples/extensions/interactive-shell.ts",
    );
    for request in [
        json!({"mode":"rpc","env":null,"commands":["i exit 0"]}),
        json!({"mode":"rpc","env":{},"commands":["vim"," NVIM file ","vimx","cat f | less","cat f | less\tx","cat f | less x","git add -p","git status","i exit 7","i\texit 7","I exit 7","\u{feff}htop\u{a0}","🚀"]}),
        json!({"mode":"rpc","env":{"INTERACTIVE_COMMANDS":"custom, ,Extra,🚀,İ","INTERACTIVE_EXCLUDE":"VIM,extra"},"commands":["vim","nvim","custom\tfile","cat | custom file","extra","🚀","İ"]}),
        json!({"mode":"tui","env":{"SHELL":"/bin/sh"},"commands":["echo not-interactive","i exit 0","i exit 7","i kill -TERM $$","i\t  exit 0  ","i "]}),
        json!({"mode":"tui","env":{"SHELL":"/nonexistent/pi-test-shell"},"commands":["i exit 0"]}),
        json!({"mode":"tui","env":{"SHELL":""},"commands":["i exit 0"]}),
    ] {
        assert_eq!(native(&request).await, ts.request(&request), "{request}");
    }
}
#[test]
fn inherited_stdio_worker() {
    let Ok(path) = std::env::var("PI_SHELL_RESULT") else {
        return;
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result=rt.block_on(native(&json!({"mode":"tui","env":{"SHELL":"/bin/sh"},"commands":["i read value; printf 'child-out:%s' \"$value\"; printf child-err >&2; exit 3"]})));
    std::fs::write(path, serde_json::to_vec(&result).unwrap()).unwrap();
}
#[test]
fn shell_inherits_input_output_error_and_emits_screen_clear() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("result.json");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "inherited_stdio_worker", "--nocapture"])
        .env("PI_SHELL_RESULT", &result)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"fixture-input\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "worker stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stdout.contains("\x1b[2J\x1b[Hchild-out:fixture-input"), "{stdout}");
    assert!(stderr.contains("child-err"), "{stderr}");
    let value: Value = serde_json::from_slice(&std::fs::read(result).unwrap()).unwrap();
    assert_eq!(value[0]["result"]["exitCode"], 3);
}
