#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::request::BoxFuture;
use pi_coding_agent::{
    core::{
        session_control_db::{
            PostSupervisorRequestInput, SupervisorRequestKind, claim_next_supervisor_request, post_supervisor_request,
            read_supervisor_request,
        },
        session_manager::SessionEntry,
    },
    supervisor::main::{SupervisorSession, process_supervisor_request},
};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue},
};
use std::sync::{Arc, Mutex};

struct PolicySession {
    branch: Mutex<Vec<SessionEntry>>,
    entry: SessionEntry,
    calls: Arc<Mutex<Vec<JsValue>>>,
    feedback: Arc<Mutex<Vec<JsValue>>>,
    percent: f64,
    database: JsString,
    request_id: i64,
    feedback_error: bool,
}
impl SupervisorSession for PolicySession {
    fn send_custom_message(
        &self,
        message: pi_coding_agent::core::extensions::types::CustomMessagePayload,
    ) -> Option<BoxFuture<'static, Result<(), JsString>>> {
        let status = read_supervisor_request(&self.database, self.request_id)
            .unwrap()
            .unwrap()
            .status;
        self.feedback
            .lock()
            .unwrap()
            .push(json!({"message":message,"status":status}));
        let fail = self.feedback_error;
        Some(Box::pin(async move {
            if fail { Err("feedback failed".into()) } else { Ok(()) }
        }))
    }

    fn abort(&self) -> BoxFuture<'static, Result<(), JsString>> {
        self.calls.lock().unwrap().push(json!({"kind":"abort"}));
        Box::pin(async { Ok(()) })
    }
    fn prompt(&self, content: JsString) -> BoxFuture<'static, Result<(), JsString>> {
        self.calls
            .lock()
            .unwrap()
            .push(json!({"kind":"prompt","content":content}));
        self.branch.lock().unwrap().push(self.entry.clone());
        Box::pin(async { Ok(()) })
    }
    fn compact(&self, instructions: &JsString) -> Option<BoxFuture<'static, Result<(), JsString>>> {
        self.calls
            .lock()
            .unwrap()
            .push(json!({"kind":"compact","content":instructions}));
        Some(Box::pin(async { Ok(()) }))
    }
    fn context_percent(&self) -> Option<f64> {
        Some(self.percent)
    }
    fn session_id(&self) -> Option<JsString> {
        Some("resident-fixture".into())
    }
    fn get_branch(&self) -> Vec<SessionEntry> {
        self.branch.lock().unwrap().clone()
    }
    fn get_leaf_id(&self) -> Option<JsString> {
        self.branch.lock().unwrap().last().and_then(SessionEntry::id).cloned()
    }
}
fn entry(response: &JsValue, end_turn: bool) -> JsValue {
    let content = if end_turn {
        json!([{"type":"toolCall","id":"finish","name":"end_turn","arguments":{}}])
    } else {
        json!([{"type":"text","text":pi_js_compat::js_json::stringify_lossless(response)}])
    };
    json!({"type":"message","id":"reply","parentId":null,"timestamp":"2026-01-01T00:00:00.000Z","message":{
        "role":"assistant","content":content,"api":"faux","provider":"openai-codex","model":"gpt-6.1-sol",
        "usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},
        "stopReason":if end_turn {"toolUse"} else {"stop"},"timestamp":0,
    }})
}
async fn compare_default_case(end_turn: bool) {
    let directory = tempfile::tempdir().unwrap();
    let response = json!({"kind":"continue","instructions":"Inspect evidence.","reason":"bounded"});
    let message = entry(&response, end_turn);
    let kind = if end_turn {
        SupervisorRequestKind::ApprovalReview
    } else {
        SupervisorRequestKind::GoalIdleReview
    };
    let mut source = oracle::TsOracle::spawn(
        "coding-agent-supervisor-main-oracle.ts",
        "packages/coding-agent/src/supervisor/main.ts",
    );
    let expected = source.request(
        &json!({"id":1,"operation":"processRequest","database":directory.path().join("source.sqlite"),
            "kind":kind,"entry":message,"percent":0,"defaultReviewer":true,
        }),
    );
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let database = JsString::from(directory.path().join("native.sqlite").to_str().unwrap());
    let id = post_supervisor_request(
        &database,
        &PostSupervisorRequestInput {
            sender_session_id: "main".into(),
            project_id: "pi".into(),
            kind,
            payload: Default::default(),
            deadline_at: "2099-01-01T00:00:00.000Z".into(),
        },
    )
    .unwrap();
    let request = claim_next_supervisor_request(&database, &"runtime".into())
        .unwrap()
        .unwrap();
    let session = Arc::new(PolicySession {
        branch: Mutex::new(Vec::new()),
        entry: pi_js_compat::js_json::from_value(&message).unwrap(),
        calls: Arc::new(Mutex::new(Vec::new())),
        feedback: Arc::new(Mutex::new(Vec::new())),
        percent: 0.0,
        database: database.clone(),
        request_id: id,
        feedback_error: false,
    });
    process_supervisor_request(&database, request, session.clone(), None)
        .await
        .unwrap();
    let stored = read_supervisor_request(&database, id).unwrap().unwrap();
    let actual = json!({"status":stored.status,"response":stored.response,"feedback":*session.feedback.lock().unwrap(),
        "calls":*session.calls.lock().unwrap(),"reviewed":[],"processError":null,
    });
    assert_eq!(actual["response"], expected["output"]["response"]);
    assert_eq!(actual, expected["output"]);
}
#[tokio::test(start_paused = true)]
async fn default_veto_suppresses_non_generic_goal_instructions() {
    compare_default_case(false).await;
}
#[tokio::test(start_paused = true)]
async fn end_turn_without_a_stopped_response_uses_frozen_error() {
    compare_default_case(true).await;
}
