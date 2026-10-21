//! Port of `packages/coding-agent/test/supervisor-client.test.ts`.

use pi_coding_agent::core::session_control_db::{
    SupervisorRequestKind, SupervisorResponse, claim_next_supervisor_request, complete_supervisor_request,
    get_control_db_path, read_supervisor_request, recover_supervisor_requests,
};
use pi_coding_agent::supervisor::client::{
    RequestSupervisorDecisionInput, SupervisorClientDependencies, request_supervisor_decision_with_dependencies,
};
use serde_json::{Map, Value, json};
use tempfile::TempDir;

fn payload(value: Value) -> Map<String, Value> {
    value.as_object().expect("payload object").clone()
}

fn input(control_db_path: &str, kind: SupervisorRequestKind) -> RequestSupervisorDecisionInput {
    RequestSupervisorDecisionInput {
        control_db_path: control_db_path.to_owned(),
        kind,
        payload: payload(json!({ "objective": "finish" })),
        project_id: "pi".to_owned(),
        sender_session_id: "main".to_owned(),
        timeout_ms: 1_000,
        max_attempts: None,
        poll_interval_ms: Some(1),
        retry_delay_ms: None,
        retry_jitter_ratio: None,
        signal: None,
    }
}

#[tokio::test]
async fn waits_for_and_returns_the_durable_supervisor_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(temp_dir.path());
    let mut decision_input = input(&control_db_path, SupervisorRequestKind::ApprovalReview);
    decision_input.payload = payload(json!({ "toolName": "read" }));

    let responder_path = control_db_path.clone();
    let responder = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let request = claim_next_supervisor_request(&responder_path, "runtime")
            .expect("claim request")
            .expect("request record");
        complete_supervisor_request(
            &responder_path,
            request.id,
            "runtime",
            &SupervisorResponse::Approve {
                reason: "bounded".to_owned(),
            },
        )
        .expect("complete request");
    });

    let decision = request_supervisor_decision_with_dependencies(&decision_input, &fake_dependencies())
        .await
        .expect("decision");
    responder.await.expect("responder");
    assert_eq!(
        decision,
        SupervisorResponse::Approve {
            reason: "bounded".to_owned()
        }
    );
}

#[tokio::test]
async fn retries_a_timed_out_request_before_returning_a_later_durable_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(temp_dir.path());
    let mut decision_input = input(&control_db_path, SupervisorRequestKind::GoalIdleReview);
    decision_input.timeout_ms = 500;
    decision_input.max_attempts = Some(2);
    decision_input.retry_delay_ms = Some(1.0);
    decision_input.retry_jitter_ratio = Some(0.0);

    let responder_path = control_db_path.clone();
    let responder = tokio::spawn(async move {
        for _ in 0..2000 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            // The first attempt must expire before any responder claims it, so only
            // answer the retried request record.
            if read_supervisor_request(&responder_path, 2)
                .expect("read retried request")
                .is_none()
            {
                continue;
            }
            recover_supervisor_requests(&responder_path).expect("recover requests");
            let Some(request) = claim_next_supervisor_request(&responder_path, "runtime").expect("claim request")
            else {
                continue;
            };
            complete_supervisor_request(
                &responder_path,
                request.id,
                "runtime",
                &SupervisorResponse::Continue {
                    instructions: "Continue the goal.".to_owned(),
                    reason: "recovered".to_owned(),
                },
            )
            .expect("complete request");
            return;
        }
        panic!("retried request never became claimable");
    });

    let decision = request_supervisor_decision_with_dependencies(&decision_input, &fake_dependencies())
        .await
        .expect("decision");
    responder.await.expect("responder");
    assert_eq!(
        decision,
        SupervisorResponse::Continue {
            instructions: "Continue the goal.".to_owned(),
            reason: "recovered".to_owned(),
        }
    );
}

#[tokio::test]
async fn returns_error_when_all_request_attempts_expire_without_a_service_response() {
    let temp_dir = TempDir::new().expect("temp dir");
    let control_db_path = get_control_db_path(temp_dir.path());
    let mut decision_input = input(&control_db_path, SupervisorRequestKind::GoalIdleReview);
    decision_input.timeout_ms = 20;
    decision_input.max_attempts = Some(2);
    decision_input.retry_delay_ms = Some(1.0);
    decision_input.retry_jitter_ratio = Some(0.0);

    let decision = request_supervisor_decision_with_dependencies(&decision_input, &fake_dependencies())
        .await
        .expect("decision");
    assert_eq!(
        decision,
        SupervisorResponse::Error {
            reason: "Supervisor request timed out after 2 attempts".to_owned()
        }
    );
}

fn fake_dependencies() -> SupervisorClientDependencies {
    SupervisorClientDependencies {
        ensure_running: std::sync::Arc::new(|_| Box::pin(async { Ok(()) })),
    }
}

#[tokio::test]
async fn startup_failure_and_preaborted_requests_never_post() {
    let directory = TempDir::new().unwrap();
    let path = get_control_db_path(directory.path());
    let mut request = input(&path, SupervisorRequestKind::ApprovalReview);
    let deps = SupervisorClientDependencies {
        ensure_running: std::sync::Arc::new(|_| Box::pin(async { Err("launch failed".into()) })),
    };
    let result = request_supervisor_decision_with_dependencies(&request, &deps)
        .await
        .unwrap();
    assert_eq!(
        result,
        SupervisorResponse::Error {
            reason: "Supervisor startup failed: launch failed".into()
        }
    );
    assert!(!std::path::Path::new(&path).exists());
    let token = tokio_util::sync::CancellationToken::new();
    token.cancel();
    request.signal = Some(token);
    let result = request_supervisor_decision_with_dependencies(&request, &deps)
        .await
        .unwrap();
    assert_eq!(
        result,
        SupervisorResponse::Error {
            reason: "Supervisor request cancelled".into()
        }
    );
    assert!(!std::path::Path::new(&path).exists());
}

#[tokio::test]
async fn abort_during_poll_cancels_durable_claim() {
    let directory = TempDir::new().unwrap();
    let path = get_control_db_path(directory.path());
    let mut request = input(&path, SupervisorRequestKind::ApprovalReview);
    request.poll_interval_ms = Some(5000);
    let token = tokio_util::sync::CancellationToken::new();
    request.signal = Some(token.clone());
    let worker_path = path.clone();
    let worker = tokio::spawn(async move {
        while read_supervisor_request(&worker_path, 1).unwrap().is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        claim_next_supervisor_request(&worker_path, "owner").unwrap().unwrap();
        token.cancel();
    });
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        request_supervisor_decision_with_dependencies(&request, &fake_dependencies()),
    )
    .await
    .unwrap()
    .unwrap();
    worker.await.unwrap();
    assert_eq!(
        response,
        SupervisorResponse::Error {
            reason: "Supervisor request cancelled".into()
        }
    );
    let row = read_supervisor_request(&path, 1).unwrap().unwrap();
    assert_eq!(
        row.status,
        pi_coding_agent::core::session_control_db::SupervisorRequestStatus::Cancelled
    );
    assert_eq!(row.claim_token, None);
}
