#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::request::BoxFuture;
use pi_coding_agent::{
    core::{
        extensions::types::CustomMessagePayload,
        session_control_db::{
            PostSupervisorRequestInput, SupervisorRequestKind, cancel_supervisor_request,
            claim_next_supervisor_request, post_supervisor_request, read_supervisor_request, with_control_db,
        },
        session_manager::SessionEntry,
    },
    supervisor::main::{
        SupervisorInstructionReviewer, SupervisorSession, create_supervisor_instruction_reviewer,
        process_supervisor_request,
    },
};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue},
};
use std::sync::{Arc, Mutex};

struct PolicySession {
    branch: Arc<Mutex<Vec<SessionEntry>>>,
    entry: SessionEntry,
    calls: Arc<Mutex<Vec<JsValue>>>,
    feedback: Arc<Mutex<Vec<JsValue>>>,
    percent: f64,
    database: JsString,
    request_id: i64,
    feedback_error: bool,
    cancel_prompt: bool,
    release_prompt: Arc<tokio::sync::Notify>,
}
impl SupervisorSession for PolicySession {
    fn send_custom_message(&self, message: CustomMessagePayload) -> Option<BoxFuture<'static, Result<(), JsString>>> {
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
        let release = self.release_prompt.clone();
        let cancel = self.cancel_prompt;
        Box::pin(async move {
            if cancel {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                release.notify_one();
            }
            Ok(())
        })
    }
    fn prompt(&self, content: JsString) -> BoxFuture<'static, Result<(), JsString>> {
        self.calls
            .lock()
            .unwrap()
            .push(json!({"kind":"prompt","content":content}));
        let cancel = self.cancel_prompt;
        if cancel {
            assert!(
                cancel_supervisor_request(&self.database, self.request_id, &"main".into(), "fixture cancelled")
                    .unwrap()
            );
        }
        let branch = self.branch.clone();
        let entry = self.entry.clone();
        let calls = self.calls.clone();
        let release = self.release_prompt.clone();
        Box::pin(async move {
            if cancel {
                release.notified().await;
                calls.lock().unwrap().push(json!({"kind":"prompt_cleanup"}));
            }
            branch.lock().unwrap().push(entry);
            Ok(())
        })
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
struct PolicyCase {
    kind: SupervisorRequestKind,
    response: JsValue,
    end_turn: bool,
    percent: f64,
    decision: Option<Result<JsValue, JsString>>,
    feedback_error: bool,
    block_completion: bool,
    cancel_review: bool,
    cancel_prompt: bool,
}
impl Default for PolicyCase {
    fn default() -> Self {
        Self {
            kind: SupervisorRequestKind::GoalIdleReview,
            response: json!({"kind":"continue","instructions":"Inspect evidence.","reason":"bounded"}),
            end_turn: false,
            percent: 0.0,
            decision: None,
            feedback_error: false,
            block_completion: false,
            cancel_review: false,
            cancel_prompt: false,
        }
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
fn create_fixture_reviewer(
    case: &PolicyCase,
    database: &JsString,
    request_id: i64,
    reviewed: Arc<Mutex<Vec<JsString>>>,
) -> Option<SupervisorInstructionReviewer> {
    let decision = case.decision.clone()?;
    let database = database.clone();
    let cancel = case.cancel_review;
    Some(create_supervisor_instruction_reviewer(Arc::new(
        move |instructions, signal| {
            reviewed.lock().unwrap().push(instructions);
            let database = database.clone();
            let decision = decision.clone();
            Box::pin(async move {
                if cancel {
                    assert!(
                        cancel_supervisor_request(&database, request_id, &"main".into(), "fixture cancelled").unwrap()
                    );
                    signal.wait().await;
                    return Err("review cancelled".into());
                }
                decision.map(Some)
            })
        },
    )))
}
async fn compare_case(case: PolicyCase) {
    let directory = tempfile::tempdir().unwrap();
    let message = entry(&case.response, case.end_turn);
    let mut source = oracle::TsOracle::spawn(
        "coding-agent-supervisor-main-oracle.ts",
        "packages/coding-agent/src/supervisor/main.ts",
    );
    let decision = case.decision.as_ref().and_then(|value| value.as_ref().ok());
    let expected = source.request(&json!({"id":1,"operation":"processRequest",
        "database":directory.path().join("source.sqlite"),"kind":case.kind,"entry":message,
        "percent":case.percent,"defaultReviewer":case.decision.is_none(),"decision":decision,
        "reviewError":case.decision.as_ref().is_some_and(Result::is_err),
        "feedbackError":case.feedback_error,"blockCompletion":case.block_completion,"cancelReview":case.cancel_review,"cancelPrompt":case.cancel_prompt,
    }));
    assert_eq!(expected["ok"], json!(true), "{expected:?}");
    let database = JsString::from(directory.path().join("native.sqlite").to_str().unwrap());
    let id = post_supervisor_request(
        &database,
        &PostSupervisorRequestInput {
            sender_session_id: "main".into(),
            project_id: "pi".into(),
            kind: case.kind,
            payload: Default::default(),
            deadline_at: "2099-01-01T00:00:00.000Z".into(),
        },
    )
    .unwrap();
    let request = claim_next_supervisor_request(&database, &"runtime".into())
        .unwrap()
        .unwrap();
    if case.block_completion {
        with_control_db(&database, |connection| {
            connection.execute_batch("CREATE TRIGGER block_complete BEFORE UPDATE OF status ON supervisor_requests WHEN NEW.status = 'completed' BEGIN SELECT RAISE(ABORT, 'blocked completion'); END")?;
            Ok(())
        }).unwrap();
    }
    let session = Arc::new(PolicySession {
        branch: Arc::new(Mutex::new(Vec::new())),
        entry: pi_js_compat::js_json::from_value(&message).unwrap(),
        calls: Arc::new(Mutex::new(Vec::new())),
        feedback: Arc::new(Mutex::new(Vec::new())),
        percent: case.percent,
        database: database.clone(),
        request_id: id,
        feedback_error: case.feedback_error,
        cancel_prompt: case.cancel_prompt,
        release_prompt: Arc::new(tokio::sync::Notify::new()),
    });
    let reviewed = Arc::new(Mutex::new(Vec::new()));
    let reviewer = create_fixture_reviewer(&case, &database, id, reviewed.clone());
    let error = process_supervisor_request(&database, request, session.clone(), reviewer)
        .await
        .err()
        .map(|error| error.message().clone());
    let stored = read_supervisor_request(&database, id).unwrap().unwrap();
    let actual = json!({"status":stored.status,"response":stored.response,"feedback":*session.feedback.lock().unwrap(),
        "calls":*session.calls.lock().unwrap(),"reviewed":*reviewed.lock().unwrap(),"processError":error,
    });
    assert_eq!(actual["response"], expected["output"]["response"]);
    assert_eq!(actual, expected["output"]);
}
#[tokio::test(start_paused = true)]
async fn default_veto_suppresses_non_generic_goal_instructions() {
    compare_case(PolicyCase::default()).await;
}
#[tokio::test(start_paused = true)]
async fn end_turn_without_a_stopped_response_uses_frozen_error() {
    compare_case(PolicyCase {
        kind: SupervisorRequestKind::ApprovalReview,
        end_turn: true,
        ..Default::default()
    })
    .await;
}
#[tokio::test(start_paused = true)]
async fn goal_review_decisions_and_feedback_match_frozen() {
    for decision in [
        "ACCEPT",
        "REJECT_TASK_ASSIGNMENT",
        "REJECT_IMPLEMENTATION_PRESCRIPTION",
        "REJECT_SEQUENCING_INSTRUCTION",
        "REJECT_AGENT_OR_TOOL_DIRECTION",
        "REJECT_PLAN_OVERRIDE",
        " ACCEPT",
    ] {
        compare_case(PolicyCase {
            decision: Some(Ok(json!(decision))),
            ..Default::default()
        })
        .await;
    }
    compare_case(PolicyCase {
        decision: Some(Err("review failed".into())),
        ..Default::default()
    })
    .await;
    compare_case(PolicyCase {
        feedback_error: true,
        ..Default::default()
    })
    .await;
}
#[tokio::test(start_paused = true)]
async fn generic_goal_and_compaction_threshold_match_frozen() {
    for percent in [74.0, 75.0] {
        compare_case(PolicyCase {percent,decision:Some(Ok(json!("ACCEPT"))),
            kind:SupervisorRequestKind::GoalCompletionReview,
            response:json!({"kind":"continue","instructions":"Continue working toward the active goal.","reason":"bounded"}),
            ..Default::default()
        }).await;
    }
}
#[tokio::test(start_paused = true)]
async fn completion_errors_propagate_like_frozen() {
    compare_case(PolicyCase {
        block_completion: true,
        decision: Some(Ok(json!("ACCEPT"))),
        ..Default::default()
    })
    .await;
}
#[tokio::test(start_paused = true)]
async fn cancellation_during_prompt_awaits_abort_cleanup_like_frozen() {
    compare_case(PolicyCase {
        cancel_prompt: true,
        ..Default::default()
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn cancellation_during_instruction_review_matches_frozen() {
    compare_case(PolicyCase {
        cancel_review: true,
        decision: Some(Ok(json!("ACCEPT"))),
        ..Default::default()
    })
    .await;
}
