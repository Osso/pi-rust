//! Real SQLite + evaluator-lifetime differential at the frozen service boundary.
#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::request::BoxFuture;
use pi_coding_agent::{
    core::session_control_db::{
        PostSupervisorRequestInput, SupervisorRequestKind, cancel_supervisor_request, claim_next_supervisor_request,
        post_supervisor_request, read_supervisor_request,
    },
    supervisor::service::{
        RunSupervisorRequestInput, RunSupervisorRequestOutcome, SupervisorEvaluator, run_supervisor_request,
    },
};
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue},
};
use std::sync::{Arc, Mutex};

fn outcome_name(outcome: RunSupervisorRequestOutcome) -> &'static str {
    match outcome {
        RunSupervisorRequestOutcome::Completed => "completed",
        RunSupervisorRequestOutcome::Preempted => "preempted",
        RunSupervisorRequestOutcome::Cancelled => "cancelled",
    }
}
fn post(database: &JsString, kind: SupervisorRequestKind) -> i64 {
    post_supervisor_request(
        database,
        &PostSupervisorRequestInput {
            sender_session_id: "main".into(),
            project_id: "pi".into(),
            kind,
            payload: Default::default(),
            deadline_at: "2099-01-01T00:00:00.000Z".into(),
        },
    )
    .unwrap()
}
async fn native(database: JsString, scenario: &'static str) -> JsValue {
    let kind = if scenario.starts_with("preempt") {
        SupervisorRequestKind::GoalIdleReview
    } else {
        SupervisorRequestKind::ApprovalReview
    };
    let id = post(&database, kind);
    let mut request = claim_next_supervisor_request(&database, &"runtime".into())
        .unwrap()
        .unwrap();
    if scenario == "deadline_settled" {
        request.deadline_at = "2000-01-01T00:00:00.000Z".into();
    }
    let trace = Arc::new(Mutex::new(Vec::<String>::new()));
    let evaluation_trace = trace.clone();
    let evaluation_db = database.clone();
    let evaluate: SupervisorEvaluator = Arc::new(move |_prompt, signal| {
        let trace = evaluation_trace.clone();
        let database = evaluation_db.clone();
        trace.lock().unwrap().push("evaluate".into());
        let abort_trace = trace.clone();
        signal.add_listener(move |_| abort_trace.lock().unwrap().push("abort".into()));
        if scenario.starts_with("cancel") {
            assert!(cancel_supervisor_request(&database, id, &"main".into(), "fixture cancelled").unwrap());
            trace.lock().unwrap().push("cancel".into());
        }
        if scenario.starts_with("preempt") {
            post(&database, SupervisorRequestKind::ApprovalReview);
            trace.lock().unwrap().push("approval".into());
        }
        Box::pin(async move {
            if scenario == "evaluation_reject" {
                return Err("evaluation failed".into());
            }
            if scenario.ends_with("pending") {
                signal.wait().await;
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                let status = read_supervisor_request(&database, id).unwrap().unwrap().status;
                let status = pi_js_compat::js_json::to_value(&status).unwrap();
                trace
                    .lock()
                    .unwrap()
                    .push(format!("cleanup:{}", status.as_string().unwrap().to_utf8().unwrap()));
            }
            Ok(if kind == SupervisorRequestKind::ApprovalReview {
                json!({"kind":"approve","reason":"ok"})
            } else {
                json!({"kind":"continue","reason":"ok","instructions":"Continue."})
            })
        }) as BoxFuture<'static, Result<JsValue, JsString>>
    });
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        run_supervisor_request(RunSupervisorRequestInput {
            control_db_path: database.clone(),
            evaluate,
            request,
            poll_interval_ms: Some(1),
        }),
    )
    .await
    .expect("the service must settle a durably cancelled/preempted evaluation");
    let (outcome, error) = match result {
        Ok(value) => (Some(outcome_name(value)), None),
        Err(error) => (None, Some(error.message().clone())),
    };
    let stored = read_supervisor_request(&database, id).unwrap().unwrap();
    json!({"outcome":outcome,"error":error,"status":stored.status,"response":stored.response,"trace":*trace.lock().unwrap()})
}
async fn compare(scenario: &'static str) {
    let directory = tempfile::tempdir().unwrap();
    let frozen_database = directory.path().join("source.sqlite").to_str().unwrap().to_owned();
    let mut source = oracle::TsOracle::spawn(
        "coding-agent-supervisor-runtime-oracle.ts",
        "packages/coding-agent/src/supervisor/service.ts",
    );
    let expected = source.request(&json!({"id":1,"database":frozen_database,"scenario":scenario}));
    assert_eq!(expected["ok"], json!(true));
    let actual = native(
        directory.path().join("native.sqlite").to_str().unwrap().into(),
        scenario,
    )
    .await;
    assert_eq!(actual, expected["output"], "{scenario}");
}

#[tokio::test(start_paused = true)]
async fn evaluation_rejection_propagates_without_service_completion() {
    compare("evaluation_reject").await;
}
#[tokio::test(start_paused = true)]
async fn cancellation_aborts_and_awaits_pending_cleanup() {
    compare("cancel_pending").await;
}
#[tokio::test(start_paused = true)]
async fn cancellation_wins_over_an_already_settled_evaluation() {
    compare("cancel_settled").await;
}
#[tokio::test(start_paused = true)]
async fn deadline_wins_over_an_already_settled_evaluation() {
    compare("deadline_settled").await;
}
#[tokio::test(start_paused = true)]
async fn approval_preempts_an_already_settled_evaluation() {
    compare("preempt_settled").await;
}
#[tokio::test(start_paused = true)]
async fn preemption_awaits_cleanup_before_requeue() {
    compare("preempt_pending").await;
}
