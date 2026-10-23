use pi_coding_agent::core::session_control_db::{
    SupervisorRequestKind, SupervisorRequestStatus, SupervisorResponse, cancel_supervisor_request,
    claim_next_supervisor_request, get_control_db_path, read_supervisor_request,
};
use pi_coding_agent::supervisor::client::{
    RequestSupervisorDecisionInput, SupervisorClientDependencies, request_supervisor_decision_with_dependencies,
};
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

#[tokio::test]
async fn claimed_request_abort_clears_claim_and_external_cancel_returns_its_reason() {
    for external in [false, true] {
        let temp = tempfile::TempDir::new().unwrap();
        let path = get_control_db_path(temp.path());
        let controller = AbortController::new();
        let input = RequestSupervisorDecisionInput {
            control_db_path: path.clone().into(),
            kind: SupervisorRequestKind::SupervisorAdvisory,
            payload: Default::default(),
            project_id: "pi".into(),
            sender_session_id: "main".into(),
            timeout_ms: 1000.0,
            signal: Some(controller.signal()),
            max_attempts: None,
            poll_interval_ms: Some(1.0),
            retry_delay_ms: None,
            retry_jitter_ratio: None,
        };
        let request = tokio::spawn(async move {
            request_supervisor_decision_with_dependencies(
                &input,
                &SupervisorClientDependencies {
                    ensure_running: Some(Arc::new(|_| Box::pin(async { Ok(()) }))),
                },
            )
            .await
        });
        let claimed = loop {
            if let Some(claimed) = claim_next_supervisor_request(&path, &JsString::from("worker")).unwrap() {
                break claimed;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        };
        assert_eq!(claimed.status, SupervisorRequestStatus::Claimed);
        if external {
            assert!(cancel_supervisor_request(&path, claimed.id, &"main".into(), "external cancellation").unwrap());
        } else {
            controller.abort(None);
        }
        let cancelled = read_supervisor_request(&path, claimed.id).unwrap().unwrap();
        assert_eq!(cancelled.status, SupervisorRequestStatus::Cancelled);
        assert!(cancelled.claim_token.is_none());
        let response = request.await.unwrap().unwrap();
        assert_eq!(
            response,
            SupervisorResponse::Error {
                reason: if external {
                    "external cancellation".into()
                } else {
                    "Supervisor request cancelled".into()
                }
            }
        );
    }
}
