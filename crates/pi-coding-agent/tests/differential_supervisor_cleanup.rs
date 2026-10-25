use super::oracle;
use pi_ai::{request::BoxFuture, session_resources::register_session_resource_cleanup};
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
struct CleanupSession(Arc<Mutex<Vec<JsValue>>>);
impl SupervisorSession for CleanupSession {
    fn abort(&self) -> BoxFuture<'static, Result<(), JsString>> {
        self.0.lock().unwrap().push(json!({"kind":"abort"}));
        Box::pin(async { Ok(()) })
    }
    fn prompt(&self, content: JsString) -> BoxFuture<'static, Result<(), JsString>> {
        self.0.lock().unwrap().push(json!({"kind":"prompt","content":content}));
        Box::pin(async { Err("unexpected prompt".into()) })
    }
    fn compact(&self, content: &JsString) -> Option<BoxFuture<'static, Result<(), JsString>>> {
        self.0.lock().unwrap().push(json!({"kind":"compact","content":content}));
        Some(Box::pin(async { Ok(()) }))
    }
    fn context_percent(&self) -> Option<f64> {
        Some(75.0)
    }
    fn session_id(&self) -> Option<JsString> {
        Some("resident-fixture".into())
    }
    fn get_branch(&self) -> Vec<SessionEntry> {
        Vec::new()
    }
    fn get_leaf_id(&self) -> Option<JsString> {
        None
    }
}
#[tokio::test(start_paused = true)]
async fn provider_cleanup_failure_stops_before_prompt_and_completes_error_like_frozen() {
    let directory = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn(
        "coding-agent-supervisor-main-oracle.ts",
        "packages/coding-agent/src/supervisor/main.ts",
    );
    let expected = source.request(
        &json!({"id":1,"operation":"processRequest","database":directory.path().join("source.sqlite"),
        "kind":"goal_idle_review","entry":null,"percent":75,"defaultReviewer":true,"cleanupError":true}),
    );
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    assert_eq!(
        expected["output"]["response"],
        json!({"kind":"error","reason":"Failed to cleanup session resources"})
    );
    let database = JsString::from(directory.path().join("native.sqlite").to_str().unwrap());
    let id = post_supervisor_request(
        &database,
        &PostSupervisorRequestInput {
            sender_session_id: "main".into(),
            project_id: "pi".into(),
            kind: SupervisorRequestKind::GoalIdleReview,
            payload: Default::default(),
            deadline_at: "2099-01-01T00:00:00.000Z".into(),
        },
    )
    .unwrap();
    let request = claim_next_supervisor_request(&database, &"runtime".into())
        .unwrap()
        .unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let _registration = register_session_resource_cleanup(Box::new(|session_id| {
        if session_id == Some(&JsString::from("resident-fixture")) {
            Err(Box::new(std::io::Error::other("fixture cleanup failed")))
        } else {
            Ok(())
        }
    }));
    process_supervisor_request(&database, request, Arc::new(CleanupSession(calls.clone())), None)
        .await
        .unwrap();
    let stored = read_supervisor_request(&database, id).unwrap().unwrap();
    let actual = json!({"status":stored.status,"response":stored.response,"feedback":[],"calls":*calls.lock().unwrap(),"reviewed":[],"processError":null});
    assert_eq!(actual, expected["output"]);
}
