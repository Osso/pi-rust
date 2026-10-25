use crate::shared_oracle as oracle;
use crate::shared_tool_context as tool_context;

use pi_ai::request::BoxFuture;
use pi_coding_agent::core::extensions::types::{
    ContextTool, ExtensionMode, ExtensionUiContext, ExtensionUiCustomFactory, ExtensionUiCustomOptions, NotifyType,
};
use pi_coding_agent::core::session_control_db::{RuntimeMailboxMessage, list_runtime_mailbox_messages};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::tools::ask_questions::AskQuestionsTool;
use pi_js_compat::{
    js_json as json,
    js_json::{self, JsString, JsValue},
};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Notify, oneshot};

type PanelSender = oneshot::Sender<Result<JsValue, JsString>>;
#[derive(Default)]
struct Ui {
    pending: Mutex<Vec<Option<PanelSender>>>,
    notifications: Mutex<Vec<JsValue>>,
    entered: Notify,
    notify_error: Option<pi_js_compat::callback_error::CallbackError>,
}
impl Ui {
    async fn wait_panels(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while self.pending.lock().unwrap().len() < count {
                self.entered.notified().await;
            }
        })
        .await
        .expect("panel entry");
    }
    fn settle(&self, index: usize, answer: Result<JsValue, JsString>) {
        self.pending.lock().unwrap()[index]
            .take()
            .expect("pending panel")
            .send(answer)
            .expect("panel waiter");
    }
}
impl ExtensionUiContext for Ui {
    fn custom<'a>(
        &'a self,
        _: ExtensionUiCustomFactory,
        _: Option<ExtensionUiCustomOptions>,
    ) -> BoxFuture<'a, Result<Option<JsValue>, pi_js_compat::callback_error::CallbackError>> {
        Box::pin(async move {
            let (sender, receiver) = oneshot::channel();
            self.pending.lock().unwrap().push(Some(sender));
            self.entered.notify_one();
            receiver.await.expect("panel reply").map(Some).map_err(|reason| {
                pi_js_compat::callback_error::CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Value(
                    JsValue::String(reason),
                ))
            })
        })
    }
    fn notify(
        &self,
        body: &JsString,
        kind: Option<NotifyType>,
    ) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        let kind = kind.map(|kind| match kind {
            NotifyType::Info => "info",
            NotifyType::Warning => "warning",
            NotifyType::Error => "error",
        });
        self.notifications
            .lock()
            .unwrap()
            .push(json!({"body":body,"kind":kind}));
        if let Some(error) = &self.notify_error {
            return Err(error.clone());
        }
        Ok(())
    }
}

fn write_queue_session(root: &std::path::Path, name: &str, id: &str) -> JsString {
    let path = root.join(name);
    let header = json!({"type":"session","version":3,"id":id,"timestamp":"2026-10-24T00:00:00.000Z","cwd":"/repo"});
    std::fs::write(&path, format!("{}\n", js_json::stringify_lossless(&header))).unwrap();
    pi_js_compat::node_fs::decode_node_path(path.as_os_str())
}

#[test]
fn queued_notification_throw_reaches_the_outer_catch_without_enqueuing_its_failure_body() {
    let root = tempfile::tempdir().unwrap();
    let first = write_queue_session(root.path(), "first.jsonl", "initial-session");
    let second = write_queue_session(root.path(), "second.jsonl", "next-session");
    let raw = JsString::from_utf16(vec![0xd800]);
    let question = JsString::from("Question-") + &raw;
    let mut answers = pi_js_compat::js_json::JsObject::new();
    answers.insert(question.clone(), JsValue::String(JsString::from("answer-") + &raw));
    let mut request = json!({"id":1,"db":pi_js_compat::node_fs::decode_node_path(root.path().join("source.sqlite").as_os_str()),"sessionFile":first,"nextSessionFile":second,"question":question,"metadata":{"raw":raw},"reason":raw,"notifyError":JsString::from_utf16(vec![0xdfff]),"answer":{"answers":JsValue::Object(answers),"cancelled":false}});
    let mut oracle = oracle::TsOracle::spawn(
        "question-notify-throw-oracle.ts",
        "packages/coding-agent/src/core/tools/ask-questions.ts",
    );
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(expected["output"]["beforeSecondCount"], json!(0));
    request["db"] = json!(pi_js_compat::node_fs::decode_node_path(
        root.path().join("native.sqlite").as_os_str()
    ));
    let stderr = root.path().join("native.stderr");
    request["stderrPath"] = json!(pi_js_compat::node_fs::decode_node_path(stderr.as_os_str()));
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "queued_notify_throw_probe", "--ignored", "--nocapture"])
        .env("PI_NOTIFY_THROW_REQUEST", js_json::stringify_lossless(&request))
        .stderr(std::fs::File::create(&stderr).unwrap())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", std::fs::read_to_string(&stderr).unwrap());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let record = stdout
        .lines()
        .find_map(|line| line.strip_prefix("NATIVE_QUEUE_RESULT="))
        .expect("child queue result");
    let mut actual = js_json::parse_lossless(record).unwrap();
    actual["consoleLines"] = json!(std::fs::read_to_string(&stderr).unwrap().lines().collect::<Vec<_>>());
    assert_eq!(actual, expected["output"]);
}

