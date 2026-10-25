//! Behavioral tests for the bundled `default-footer` extension.

#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/extensions.rs"]
mod support;

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use bundled::{Harness, HarnessOptions, test_model};
use pi_agent_core::harness::ThinkingLevel;
use pi_coding_agent::bundled_extensions::default_footer::{
    DefaultFooterAgentLifecycleCounts, DefaultFooterComponentInput, count_default_footer_agents,
    create_default_footer_component, default_footer_extension,
};
use pi_coding_agent::core::extensions::create_extension_runtime;
use pi_coding_agent::core::extensions::types::{
    ContextUsage, ExtensionEvent, ExtensionUiComponent, ExtensionUiFooterContext, ExtensionUiTui,
    ExtensionUiTuiBackend, SessionStartEvent, SessionStartReason,
};
use pi_coding_agent::core::footer_data_provider::{FooterDataProvider, FooterSessionOverride, GitBranchRunner};
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, AgentPermission, AgentSlot, AgentSnapshot, MultiAgentStore, MultiAgentStoreOptions,
};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::modes::interactive::theme::{ThemeColor, current_theme_if_initialized};
use pi_coding_agent::utils::ansi::strip_ansi;
use pi_tui::utils::visible_width;

struct StubGit;

impl GitBranchRunner for StubGit {
    fn resolve_sync(&self, _repo_dir: &JsString) -> Option<JsString> {
        Some(JsString::from("main"))
    }

    fn resolve_async(&self, _repo_dir: &JsString) -> pi_ai::request::BoxFuture<'static, Option<JsString>> {
        Box::pin(async { Some(JsString::from("main")) })
    }
}

fn git_fixture_dir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "pi-default-footer-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(dir.join(".git")).expect("git dir");
    std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    dir
}

struct StubTui {
    renders: Arc<Mutex<usize>>,
}

impl ExtensionUiTuiBackend for StubTui {
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
        *self.renders.lock().unwrap() += 1;
    }

    fn terminal_rows(&self) -> usize {
        40
    }

    fn terminal_columns(&self) -> usize {
        160
    }
}

fn assistant_message(input: f64, output: f64, cost: f64) -> pi_agent_core::AgentMessage {
    let mut message = pi_ai::types::AssistantMessage::empty("openai-responses", "openai", "test-model", 1.0);
    message.usage.input = input;
    message.usage.output = output;
    message.usage.cost.total = cost;
    pi_agent_core::AgentMessage::Assistant(Box::new(message))
}

fn session_with_usage(input: f64, output: f64, cost: f64) -> SessionManager {
    let mut session = SessionManager::in_memory(Some(&JsString::from("/tmp/project")), None).expect("session");
    session
        .append_message(assistant_message(input, output, cost))
        .expect("append assistant");
    session
}

struct Fixture {
    harness: Harness,
    footer_data: Arc<FooterDataProvider>,
}

async fn fixture(
    session: SessionManager,
    context_usage: Option<ContextUsage>,
    model: Option<pi_ai::types::Model>,
) -> Fixture {
    let runtime = create_extension_runtime();
    let mut options = HarnessOptions::new("/tmp/project");
    options.session_manager = Arc::new(Mutex::new(session));
    options.context_usage = context_usage;
    options.model = model;
    options.thinking_level = ThinkingLevel::High;
    let harness = Harness::new(Vec::new(), runtime, options).await;
    let repo = git_fixture_dir();
    let footer_data = Arc::new(FooterDataProvider::with_git_runner(
        JsString::from(repo.to_str().expect("fixture path must be UTF-8")),
        Arc::new(StubGit),
    ));
    Fixture { harness, footer_data }
}

fn component(
    fixture: &Fixture,
    counts: Option<DefaultFooterAgentLifecycleCounts>,
) -> pi_coding_agent::bundled_extensions::default_footer::DefaultFooterComponent {
    create_default_footer_component(DefaultFooterComponentInput {
        ctx: fixture.harness.context(),
        footer_data: Arc::clone(&fixture.footer_data),
        get_agent_counts: Some(Arc::new(move || counts)),
        get_selected_agent: None,
        theme: Arc::clone(&current_theme_if_initialized().expect("theme")),
    })
}

