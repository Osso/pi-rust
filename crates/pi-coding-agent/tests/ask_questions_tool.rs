//! Frozen core question-tool and panel proofs, all state isolated by Cargo runner.
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/tool_context.rs"]
mod tool_context;
use futures_util::FutureExt;
use pi_agent_core::AgentHarnessTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::extensions::types::*;
use pi_coding_agent::core::session_control_db::{get_control_db_path, list_runtime_mailbox_messages};
use pi_coding_agent::core::tools::ask_questions::AskQuestionsTool;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
struct Backend;
impl ExtensionUiTuiBackend for Backend {
    fn create_fixed_cell(&self) -> Rc<dyn pi_tui::components::editor::EditorFixedCell> {
        panic!("no editor")
    }
    fn request_render(&self) {}
    fn request_render_force(&self) {}
    fn stop(&self) {}
    fn start(&self) -> Result<(), String> {
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
    fn show_overlay(
        &self,
        _: Box<dyn ExtensionUiComponent>,
        _: Option<ExtensionUiOverlayOptions>,
    ) -> Rc<dyn ExtensionUiOverlayHandle> {
        panic!("no overlays")
    }
}
struct Ui {
    scripts: Mutex<VecDeque<Result<Vec<String>, String>>>,
    seen: Mutex<Vec<String>>,
}
impl Ui {
    fn new(scripts: Vec<Result<Vec<String>, String>>) -> Arc<Self> {
        Arc::new(Self {
            scripts: Mutex::new(scripts.into()),
            seen: Mutex::new(Vec::new()),
        })
    }
}
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
            let keys = self.scripts.lock().unwrap().pop_front().expect("UI request")?;
            let result = Arc::new(Mutex::new(None));
            let output = result.clone();
            let component = factory(
                ExtensionUiFactoryContext {
                    tui: ExtensionUiTui::new(Arc::new(Backend)),
                    theme: self.theme(),
                    keybindings: pi_coding_agent::core::keybindings::KeybindingsManager::new(Vec::new(), None),
                },
                ExtensionUiDone::new(Rc::new(move |value| *output.lock().unwrap() = value)),
            )?
            .now_or_never()
            .expect("ready panel")?;
            self.seen.lock().unwrap().push(component.render(80).join("\n"));
            component.set_focused(true);
            for key in keys {
                component.handle_input(&key);
                self.seen.lock().unwrap().push(component.render(80).join("\n"));
            }
            let result = result.lock().unwrap().clone();
            Ok(result)
        })
    }
}
fn keys(keys: &[&str]) -> Result<Vec<String>, String> {
    Ok(keys.iter().map(|key| (*key).to_owned()).collect())
}
async fn ctx(ui: Arc<Ui>) -> ExtensionContext {
    tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "session"))
        .mode(ExtensionMode::Tui)
        .ui(ui)
        .build()
        .await
        .1
}
#[tokio::test]
async fn explicit_boolean_is_required_before_ui_or_question_validation() {
    let context = tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "session"))
        .build()
        .await
        .1;
    for blocking in [None, Some(Value::Null), Some(json!("true")), Some(json!(0))] {
        let mut params = json!({"questions":[]});
        if let Some(blocking) = blocking {
            params["blocking"] = blocking;
        }
        let error = AskQuestionsTool::new()
            .execute("id".into(), params, None, None, context.clone(), None)
            .await
            .unwrap_err();
        assert_eq!(error.message, "ask_questions requires explicit blocking: true or false");
    }
}
#[tokio::test]
async fn blocking_panels_match_frozen_answers_for_real_keystrokes() {
    let mut oracle = oracle::TsOracle::spawn("coding-agent-question-panel-oracle.ts", "packages/coding-agent/src");
    for (questions, script) in [
        (
            json!([{"question":"Choose?","options":[{"label":"A"},{"label":"B"}]}]),
            vec!["\u{1b}[B", "\r"],
        ),
        (json!([{"question":"Free?"}]), vec!["  answer  ", "\r"]),
        (json!([{"question":"__proto__"}]), vec!["  answer  ", "\r"]),
        (
            json!([{"question":"2"},{"question":"1"}]),
            vec!["second", "\r", "first", "\r", "\r"],
        ),
        (
            json!([{"question":"Other?","options":[{"label":"A"},{"label":"B"}]}]),
            vec!["\u{1b}[B", "\u{1b}[B", "\r", "other", "\r"],
        ),
        (
            json!([{"question":"Multi?","multiSelect":true,"options":[{"label":"A"},{"label":"B"}]}]),
            vec!["\u{1b}[B", "\r", "\u{1b}[A", "\r", "\u{1b}[C", "\r"],
        ),
        (
            json!([{"question":"First?"},{"question":"Second?"}]),
            vec!["partial", "\t", "\u{1b}"],
        ),
        (
            json!([{"question":"Multi?","multiSelect":true,"options":[{"label":"A"},{"label":"B"}]}]),
            vec!["\u{1b}[C", "\r"],
        ),
    ] {
        let expected = oracle.request(&json!({"questions":questions,"keys":script}));
        let ui = Ui::new(vec![keys(&script)]);
        let result = AskQuestionsTool::new()
            .execute(
                "id".into(),
                json!({"blocking":true,"questions":questions}),
                None,
                None,
                ctx(ui.clone()).await,
                None,
            )
            .await
            .unwrap_or_else(|error| panic!("{questions}: {script:?}: frozen={expected}: {error:?}"));
        assert_eq!(
            result.details.as_ref().unwrap()["answers"],
            expected["result"]["answers"],
            "{questions}"
        );
        assert_eq!(
            result.details.as_ref().unwrap()["cancelled"],
            expected["result"]["cancelled"]
        );
        assert_eq!(
            serde_json::to_value(&result.content).unwrap()[0]["text"],
            expected["toolText"],
            "answer enumeration for {questions}"
        );
        let strip = pi_coding_agent::utils::ansi::strip_ansi;
        let actual = ui
            .seen
            .lock()
            .unwrap()
            .iter()
            .map(|text| strip(text))
            .collect::<Vec<_>>();
        let renders = expected["renders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|text| strip(text.as_str().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(actual, renders, "panel rendering for {questions}");
    }
}
#[tokio::test]
async fn nonblocking_answers_and_cancellation_arrive_via_correlated_inbox() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let db = get_control_db_path(dir.path());
    let ui = Ui::new(vec![keys(&["answer", "\r"]), keys(&["\u{1b}"])]);
    let context = tool_context::TestContextBuilder::new(root, tool_context::persisted_session(root, root, "session"))
        .mode(ExtensionMode::Tui)
        .ui(ui.clone())
        .control_db_path(&db)
        .build()
        .await
        .1;
    for id in ["one", "two"] {
        let result = AskQuestionsTool::new()
            .execute(
                id.into(),
                json!({"blocking":false,"questions":[{"question":"Free?"}]}),
                None,
                None,
                context.clone(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(result.details.as_ref().unwrap()["queued"], true);
        assert_eq!(result.details.as_ref().unwrap()["requestId"], id);
    }
    for _ in 0..100 {
        if list_runtime_mailbox_messages(&db).unwrap().len() == 2 {
            break;
        }
        tokio::task::yield_now().await;
    }
    let messages = list_runtime_mailbox_messages(&db).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].store_ref.message_id, "ask_questions:one");
    assert_eq!(
        messages[0].body,
        "User answered questions (request one):\n- Free? → answer"
    );
    assert_eq!(messages[1].body, "Questions cancelled (request two)\n");
}
#[tokio::test]
async fn failed_panel_does_not_strand_later_requests() {
    let ui = Ui::new(vec![Err("panel failed".into()), keys(&["next", "\r"])]);
    let context = ctx(ui).await;
    let params = json!({"blocking":true,"questions":[{"question":"Free?"}]});
    let (first, second) = tokio::join!(
        AskQuestionsTool::new().execute("one".into(), params.clone(), None, None, context.clone(), None),
        AskQuestionsTool::new().execute("two".into(), params, None, None, context, None)
    );
    assert_eq!(first.unwrap_err().message, "panel failed");
    assert_eq!(second.unwrap().details.unwrap()["answers"]["Free?"], "next");
}