#[tokio::test]
#[ignore = "explicit private child probe capturing the outer delivery catch stderr"]
async fn queued_notify_throw_probe() {
    use pi_js_compat::{abort::AbortReason, callback_error::CallbackError};
    let request =
        js_json::parse_lossless(std::env::var("PI_NOTIFY_THROW_REQUEST").expect("private probe request")).unwrap();
    let path = request["sessionFile"].as_string().unwrap();
    let manager = Arc::new(Mutex::new(
        SessionManager::open(path, None, Some(&JsString::from("/repo"))).unwrap(),
    ));
    let db = request["db"].as_string().unwrap();
    list_runtime_mailbox_messages(db).unwrap();
    let ui = Arc::new(Ui {
        notify_error: Some(CallbackError::from_rejection(AbortReason::Value(
            request["notifyError"].clone(),
        ))),
        ..Default::default()
    });
    let context = tool_context::TestContextBuilder::new("/repo", manager.clone())
        .mode(ExtensionMode::Tui)
        .ui(ui.clone())
        .control_db_path(db.clone())
        .build()
        .await
        .1;
    let params =
        json!({"blocking":false,"questions":[{"question":request["question"]}],"metadata":request["metadata"]});
    let mut queued = Vec::new();
    for id in ["one", "two"] {
        queued.push(
            AskQuestionsTool::new()
                .execute(id.into(), params.clone(), None, None, Some(context.clone()), None)
                .await
                .unwrap(),
        );
    }
    ui.wait_panels(1).await;
    manager
        .lock()
        .unwrap()
        .set_session_file(request["nextSessionFile"].as_string().unwrap())
        .unwrap();
    ui.settle(0, Err(request["reason"].as_string().unwrap().clone()));
    ui.wait_panels(2).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if std::fs::read_to_string(request["stderrPath"].as_string().unwrap().to_utf8().unwrap())
                .unwrap()
                .contains("Failed question delivery handler (request one):")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("outer delivery catch log");
    let before = list_runtime_mailbox_messages(db).unwrap().len();
    ui.settle(1, Ok(request["answer"].clone()));
    let messages=wait_messages(db,1).await.into_iter().map(|message|json!({"id":message.id,"recipient":message.recipient,"sender":message.sender,"kind":message.kind,"body":message.body,"storeRef":message.store_ref,"status":message.status})).collect::<Vec<_>>();
    let actual = json!({"queued":queued,"notifications":ui.notifications.lock().unwrap().clone(),"entered":ui.pending.lock().unwrap().len(),"beforeSecondCount":before,"messages":messages});
    println!("NATIVE_QUEUE_RESULT={}", js_json::stringify_lossless(&actual));
}

async fn wait_notifications(ui: &Ui) -> Vec<JsValue> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let messages = ui.notifications.lock().unwrap().clone();
            if !messages.is_empty() {
                return messages;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("question error notification")
}

#[tokio::test]
async fn durable_enqueue_failure_keeps_the_queued_result_and_notifies_with_the_frozen_error() {
    let root = tempfile::tempdir().unwrap();
    let session = root.path().join("session.jsonl");
    std::fs::write(&session, "{\"type\":\"session\",\"version\":3,\"id\":\"initial-session\",\"timestamp\":\"2026-10-24T00:00:00.000Z\",\"cwd\":\"/repo\"}\n").unwrap();
    let session = pi_js_compat::node_fs::decode_node_path(session.as_os_str());
    let db_path = root.path().join("unopenable-database");
    std::fs::create_dir(&db_path).unwrap();
    let db = pi_js_compat::node_fs::decode_node_path(db_path.as_os_str());
    let answer = json!({"answers":{"Question?":"answer"},"cancelled":false});
    let mut oracle = oracle::TsOracle::spawn(
        "question-queue-oracle.ts",
        "packages/coding-agent/src/core/tools/ask-questions.ts",
    );
    let expected=oracle.request(&json!({"id":1,"operation":"delivery-error","db":db,"sessionFile":session,"nextSessionFile":session,"question":"Question?","answer":answer}));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let manager = Arc::new(Mutex::new(
        SessionManager::open(&session, None, Some(&JsString::from("/repo"))).unwrap(),
    ));
    let ui = Arc::new(Ui::default());
    let context = tool_context::TestContextBuilder::new("/repo", manager)
        .mode(ExtensionMode::Tui)
        .ui(ui.clone())
        .control_db_path(db)
        .build()
        .await
        .1;
    let result = AskQuestionsTool::new()
        .execute(
            "failure".into(),
            json!({"blocking":false,"questions":[{"question":"Question?"}]}),
            None,
            None,
            Some(context),
            None,
        )
        .await
        .unwrap();
    ui.wait_panels(1).await;
    ui.settle(0, Ok(answer));
    let notifications = wait_notifications(&ui).await;
    let actual = json!({"queued":[result],"notifications":notifications,"entered":ui.pending.lock().unwrap().len()});
    assert_eq!(actual, expected["output"]);
}