#[tokio::test]
async fn renders_stats_without_cache_or_auto_fields() {
    let fixture = fixture(
        session_with_usage(412_000.0, 33_000.0, 9.389),
        Some(ContextUsage {
            tokens: Some(1.0),
            context_window: 272_000.0,
            percent: Some(60.9),
        }),
        Some(test_model(false)),
    )
    .await;
    let footer = component(
        &fixture,
        Some(DefaultFooterAgentLifecycleCounts {
            running: 2,
            waiting_for_input: 3,
            steering_pending: 1,
        }),
    );
    let lines = footer.render(160);
    let stats = strip_ansi(&lines[1]);
    assert!(
        stats.contains("agents 6 in 412k out 33k cost $9.39 ctx 60.9%/272k"),
        "{stats:?}"
    );
    assert!(!stats.contains("R13M"));
    assert!(!stats.contains("CH99.6%"));
    assert!(!stats.contains("auto"));
    let theme = current_theme_if_initialized().expect("theme");
    assert!(lines[1].contains(&theme.fg(ThemeColor::Dim, "in 412k")));
    assert!(lines[1].contains(&js_format!(
        "{}60.9%{}",
        theme.fg(ThemeColor::Dim, "ctx "),
        theme.fg(ThemeColor::Dim, "/272k")
    )));
}

#[tokio::test]
async fn shows_configured_executable_name() {
    if std::env::var_os("PI_DEFAULT_FOOTER_CHILD").is_none() {
        let status = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", "shows_configured_executable_name", "--nocapture"])
            .env("PI_DEFAULT_FOOTER_CHILD", "1")
            .env("PI_EXECUTABLE_NAME", "pi-dev")
            .status()
            .expect("child test run");
        assert!(status.success());
        return;
    }
    let fixture = fixture(
        session_with_usage(0.0, 0.0, 0.0),
        Some(ContextUsage {
            tokens: None,
            context_window: 272_000.0,
            percent: None,
        }),
        Some(test_model(false)),
    )
    .await;
    let footer = component(&fixture, None);
    assert!(strip_ansi(&footer.render(160)[1]).contains("[pi-dev]"));
}

#[tokio::test]
async fn unknown_context_usage_is_fully_dimmed() {
    let fixture = fixture(
        session_with_usage(0.0, 0.0, 0.0),
        Some(ContextUsage {
            tokens: None,
            context_window: 272_000.0,
            percent: None,
        }),
        Some(test_model(false)),
    )
    .await;
    let footer = component(&fixture, None);
    let line = &footer.render(160)[1];
    let theme = current_theme_if_initialized().expect("theme");
    assert!(line.contains(&theme.fg(ThemeColor::Dim, "ctx ?/272k")));
    assert!(strip_ansi(line).contains("ctx ?/272k"));
}

#[tokio::test]
async fn narrow_footer_truncates_but_keeps_agent_count() {
    let fixture = fixture(
        session_with_usage(412_000.0, 33_000.0, 9.389),
        Some(ContextUsage {
            tokens: Some(1.0),
            context_window: 272_000.0,
            percent: Some(60.9),
        }),
        Some(test_model(false)),
    )
    .await;
    let footer = component(
        &fixture,
        Some(DefaultFooterAgentLifecycleCounts {
            running: 2,
            waiting_for_input: 3,
            steering_pending: 1,
        }),
    );
    let line = &footer.render(24)[1];
    assert!(visible_width(line) <= 24);
    assert!(strip_ansi(line).contains("agents 6"));
}

#[tokio::test]
async fn zero_counts_omit_agents_segment() {
    let fixture = fixture(
        session_with_usage(412_000.0, 33_000.0, 9.389),
        Some(ContextUsage {
            tokens: Some(1.0),
            context_window: 272_000.0,
            percent: Some(60.9),
        }),
        Some(test_model(false)),
    )
    .await;
    let footer = component(
        &fixture,
        Some(DefaultFooterAgentLifecycleCounts {
            running: 0,
            waiting_for_input: 0,
            steering_pending: 0,
        }),
    );
    assert!(!strip_ansi(&footer.render(160)[1]).contains("agents"));
}

