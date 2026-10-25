use crate::shared_oracle as oracle;
use pi_coding_agent::core::session_control_db::{SupervisorRequestKind, read_supervisor_request};
use pi_coding_agent::supervisor::client::*;
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn startup_cancel_retry_timeout_and_numeric_boundaries_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn(
        "supervisor-client-oracle.ts",
        "packages/coding-agent/src/supervisor/client.ts",
    );
    let cases = [
        json!({"preAbort":true}),
        json!({"abortStartup":true}),
        json!({"startupError":"cannot launch"}),
        json!({"abortStartup":true,"startupError":"cannot launch"}),
        json!({"timeout":0}),
        json!({"timeout":-1,"attempts":2}),
        json!({"timeout":0,"attempts":0}),
        json!({"timeout":0,"attempts":0.5}),
        json!({"timeout":0,"attempts":2.5}),
        json!({"timeout":0,"attempts":-1}),
        json!({"timeoutTag":"NaN"}),
        json!({"timeoutTag":"Infinity"}),
        json!({"abortLater":true,"timeout":1000,"poll":10000}),
        json!({"abortLater":true,"timeout":0,"attempts":2,"retry":10000}),
    ];
    for (index, case) in cases.into_iter().enumerate() {
        let temp = tempfile::TempDir::new().unwrap();
        let source_path = temp.path().join("source.db").to_str().unwrap().to_owned();
        let native_path: JsString = temp.path().join("native.db").to_str().unwrap().into();
        let timeout = case["timeout"].as_f64().unwrap_or(1000.0);
        let attempts = case["attempts"].as_f64();
        let poll = case["poll"].as_f64();
        let retry = case["retry"].as_f64().unwrap_or(1.0);
        let mut wire_input = json!({"controlDbPath":source_path,"kind":"supervisor_advisory","payload":{"question":"evidence"},"projectId":"pi","senderSessionId":"main","timeoutMs":timeout,"retryDelayMs":retry,"retryJitterRatio":0});
        if let Some(attempts) = attempts {
            wire_input["maxAttempts"] = json!(attempts);
        }
        if let Some(poll) = poll {
            wire_input["pollIntervalMs"] = json!(poll);
        }
        let mut wire = case.clone();
        wire["input"] = wire_input;
        wire["id"] = json!(index);
        let expected = oracle.request(&wire);
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let controller = AbortController::new();
        if case["preAbort"] == JsValue::Bool(true) {
            controller.abort(None);
        }
        let startups = Arc::new(AtomicUsize::new(0));
        let count = startups.clone();
        let startup_case = case.clone();
        let startup_controller = controller.clone();
        let dependencies = SupervisorClientDependencies {
            ensure_running: Some(Arc::new(move |_| {
                count.fetch_add(1, Ordering::SeqCst);
                let case = startup_case.clone();
                let controller = startup_controller.clone();
                Box::pin(async move {
                    if case["abortStartup"] == JsValue::Bool(true) {
                        controller.abort(None);
                    }
                    if let Some(error) = case["startupError"].as_string() {
                        return Err(pi_js_compat::node_error::NativeNodeError::new("Error", error.clone()));
                    }
                    Ok(())
                })
            })),
        };
        let timer = if case["abortLater"] == JsValue::Bool(true) {
            let controller = controller.clone();
            Some(tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                controller.abort(None);
            }))
        } else {
            None
        };
        let timeout = match case["timeoutTag"]
            .as_string()
            .and_then(|tag| tag.to_utf8().ok())
            .as_deref()
        {
            Some("NaN") => f64::NAN,
            Some("Infinity") => f64::INFINITY,
            _ => timeout,
        };
        let input = RequestSupervisorDecisionInput {
            control_db_path: native_path.clone(),
            kind: SupervisorRequestKind::SupervisorAdvisory,
            payload: json!({"question":"evidence"}).as_object().unwrap().clone(),
            project_id: "pi".into(),
            sender_session_id: "main".into(),
            timeout_ms: timeout,
            signal: Some(controller.signal()),
            max_attempts: attempts,
            poll_interval_ms: poll,
            retry_delay_ms: Some(retry),
            retry_jitter_ratio: Some(0.0),
        };
        let outcome = request_supervisor_decision_with_dependencies(&input, &dependencies).await;
        if let Some(timer) = timer {
            timer.abort();
        }
        let mut actual = json!({"startups":startups.load(Ordering::SeqCst)});
        match outcome {
            Ok(result) => actual["result"] = js_json::to_value(&result).unwrap(),
            Err(error) => actual["error"] = json!({"name":error.name,"message":error.message}),
        }
        let mut requests = Vec::new();
        for id in 1.. {
            let Some(row) = read_supervisor_request(&native_path, id).unwrap() else {
                break;
            };
            let mut value = json!({"status":row.status});
            if let Some(response) = row.response {
                value["response"] = js_json::to_value(&response).unwrap();
            }
            requests.push(value);
        }
        actual["requests"] = JsValue::Array(requests);
        assert_eq!(actual, expected["output"], "case {index}");
    }
}
