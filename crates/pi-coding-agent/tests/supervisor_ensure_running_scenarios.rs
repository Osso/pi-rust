use crate::shared_oracle as oracle;
use pi_coding_agent::core::resident_console_transport::{ResidentConsoleIdentity, ResidentConsoleManagedBy};
use pi_coding_agent::supervisor::ensure_running::*;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
fn identity(version: impl Into<JsString>) -> ResidentConsoleIdentity {
    ResidentConsoleIdentity {
        version: version.into(),
        pid: 1234.0,
        executable: "/opt/pi".into(),
        entrypoint: None,
        instance_id: "instance".into(),
        managed_by: ResidentConsoleManagedBy::Pi,
        ready: true,
    }
}
#[tokio::test]
async fn startup_branch_results_match_frozen_and_release_locks() {
    let mut oracle = oracle::TsOracle::spawn(
        "supervisor-ensure-running-oracle.ts",
        "packages/coding-agent/src/supervisor/ensure-running.ts",
    );
    let version = pi_coding_agent::config::version();
    let mut external = identity("old");
    external.managed_by = ResidentConsoleManagedBy::External;
    let scenarios = [
        json!({"initial":identity(version)}),
        json!({"afterLaunch":identity(version)}),
        json!({"initial":identity("old"),"afterLaunch":identity(version)}),
        json!({"initial":external}),
        json!({"launchError":"spawn failed"}),
        json!({}),
        json!({"initial":identity("old"),"keepOld":true}),
    ];
    for (index, case) in scenarios.into_iter().enumerate() {
        let temp = tempfile::TempDir::new().unwrap();
        let path: JsString = temp.path().join("control.sqlite").to_str().unwrap().into();
        let input = EnsureSupervisorRunningInput {
            control_db_path: path.clone(),
            launch_invocation: Some(SupervisorLaunchInvocation {
                command: "unused".into(),
                args: vec![],
                cwd: "/tmp".into(),
                env: Default::default(),
            }),
            poll_interval_ms: Some(1.0),
            startup_timeout_ms: Some(5.0),
        };
        let resident = Arc::new(Mutex::new(
            case.get("initial")
                .map(|value| js_json::from_value::<ResidentConsoleIdentity>(value).unwrap()),
        ));
        let launches = Arc::new(AtomicUsize::new(0));
        let terminations = Arc::new(AtomicUsize::new(0));
        let probe_resident = resident.clone();
        let launch_resident = resident.clone();
        let stop_resident = resident.clone();
        let launch_count = launches.clone();
        let stop_count = terminations.clone();
        let launch_case = case.clone();
        let stop_case = case.clone();
        let dependencies = EnsureSupervisorRunningDependencies {
            probe: Some(Arc::new(move |_| {
                let resident = probe_resident.clone();
                Box::pin(async move { Ok(resident.lock().unwrap().clone()) })
            })),
            launch: Some(Arc::new(move |_| {
                let resident = launch_resident.clone();
                let count = launch_count.clone();
                let case = launch_case.clone();
                Box::pin(async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    if let Some(error) = case.get("launchError").and_then(JsValue::as_string) {
                        return Err(pi_js_compat::node_error::NativeNodeError::new("Error", error.clone()));
                    }
                    *resident.lock().unwrap() =
                        case.get("afterLaunch").map(|value| js_json::from_value(value).unwrap());
                    Ok(())
                })
            })),
            terminate: Some(Arc::new(move |_| {
                let resident = stop_resident.clone();
                let count = stop_count.clone();
                let case = stop_case.clone();
                Box::pin(async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    if case["keepOld"] != JsValue::Bool(true) {
                        *resident.lock().unwrap() = None;
                    }
                    Ok(())
                })
            })),
        };
        let mut request = case.clone();
        request["id"] = json!(index);
        request["op"] = json!("scenario");
        request["input"] = json!({"controlDbPath":path,"pollIntervalMs":1,"startupTimeoutMs":5,"launchInvocation":js_json::to_value(input.launch_invocation.as_ref().unwrap()).unwrap()});
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let result = ensure_supervisor_running(&input, &dependencies).await;
        let mut actual =
            json!({"launches":launches.load(Ordering::SeqCst),"terminations":terminations.load(Ordering::SeqCst)});
        match result {
            Ok(result) => actual["result"] = js_json::to_value(&result).unwrap(),
            Err(error) => actual["error"] = JsValue::String(error.message),
        }
        assert_eq!(actual, expected["output"], "scenario {index}");
        assert!(!temp.path().join("control.sqlite.supervisor-start.lock").exists());
    }
}