async fn wait_messages(db: &JsString, count: usize) -> Vec<RuntimeMailboxMessage> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let messages = list_runtime_mailbox_messages(db).unwrap();
            if messages.len() == count {
                return messages;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("durable question replies")
}

#[tokio::test]
async fn queued_failure_and_next_answer_keep_the_captured_session_after_a_live_switch() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first.jsonl");
    let second = root.path().join("second.jsonl");
    for (path, id) in [(&first, "initial-session"), (&second, "next-session")] {
        let header = json!({"type":"session","version":3,"id":id,"timestamp":"2026-10-24T00:00:00.000Z","cwd":"/repo"});
        std::fs::write(path, format!("{}\n", js_json::stringify_lossless(&header))).unwrap();
    }
    let first = pi_js_compat::node_fs::decode_node_path(first.as_os_str());
    let second = pi_js_compat::node_fs::decode_node_path(second.as_os_str());
    let directory = pi_js_compat::node_fs::decode_node_path(root.path().as_os_str());
    let raw = JsString::from_utf16(vec![0xd800]);
    let question = JsString::from("Question-") + &raw;
    let reason = JsString::from("failed-") + &raw;
    let metadata = json!({"raw":raw,"nested":[true,null]});
    let mut answers = pi_js_compat::js_json::JsObject::new();
    answers.insert(question.clone(), JsValue::String(JsString::from("answer-") + &raw));
    let answer = json!({"answers":JsValue::Object(answers),"cancelled":false});
    let params = json!({"blocking":false,"questions":[{"question":question}],"metadata":metadata});
    let mut oracle = oracle::TsOracle::spawn(
        "question-queue-oracle.ts",
        "packages/coding-agent/src/core/tools/ask-questions.ts",
    );
    for (index, agent) in [None, Some(JsString::default()), Some(JsString::from("child"))]
        .into_iter()
        .enumerate()
    {
        let source_db =
            pi_js_compat::node_fs::decode_node_path(root.path().join(format!("source-{index}.sqlite")).as_os_str());
        let db =
            pi_js_compat::node_fs::decode_node_path(root.path().join(format!("native-{index}.sqlite")).as_os_str());
        let expected=oracle.request(&json!({"id":1,"db":source_db,"sessionFile":first,"nextSessionFile":second,"agentId":agent,"question":question,"metadata":metadata,"reason":reason,"answer":answer}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        let manager = Arc::new(Mutex::new(
            SessionManager::open(&first, Some(&directory), Some(&JsString::from("/repo"))).unwrap(),
        ));
        let ui = Arc::new(Ui::default());
        let mut builder = tool_context::TestContextBuilder::new("/repo", manager.clone())
            .mode(ExtensionMode::Tui)
            .ui(ui.clone())
            .control_db_path(db.clone());
        if let Some(agent) = agent {
            builder = builder.agent_id(agent);
        }
        let context = builder.build().await.1;
        list_runtime_mailbox_messages(&db).unwrap();
        let mut queued = Vec::new();
        for id in ["one", "two"] {
            queued.push(
                js_json::to_value(
                    &AskQuestionsTool::new()
                        .execute(id.into(), params.clone(), None, None, Some(context.clone()), None)
                        .await
                        .unwrap(),
                )
                .unwrap(),
            );
        }
        ui.wait_panels(1).await;
        assert_eq!(
            ui.pending.lock().unwrap().len(),
            1,
            "second panel must wait for first settlement"
        );
        manager.lock().unwrap().set_session_file(&second).unwrap();
        assert_eq!(manager.lock().unwrap().get_session_id(), "next-session");
        ui.settle(0, Err(reason.clone()));
        ui.wait_panels(2).await;
        wait_messages(&db, 1).await;
        ui.settle(1, Ok(answer.clone()));
        let messages=wait_messages(&db,2).await.into_iter().map(|message|json!({"id":message.id,"recipient":message.recipient,"sender":message.sender,"kind":message.kind,"body":message.body,"storeRef":message.store_ref,"status":message.status})).collect::<Vec<_>>();
        let actual = json!({"queued":queued,"notifications":ui.notifications.lock().unwrap().clone(),"entered":ui.pending.lock().unwrap().len(),"messages":messages});
        assert_eq!(actual, expected["output"], "recipient case {index}");
    }
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/tool_context.rs"]
pub mod shared_tool_context;