#[tokio::test]
async fn extension_status_lines_group_goal_separately() {
    let fixture = fixture(
        session_with_usage(0.0, 0.0, 0.0),
        Some(ContextUsage {
            tokens: None,
            context_window: 272_000.0,
            percent: None,
        }),
        Some(test_model(false)),
    )
    .await;
    fixture.footer_data.set_extension_status(
        &JsString::from("bwrap"),
        Some(pi_coding_agent::core::footer_data_provider::ExtensionStatus::Text(
            JsString::from("bwrap ready"),
        )),
    );
    fixture.footer_data.set_extension_status(
        &JsString::from("goal"),
        Some(pi_coding_agent::core::footer_data_provider::ExtensionStatus::Text(
            JsString::from("goal: ship footer"),
        )),
    );
    fixture.footer_data.set_extension_status(
        &JsString::from("multi-agent-mode"),
        Some(pi_coding_agent::core::footer_data_provider::ExtensionStatus::Text(
            JsString::from("multi-agent: proactive"),
        )),
    );
    let footer = component(&fixture, None);
    let statuses = footer
        .render(160)
        .into_iter()
        .skip(2)
        .map(|line| strip_ansi(&line))
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        vec![
            "bwrap ready multi-agent: proactive".to_owned(),
            "goal: ship footer".to_owned()
        ]
    );
}

#[tokio::test]
async fn pwd_line_uses_override_and_restores_main_session() {
    let mut child = SessionManager::in_memory(Some(&JsString::from("/tmp/child")), None).expect("child session");
    child
        .append_message(assistant_message(23.0, 19.0, 2.0))
        .expect("child assistant");
    child
        .set_session_name(Some(&JsString::from("child")))
        .expect("session name");
    let override_session: Arc<Mutex<SessionManager>> = Arc::new(Mutex::new(child));

    let fixture = fixture(
        session_with_usage(11.0, 7.0, 1.0),
        Some(ContextUsage {
            tokens: Some(1.0),
            context_window: 272_000.0,
            percent: Some(60.9),
        }),
        Some(test_model(false)),
    )
    .await;
    let footer = component(&fixture, None);

    fixture.footer_data.set_session_override(FooterSessionOverride {
        cwd: JsString::from("/tmp/child"),
        session_manager: Some(
            pi_coding_agent::core::readonly_session_manager::create_readonly_session_manager(Arc::clone(
                &override_session,
            )),
        ),
        model: Some(test_model(true)),
        thinking_level: JsString::from("high"),
        context_usage: Some(pi_coding_agent::core::footer_data_provider::ContextUsage {
            tokens: Some(25_000.0),
            context_window: 100_000.0,
            percent: Some(25.0),
        }),
    });
    let child_output = JsString::join(footer.render(160), "\n");
    let child_output = strip_ansi(&child_output);
    assert!(child_output.contains("/tmp/child • child"), "{child_output:?}");
    assert!(!child_output.contains("/tmp/child (main)"));
    assert!(child_output.contains("in 23 out 19 cost $2.00 ctx 25.0%/100k"));
    assert!(child_output.contains("reasoner • effort high"));

    fixture.footer_data.clear_session_override();
    let main_output = strip_ansi(&JsString::join(footer.render(160), "\n"));
    assert!(main_output.contains("/tmp/project (main)"), "{main_output:?}");
    assert!(main_output.contains("reasoner"));
    assert!(!main_output.contains("child"));
}

#[tokio::test]
async fn effort_only_shown_for_reasoning_models() {
    let fixture = fixture(
        session_with_usage(0.0, 0.0, 0.0),
        Some(ContextUsage {
            tokens: None,
            context_window: 200_000.0,
            percent: None,
        }),
        Some(test_model(true)),
    )
    .await;
    let footer = component(&fixture, None);
    assert!(strip_ansi(&footer.render(120)[1]).contains("reasoner • effort high"));
}

fn spawn_agent(store: &MultiAgentStore, name: &str, slot: Option<i64>) -> JsString {
    let mut agent = AgentSnapshot {
        key_order: Vec::new(),
        id: store.allocate_agent_id_for_lifecycle_coordinator("agent").unwrap(),
        parent_id: None,
        display_name: JsString::from(name.to_owned()),
        agent_type: "worker".into(),
        origin: None,
        lifecycle: AgentLifecycle::Running,
        revision: 1,
        created_at: JsString::from("2026-06-27T00:00:00.000Z"),
        updated_at: JsString::from("2026-06-27T00:00:00.000Z"),
        cwd: JsString::from("/repo"),
        worktree: None,
        model: None,
        account: None,
        permission: AgentPermission {
            policy: "on-request".into(),
            inherited_from: None,
            narrowed: true,
        },
        slot: slot.map(|index| AgentSlot { index, pinned: true }),
        transcript: None,
        event_stream: None,
        worker: None,
        detached: None,
        current_activity: None,
        last_activity: None,
        result: None,
        error: None,
        extra: pi_js_compat::js_json::JsObject::new(),
    };
    let id = agent.id.clone();
    store.publish_lifecycle_coordinator_snapshot(&agent).unwrap();
    agent.id = JsString::default();
    id
}

