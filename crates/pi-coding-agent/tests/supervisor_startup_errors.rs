#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::{SupervisorRequestKind, SupervisorResponse};
use pi_coding_agent::supervisor::client::{
    RequestSupervisorDecisionInput, SupervisorClientDependencies, request_supervisor_decision_with_dependencies,
};
use pi_coding_agent::supervisor::ensure_running::{
    EnsureSupervisorRunningDependencies, EnsureSupervisorRunningInput, ensure_supervisor_running,
};
use pi_js_compat::js_json::JsValue;
use pi_js_compat::{js_json as json, node_error::NativeErrorClass};
use std::sync::Arc;

#[tokio::test]
async fn startup_system_error_is_preserved_and_client_uses_only_its_message() {
    let supplied = pi_js_compat::node_os::homedir_system_error(-2, "ENOENT", "no such file or directory");
    let mut oracle = oracle::TsOracle::spawn(
        "supervisor-ensure-running-oracle.ts",
        "packages/coding-agent/src/supervisor/ensure-running.ts",
    );
    let expected = oracle.request(&json!({"id":1,"op":"probeError","error":supplied.value()}));
    assert_eq!(expected["ok"], JsValue::Bool(true));
    let probe_error = supplied.clone();
    let result = ensure_supervisor_running(
        &EnsureSupervisorRunningInput::new("/unused".into()),
        &EnsureSupervisorRunningDependencies {
            probe: Some(Arc::new(move |_| {
                let error = probe_error.clone();
                Box::pin(async move { Err(error) })
            })),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert_eq!(result, supplied);
    assert_eq!(result.constructor_class(), NativeErrorClass::SystemError);
    assert_eq!(result.value(), expected["output"]);
    let startup_error = supplied.clone();
    let input = RequestSupervisorDecisionInput {
        control_db_path: "/unused".into(),
        kind: SupervisorRequestKind::SupervisorAdvisory,
        payload: Default::default(),
        project_id: "pi".into(),
        sender_session_id: "main".into(),
        timeout_ms: 1.0,
        signal: None,
        max_attempts: None,
        poll_interval_ms: None,
        retry_delay_ms: None,
        retry_jitter_ratio: None,
    };
    let response = request_supervisor_decision_with_dependencies(
        &input,
        &SupervisorClientDependencies {
            ensure_running: Some(Arc::new(move |_| {
                let error = startup_error.clone();
                Box::pin(async move { Err(error) })
            })),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        response,
        SupervisorResponse::Error {
            reason: pi_js_compat::js_format!("Supervisor startup failed: {}", supplied.message)
        }
    );
}