#[derive(Default)]
struct PendingUi {
    panels: Mutex<Vec<tokio::sync::oneshot::Sender<Value>>>,
    entered: tokio::sync::Notify,
}
impl PendingUi {
    fn settle(&self) {
        let sender = self.panels.lock().unwrap().remove(0);
        sender
            .send(json!({"answers":{"Question?":"settled"},"cancelled":false}))
            .unwrap();
    }
}
impl ExtensionUiContext for PendingUi {
    fn custom<'a>(
        &'a self,
        _: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<Value>, String>> {
        Box::pin(async move {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            self.panels.lock().unwrap().push(sender);
            self.entered.notify_one();
            Ok(Some(receiver.await.unwrap()))
        })
    }
}

#[tokio::test]
async fn agent_abort_keeps_pending_panel_fifo_until_settlement() {
    use pi_agent_core::agent::{Agent, AgentInitialState, AgentOptions};
    use pi_ai::providers::faux::{
        FauxAssistantMessageOptions, create_faux_core, faux_assistant_message, faux_tool_call,
    };
    use pi_ai::types::{AssistantMessageEvent, DoneReason, StopReason};
    use pi_ai::utils::event_stream::create_assistant_message_event_stream;
    use pi_coding_agent::core::tools::{
        ask_questions::create_ask_questions_tool_definition, tool_definition_wrapper::wrap_definition,
    };
    let ui = Arc::new(PendingUi::default());
    let context = tool_context::TestContextBuilder::new("/repo", tool_context::in_memory_session("/repo", "session"))
        .mode(ExtensionMode::Tui)
        .ui(ui.clone())
        .build()
        .await
        .1;
    let params = json!({"blocking":true,"questions":[{"question":"Question?"}]});
    let model = create_faux_core(Default::default()).get_model(None).unwrap();
    let call_params = params.clone();
    let agent_context = context.clone();
    let agent = Agent::new(AgentOptions {
        initial_state: Some(AgentInitialState {
            model: Some(model),
            tools: Some(vec![wrap_definition(
                create_ask_questions_tool_definition(),
                Arc::new(move || agent_context.clone()),
            )]),
            ..Default::default()
        }),
        stream_fn: Some(Arc::new(move |_, _, _| {
            let message = faux_assistant_message(
                faux_tool_call(
                    "ask_questions",
                    call_params.as_object().unwrap().clone(),
                    Some("first".into()),
                ),
                FauxAssistantMessageOptions {
                    stop_reason: Some(StopReason::ToolUse),
                    ..Default::default()
                },
            );
            Box::pin(async move {
                let (sender, stream) = create_assistant_message_event_stream();
                sender.push(AssistantMessageEvent::Done {
                    reason: DoneReason::ToolUse,
                    message,
                });
                stream
            })
        })),
        ..Default::default()
    })
    .unwrap();
    let running = agent.clone();
    let first = tokio::spawn(async move { running.prompt("ask").await });
    ui.entered.notified().await;
    agent.abort();
    tokio::time::timeout(std::time::Duration::from_secs(2), first)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!agent.is_streaming());
    let mut second = tokio::spawn(AskQuestionsTool::new().execute("second".into(), params, None, None, context, None));
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        ui.panels.lock().unwrap().len(),
        1,
        "aborting the tool waiter must not start a simultaneous panel"
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut second)
            .await
            .is_err()
    );
    ui.settle();
    ui.entered.notified().await;
    assert_eq!(ui.panels.lock().unwrap().len(), 1);
    ui.settle();
    assert_eq!(
        second.await.unwrap().unwrap().details.unwrap()["answers"]["Question?"],
        "settled"
    );
}