#[test]
fn counts_only_running_waiting_and_steering_agents() {
    let store = MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| Ok("2026-06-27T00:00:00.000Z".into()))),
    });
    spawn_agent(&store, "Running", None);
    assert_eq!(
        count_default_footer_agents(&store),
        DefaultFooterAgentLifecycleCounts {
            running: 1,
            waiting_for_input: 0,
            steering_pending: 0,
        }
    );
}

#[tokio::test]
async fn selected_agent_shows_slot_and_state() {
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(|| Ok("2026-06-27T00:00:00.000Z".into()))),
    }));
    spawn_agent(&store, "First", Some(1));
    let second = spawn_agent(&store, "Second", Some(2));
    let mut snapshot = store.get_agent(&JsString::from(&second)).expect("second agent");
    snapshot.lifecycle = AgentLifecycle::WaitingForInput;
    snapshot.revision += 1;
    store.publish_lifecycle_coordinator_snapshot(&snapshot).unwrap();
    store.select_agent_slot(2).expect("select slot");

    let fixture = fixture(
        session_with_usage(0.0, 0.0, 0.0),
        Some(ContextUsage {
            tokens: None,
            context_window: 272_000.0,
            percent: None,
        }),
        Some(test_model(false)),
    )
    .await;
    let store_for_counts = Arc::clone(&store);
    let store_for_selected = Arc::clone(&store);
    let footer = create_default_footer_component(DefaultFooterComponentInput {
        ctx: fixture.harness.context(),
        footer_data: Arc::clone(&fixture.footer_data),
        get_agent_counts: Some(Arc::new(move || Some(count_default_footer_agents(&store_for_counts)))),
        get_selected_agent: Some(Arc::new(move || {
            let selected_id = store_for_selected.get_selected_agent_id()?;
            let agent = store_for_selected.get_agent(&selected_id)?;
            Some(
                pi_coding_agent::bundled_extensions::default_footer::DefaultFooterSelectedAgent {
                    display_name: agent.display_name.clone(),
                    lifecycle: agent.lifecycle,
                    slot_index: agent.slot.as_ref().map(|slot| slot.index),
                },
            )
        })),
        theme: Arc::clone(&current_theme_if_initialized().expect("theme")),
    });
    let line = strip_ansi(&footer.render(160)[1]);
    assert!(line.contains("selected #2 Second waiting for input"), "{line:?}");
}

#[tokio::test]
async fn extension_registers_default_footer_factory_on_session_start() {
    let runtime = create_extension_runtime();
    let extension = support::extension(&runtime, "/tmp/project", "<first-party:default-footer>", |pi| {
        default_footer_extension(
            pi,
            &pi_coding_agent::bundled_extensions::default_footer::DefaultFooterExtensionOptions::default(),
        )
        .expect("default footer extension")
    })
    .await;
    let mut options = HarnessOptions::new("/tmp/project");
    options.model = Some(test_model(true));
    options.thinking_level = ThinkingLevel::High;
    let harness = Harness::new(vec![extension], runtime, options).await;
    let mut event = ExtensionEvent::SessionStart(SessionStartEvent {
        reason: SessionStartReason::Startup,
        previous_session_file: None,
    });
    harness.runner.emit(&mut event).await;

    let factory = harness
        .ui
        .footer_factories
        .lock()
        .unwrap()
        .first()
        .cloned()
        .expect("footer factory");
    let footer_data = FooterDataProvider::with_git_runner(
        JsString::from(git_fixture_dir().to_str().expect("fixture path must be UTF-8")),
        Arc::new(StubGit),
    );
    let component = factory(ExtensionUiFooterContext {
        tui: ExtensionUiTui::new(Arc::new(StubTui {
            renders: Arc::new(Mutex::new(0)),
        })),
        theme: Arc::clone(&current_theme_if_initialized().expect("theme")),
        footer_data,
    });
    let line = strip_ansi(&component.render(160)[1]);
    assert!(line.contains("reasoner • effort high"), "{line:?}");
}

#[path = "support/footer_callable_source.rs"]
mod callable_source;

