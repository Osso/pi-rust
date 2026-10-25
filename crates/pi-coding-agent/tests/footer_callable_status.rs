#[path = "support/agent_session_fixture.rs"]
mod agent_session_fixture;
#[path = "support/footer_callable_source.rs"]
mod source;

use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::request::CallbackError;
use pi_coding_agent::core::extensions::types::ExtensionUiContext;
use pi_coding_agent::core::footer_data_provider::{ExtensionStatus, FooterDataProvider, FooterSessionOverride};
use pi_coding_agent::modes::interactive::components::footer::FooterComponent;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance};
use pi_js_compat::abort::AbortReason;
use pi_js_compat::js_json::{JsString, JsValue, parse_lossless, stringify_lossless, to_value};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use std::sync::{Arc, Mutex};

fn set_test_theme() {
    let colors = REQUIRED_COLORS
        .iter()
        .map(|name| (JsString::from(*name), ColorValue::Index(0.0)))
        .collect::<Vec<_>>();
    set_theme_instance(Arc::new(
        Theme::new(&colors, &[], ColorMode::Color256, None, None).unwrap(),
    ));
}

fn callback_error() -> CallbackError {
    NativeNodeError::from_constructor(
        NativeErrorClass::TypeError,
        JsString::from("status-") + JsString::from_utf16(vec![0xd800]),
    )
    .with_property("code", JsValue::String("ESTATUS".into()))
    .into()
}

fn error_value(error: &CallbackError) -> JsValue {
    match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => error.value(),
        AbortReason::Value(value) => value,
    }
}

