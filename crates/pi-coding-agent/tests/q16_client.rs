#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::{
    SupervisorRequestKind, SupervisorResponse, claim_next_supervisor_request, complete_supervisor_request,
    read_supervisor_request,
};
use pi_coding_agent::supervisor::client::{
    RequestSupervisorDecisionInput, SupervisorClientDependencies, request_supervisor_decision_with_dependencies,
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
#[tokio::test]
async fn startup_cancellation_retry_and_completion_match_frozen_rows() {
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/supervisor/client.ts",
    );
    for (index, scenario) in [
        "preabort",
        "startup-error",
        "startup-abort",
        "pollabort",
        "complete",
        "retryabort",
        "timeout",
        "zero",
    ]
    .iter()
    .enumerate()
    {
        let rustdir = tempfile::tempdir().unwrap();
        let tsdir = tempfile::tempdir().unwrap();
        let path = rustdir.path().join("control.sqlite").display().to_string();
        let expected=ts.request(&json!({"id":index,"operation":"clientScenarios","cases":[{"scenario":scenario,"db":tsdir.path().join("control.sqlite")}]}));
        assert_eq!(expected["ok"], true, "{expected}");
        let token = tokio_util::sync::CancellationToken::new();
        if *scenario == "preabort" {
            token.cancel();
        }
        let request = RequestSupervisorDecisionInput {
            control_db_path: path.clone(),
            kind: SupervisorRequestKind::ApprovalReview,
            payload: serde_json::from_value(json!({"toolName":"read"})).unwrap(),
            project_id: "pi".into(),
            sender_session_id: "main".into(),
            timeout_ms: if ["pollabort", "complete"].contains(scenario) {
                10000
            } else {
                0
            },
            max_attempts: Some(if *scenario == "zero" {
                0
            } else if ["retryabort", "timeout"].contains(scenario) {
                2
            } else {
                1
            }),
            poll_interval_ms: Some(if *scenario == "pollabort" { 5000 } else { 1 }),
            retry_delay_ms: Some(if *scenario == "retryabort" { 100000.0 } else { 0.0 }),
            retry_jitter_ratio: Some(0.0),
            signal: Some(token.clone()),
        };
        let starts = Arc::new(AtomicUsize::new(0));
        let calls = starts.clone();
        let scenario = scenario.to_string();
        let dependencies = SupervisorClientDependencies {
            ensure_running: Arc::new(move |db| {
                calls.fetch_add(1, Ordering::SeqCst);
                let scenario = scenario.clone();
                let token = token.clone();
                Box::pin(async move {
                    if scenario == "startup-error" {
                        return Err("fake startup error".into());
                    }
                    if scenario == "startup-abort" {
                        token.cancel();
                    }
                    if scenario == "pollabort" || scenario == "retryabort" {
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                            token.cancel();
                        });
                    }
                    if scenario == "complete" {
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                            let row = claim_next_supervisor_request(&db, "fake-owner").unwrap().unwrap();
                            complete_supervisor_request(
                                &db,
                                row.id,
                                "fake-owner",
                                &SupervisorResponse::Approve {
                                    reason: "fake approval".into(),
                                },
                            )
                            .unwrap();
                        });
                    }
                    Ok(())
                })
            }),
        };
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            request_supervisor_decision_with_dependencies(&request, &dependencies),
        )
        .await
        .unwrap()
        .unwrap();
        let mut rows = Vec::new();
        if std::path::Path::new(&path).exists() {
            for id in 1..10 {
                let Some(row) = read_supervisor_request(&path, id).unwrap() else {
                    break;
                };
                rows.push(json!({"id":row.id,"kind":row.kind,"status":row.status,"response":row.response}));
            }
        }
        assert_eq!(
            json!({"response":response,"starts":starts.load(Ordering::SeqCst),"rows":rows}),
            expected["output"][0],
            "case {index}"
        );
    }
}