#[tokio::test]
async fn default_footer_checked_render_preserves_callable_status_throw() {
    use pi_coding_agent::core::footer_data_provider::ExtensionStatus;
    use pi_js_compat::abort::AbortReason;
    use pi_js_compat::js_json::{JsValue, parse_lossless, stringify_lossless, to_value};
    use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
    let directory = tempfile::tempdir().unwrap();
    let model = test_model(false);
    let source = callable_source::run("default-checked", directory.path(), to_value(&model).unwrap());
    assert!(source.status.success(), "{}", String::from_utf8_lossy(&source.stderr));
    let expected = parse_lossless(String::from_utf8(source.stdout).unwrap().trim()).unwrap();
    assert_eq!(expected["sameError"], JsValue::Bool(true));
    let fixture = fixture(session_with_usage(0.0, 0.0, 0.0), None, Some(model)).await;
    fixture.footer_data.set_extension_status(
        &"throw".into(),
        Some(ExtensionStatus::Callback(Arc::new(|view| {
            assert!(view.is_none());
            Err(NativeNodeError::from_constructor(
                NativeErrorClass::TypeError,
                JsString::from("status-") + JsString::from_utf16(vec![0xd800]),
            )
            .with_property("code", JsValue::String("ESTATUS".into()))
            .into())
        }))),
    );
    let error = component(&fixture, None).render_checked(200).unwrap_err();
    let actual = match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => error.value(),
        AbortReason::Value(value) => value,
    };
    assert_eq!(stringify_lossless(&actual), stringify_lossless(&expected["error"]));
}

#[path = "support/oracle.rs"]
mod oracle;

#[derive(Default)]
struct LifecycleUi {
    calls: Mutex<Vec<pi_js_compat::js_json::JsValue>>,
}
impl pi_coding_agent::core::extensions::types::ExtensionUiContext for LifecycleUi {
    fn set_status(
        &self,
        key: &JsString,
        text: Option<pi_coding_agent::core::footer_data_provider::ExtensionStatus>,
    ) -> Result<(), pi_ai::request::CallbackError> {
        let text = text.map(|text| text.evaluate(None)).transpose()?.flatten();
        self.calls
            .lock()
            .unwrap()
            .push(pi_js_compat::js_json!(["status", key, text]));
        Ok(())
    }
    fn request_render(&self) {
        self.calls.lock().unwrap().push(pi_js_compat::js_json!(["render"]));
    }
    fn set_default_footer(&self, _: Option<pi_coding_agent::core::extensions::types::ExtensionUiFooterFactory>) {
        self.calls.lock().unwrap().push(pi_js_compat::js_json!(["footer"]));
    }
}

#[tokio::test]
async fn background_compaction_status_lifecycle_matches_frozen() {
    use pi_coding_agent::core::extensions::types::{
        BackgroundCompactionEndEvent, BackgroundCompactionStartEvent, ExtensionMode, SessionShutdownEvent,
        SessionShutdownReason,
    };
    use pi_js_compat::js_json;
    let mut ts = oracle::TsOracle::spawn(
        "default-footer-oracle.ts",
        "packages/coding-agent/extensions/default-footer/src/index.ts",
    );
    let expected = ts.request(&js_json!({"id":81,"operation":"lifecycle"}));
    assert_eq!(expected["ok"], js_json!(true));
    let runtime = create_extension_runtime();
    let extension = support::extension(&runtime, "/tmp/project", "<footer>", |pi| {
        default_footer_extension(pi, &Default::default()).unwrap()
    })
    .await;
    let mut harness = Harness::new(vec![extension], runtime, HarnessOptions::new("/tmp/project")).await;
    let ui = Arc::new(LifecycleUi::default());
    harness.runner.set_ui_context(Some(ui.clone()), ExtensionMode::Rpc);
    for mut event in [
        ExtensionEvent::BackgroundCompactionStart(BackgroundCompactionStartEvent {}),
        ExtensionEvent::BackgroundCompactionEnd(BackgroundCompactionEndEvent {}),
        ExtensionEvent::BackgroundCompactionStart(BackgroundCompactionStartEvent {}),
        ExtensionEvent::SessionShutdown(SessionShutdownEvent {
            reason: SessionShutdownReason::Quit,
            target_session_file: None,
        }),
        ExtensionEvent::SessionStart(SessionStartEvent {
            reason: SessionStartReason::Startup,
            previous_session_file: None,
        }),
    ] {
        harness.runner.emit(&mut event).await;
    }
    assert_eq!(js_json!(ui.calls.lock().unwrap().clone()), expected["output"]);
}