#[tokio::test]
async fn callable_status_reads_and_real_footer_renders_match_frozen() {
    set_test_theme();
    let directory = tempfile::tempdir().unwrap();
    let fixture = agent_session_fixture::fixture(directory.path(), ThinkingLevel::High).await;
    let output = source::run(
        "sequence",
        directory.path(),
        to_value(&fixture.session.model()).unwrap(),
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected = parse_lossless(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
    let provider = Arc::new(FooterDataProvider::new(directory.path().to_str().unwrap()));
    let trace = Arc::new(Mutex::new(Vec::<JsValue>::new()));
    let status = Arc::new(Mutex::new(
        JsString::from("first-") + JsString::from_utf16(vec![0xd800]),
    ));
    let omitted = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let trace_callback = Arc::clone(&trace);
    let status_callback = Arc::clone(&status);
    let omitted_callback = Arc::clone(&omitted);
    provider.set_extension_status(&"dynamic".into(), Some(ExtensionStatus::Callback(Arc::new(move |view| {
        trace_callback.lock().unwrap().push(view.map_or(JsValue::Null, |view: FooterSessionOverride| pi_js_compat::js_json!({"cwd":view.cwd,"thinkingLevel":view.thinking_level})));
        Ok((!omitted_callback.load(std::sync::atomic::Ordering::SeqCst)).then(||status_callback.lock().unwrap().clone()))
    }))));
    provider.set_extension_status(&"static".into(), Some(ExtensionStatus::Text("static line".into())));
    let mut footer = FooterComponent::new(Arc::new(fixture.session.clone()), Arc::clone(&provider));
    let snapshot_trace = || to_value(&*trace.lock().unwrap()).unwrap();
    let read = || {
        let statuses = provider
            .get_extension_statuses()
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>();
        pi_js_compat::js_json!({"statuses":statuses,"trace":snapshot_trace()})
    };
    let render = |footer: &mut FooterComponent| {
        let lines = footer
            .render_checked(200)
            .unwrap()
            .into_iter()
            .skip(2)
            .collect::<Vec<_>>();
        pi_js_compat::js_json!({"lines":lines,"trace":snapshot_trace()})
    };
    let mut actual = vec![pi_js_compat::js_json!({"trace":snapshot_trace()})];
    actual.push(render(&mut footer));
    *status.lock().unwrap() = JsString::from("second-") + JsString::from_utf16(vec![0xdc00]);
    actual.push(read());
    provider.set_session_override(FooterSessionOverride {
        cwd: JsString::from("view-") + JsString::from_utf16(vec![0xd800]),
        session_manager: None,
        model: None,
        thinking_level: "low".into(),
        context_usage: None,
    });
    actual.push(read());
    actual.push(render(&mut footer));
    omitted.store(true, std::sync::atomic::Ordering::SeqCst);
    actual.push(read());
    omitted.store(false, std::sync::atomic::Ordering::SeqCst);
    provider.clear_session_override();
    actual.push(read());
    provider.set_extension_status(&"dynamic".into(), Some(ExtensionStatus::Text("replaced".into())));
    actual.push(read());
    provider.clear_extension_statuses();
    actual.push(read());
    provider.dispose();
    assert_eq!(to_value(&actual).unwrap(), expected);
}

#[tokio::test]
async fn callable_status_getter_and_checked_render_preserve_original_throw() {
    set_test_theme();
    let directory = tempfile::tempdir().unwrap();
    let fixture = agent_session_fixture::fixture(directory.path(), ThinkingLevel::High).await;
    for operation in ["read", "checked", "pyrun"] {
        let output = source::run(operation, directory.path(), to_value(&fixture.session.model()).unwrap());
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let expected = parse_lossless(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
        assert_eq!(expected["sameError"], JsValue::Bool(true));
        let provider = Arc::new(FooterDataProvider::new(directory.path().to_str().unwrap()));
        provider.set_extension_status(
            &"throw".into(),
            Some(ExtensionStatus::Callback(Arc::new(|_| Err(callback_error())))),
        );
        let error = match operation {
            "read" => provider.get_extension_statuses().unwrap_err(),
            "checked" => FooterComponent::new(Arc::new(fixture.session.clone()), Arc::clone(&provider))
                .render_checked(200)
                .unwrap_err(),
            "pyrun" => {
                use pi_coding_agent::bundled_extensions::pyrun::eval_tool::{
                    PyrunEvalContext, PyrunEvalParams, create_canonical_pyrun_eval_params,
                };
                let context = PyrunEvalContext {
                    footer_data: Some(Arc::clone(&provider)),
                    context_usage: None,
                    model: Some(fixture.session.model()),
                    cwd: directory.path().to_str().unwrap().into(),
                    session_name: None,
                };
                create_canonical_pyrun_eval_params(
                    &PyrunEvalParams {
                        code: "pass".into(),
                        session_id: None,
                    },
                    &context,
                    true,
                )
                .unwrap_err()
            }
            _ => unreachable!(),
        };
        assert_eq!(
            stringify_lossless(&error_value(&error)),
            stringify_lossless(&expected["error"])
        );
        provider.dispose();
    }
}

#[test]
fn no_op_ui_status_does_not_evaluate_a_callable() {
    struct NoOp;
    impl ExtensionUiContext for NoOp {}
    let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = Arc::clone(&called);
    NoOp.set_status(
        &"ignored".into(),
        Some(ExtensionStatus::Callback(Arc::new(move |_| {
            observed.store(true, std::sync::atomic::Ordering::SeqCst);
            Err(callback_error())
        }))),
    )
    .unwrap();
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
}

struct SilentTerminal;
impl pi_tui::terminal::Terminal for SilentTerminal {
    fn start(&mut self) -> Result<(), CallbackError> {
        Ok(())
    }
    fn stop(&mut self) -> Result<(), CallbackError> {
        Ok(())
    }
    fn drain_input(&mut self, _max_ms: u64, _idle_ms: u64) {}
    fn write(&mut self, _data: &JsString) {}
    fn columns(&self) -> usize {
        200
    }
    fn rows(&self) -> usize {
        24
    }
    fn kitty_protocol_active(&self) -> bool {
        false
    }
    fn move_by(&mut self, _lines: isize) {}
    fn hide_cursor(&mut self) {}
    fn show_cursor(&mut self) {}
    fn clear_line(&mut self) {}
    fn clear_from_cursor(&mut self) {}
    fn clear_screen(&mut self) {}
    fn set_title(&mut self, _title: &JsString) {}
    fn set_progress(&mut self, _active: bool) {}
}

#[tokio::test(flavor = "current_thread")]
async fn scheduled_footer_throw_reaches_shared_uncaught_authority() {
    use pi_tui::tui::TuiExt;
    use std::io::Write;
    if let Some(root) = std::env::var_os("PI_CALLABLE_FOOTER_CHILD") {
        pi_coding_agent::utils::uncaught::install_uncaught_handler();
        let trace = Arc::new(Mutex::new(Vec::<JsValue>::new()));
        let observed = Arc::clone(&trace);
        pi_coding_agent::utils::uncaught::set_process_crash_handler(Some(Arc::new(move |payload| {
            let error = payload
                .downcast_ref::<CallbackError>()
                .map_or(JsValue::Null, error_value);
            let report =
                pi_js_compat::js_json!({"uncaught":true,"error":error,"trace":observed.lock().unwrap().clone()});
            writeln!(std::io::stdout(), "{}", stringify_lossless(&report)).unwrap();
            std::process::exit(17);
        })));
        tokio::task::LocalSet::new()
            .run_until(async {
                set_test_theme();
                let fixture = agent_session_fixture::fixture(std::path::Path::new(&root), ThinkingLevel::High).await;
                let provider = Arc::new(FooterDataProvider::new(std::path::Path::new(&root).to_str().unwrap()));
                let observed = Arc::clone(&trace);
                provider.set_extension_status(
                    &"throw".into(),
                    Some(ExtensionStatus::Callback(Arc::new(move |_| {
                        observed.lock().unwrap().push(JsValue::Null);
                        Err(callback_error())
                    }))),
                );
                let footer = FooterComponent::new(Arc::new(fixture.session), provider);
                let terminal = std::rc::Rc::new(std::cell::RefCell::new(SilentTerminal));
                let ui = pi_tui::tui::TuiRenderer::new(terminal);
                ui.add_child(pi_tui::tui::component(footer));
                ui.start().unwrap();
                // The native owner loop drives the deterministic nextTick/timer queue.
                ui.flush_pending().unwrap();
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                std::process::exit(99);
            })
            .await;
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let source = source::run(
        "uncaught",
        directory.path(),
        pi_js_compat::js_json!({"id":"faux-1","provider":"faux","reasoning":false,"contextWindow":100000}),
    );
    assert_eq!(
        source.status.code(),
        Some(17),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected = parse_lossless(String::from_utf8(source.stdout).unwrap().trim()).unwrap();
    assert_eq!(expected["sameError"], JsValue::Bool(true));
    let native = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "scheduled_footer_throw_reaches_shared_uncaught_authority",
            "--exact",
            "--nocapture",
        ])
        .env("PI_CALLABLE_FOOTER_CHILD", directory.path())
        .output()
        .unwrap();
    assert_eq!(
        native.status.code(),
        Some(17),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let stdout = String::from_utf8(native.stdout).unwrap();
    let line = stdout.lines().find(|line| line.starts_with("{\"uncaught\":")).unwrap();
    let actual = parse_lossless(line).unwrap();
    assert_eq!(actual["uncaught"], JsValue::Bool(true));
    assert_eq!(actual["trace"], expected["trace"]);
    assert_eq!(
        stringify_lossless(&actual["error"]),
        stringify_lossless(&expected["error"])
    );
}

#[tokio::test]
async fn pyrun_disabled_bridge_does_not_read_callable_footer_statuses() {
    use pi_coding_agent::bundled_extensions::pyrun::eval_tool::{
        PyrunEvalContext, PyrunEvalParams, create_canonical_pyrun_eval_params,
    };
    let directory = tempfile::tempdir().unwrap();
    let source = source::run("pyrun-disabled", directory.path(), JsValue::Null);
    assert!(source.status.success(), "{}", String::from_utf8_lossy(&source.stderr));
    let expected = parse_lossless(String::from_utf8(source.stdout).unwrap().trim()).unwrap();
    assert_eq!(expected["trace"], pi_js_compat::js_json!([]));
    let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = Arc::clone(&called);
    let provider = Arc::new(FooterDataProvider::new(directory.path().to_str().unwrap()));
    provider.set_extension_status(
        &"throw".into(),
        Some(ExtensionStatus::Callback(Arc::new(move |_| {
            observed.store(true, std::sync::atomic::Ordering::SeqCst);
            Err(callback_error())
        }))),
    );
    let context = PyrunEvalContext {
        footer_data: Some(Arc::clone(&provider)),
        context_usage: None,
        model: None,
        cwd: directory.path().to_str().unwrap().into(),
        session_name: None,
    };
    let actual = create_canonical_pyrun_eval_params(
        &PyrunEvalParams {
            code: "pass".into(),
            session_id: None,
        },
        &context,
        false,
    )
    .unwrap();
    assert_eq!(
        stringify_lossless(&to_value(&actual).unwrap()),
        stringify_lossless(&expected["params"])
    );
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
    provider.dispose();
}

// Shared support is compiled once per integration target.
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
