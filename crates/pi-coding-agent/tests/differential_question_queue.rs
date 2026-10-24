#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/tool_context.rs"]
mod tool_context;

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
    ) -> BoxFuture<'a, Result<Option<JsValue>, JsString>> {
        Box::pin(async move {
            let (sender, receiver) = oneshot::channel();
            self.pending.lock().unwrap().push(Some(sender));
            self.entered.notify_one();
            receiver.await.expect("panel reply").map(Some)
        })
    }
    fn notify(&self, body: &JsString, kind: Option<NotifyType>) {
        let kind = kind.map(|kind| match kind {
            NotifyType::Info => "info",
            NotifyType::Warning => "warning",
            NotifyType::Error => "error",
        });
        self.notifications
            .lock()
            .unwrap()
            .push(json!({"body":body,"kind":kind}));
    }
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
