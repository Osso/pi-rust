use crate::shared_oracle as oracle;
use pi_agent_core::agent::ToolError;
use pi_coding_agent::core::tools::bash::{
    BashAdopt, BashDetachOptions, BashDetachedResult, BashExecOptions, BashOperations, DetachedBashProcess,
    DetachedJobArtifacts, create_local_bash_operations,
};
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{Arc, Mutex};

// Frozen bash.ts:196-225 passes a live exit and timer to adopt; adoption remains
// in a race with foreground exit, even if its callback awaits that same exit.
#[tokio::test]
async fn local_adoption_payload_rejection_exit_race_and_timer_transfer_match_frozen() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = JsString::from(temp.path().to_str().unwrap());
    let cases = vec![
        json!({"mode":"ready","artifacts":true}),
        json!({"mode":"exit"}),
        json!({"mode":"reject","artifacts":true}),
        json!({"mode":"clear","timeout":0.02}),
    ];
    let mut source = oracle::TsOracle::spawn("bash-adopt-oracle.ts", "packages/coding-agent/src/core/tools/bash.ts");
    let expected = source.request(&json!({"id":28055,"cwd":cwd,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut failures = Vec::new();
    for (index, input) in cases.iter().enumerate() {
        let mode = input["mode"].as_string().unwrap().to_utf8().unwrap();
        let directory = temp.path().join(format!("adopt-{mode}"));
        let artifacts = if input["artifacts"] == JsValue::Bool(true) {
            std::fs::create_dir(&directory).unwrap();
            Some(DetachedJobArtifacts {
                directory: directory.to_str().unwrap().into(),
                output_path: directory.join("output.log").to_str().unwrap().into(),
            })
        } else {
            None
        };
        let calls = Arc::new(Mutex::new(Vec::new()));
        let captured = calls.clone();
        let adopt: BashAdopt = Arc::new(move |process: DetachedBashProcess| {
            captured.lock().unwrap().push(json!({"command":process.command,"cwd":process.cwd,"pidMatches":process.pid.is_some()&&process.child.pid==process.pid,"startedAtValid":process.started_at.is_finite(),"hasTimeout":process.timeout_handle.is_some(),"hasArtifacts":process.artifacts.is_some()}));
            let mode = mode.clone();
            Box::pin(async move {
                if mode == "reject" {
                    return Err(ToolError::new("adopt failed"));
                }
                if mode == "clear" {
                    process.timeout_handle.as_ref().unwrap().cancel();
                }
                if mode == "exit" || mode == "clear" {
                    process.exit.wait().await?;
                }
                Ok(BashDetachedResult {
                    job_id: mode.into(),
                    message: "adopted".into(),
                    log_path: None,
                })
            })
        });
        let controller = AbortController::new();
        controller.abort(None);
        let result = create_local_bash_operations(None)
            .exec(
                &"sleep .1; exit 7".into(),
                &cwd,
                BashExecOptions {
                    on_data: Arc::new(|_| Ok(())),
                    signal: None,
                    timeout: input["timeout"].as_f64(),
                    env: None,
                    detach: Some(BashDetachOptions {
                        artifacts,
                        job_id: None,
                        signal: controller.signal(),
                        adopt: Some(adopt),
                    }),
                },
            )
            .await;
        let result = match result {
            Ok(value) => {
                let mut value_json = json!({"exitCode":value.exit_code});
                if let Some(detached) = value.detached {
                    value_json["detached"] = json!({"jobId":detached.job_id,"message":detached.message});
                }
                json!({"ok":true,"value":value_json})
            }
            Err(error) => json!({"ok":false,"error":error.message}),
        };
        let actual = json!({"result":result,"calls":calls.lock().unwrap().clone(),"artifactExists":directory.exists()});
        if actual != expected["output"][index] {
            failures.push(json!({"case":index,"actual":actual,"expected":expected["output"][index]}));
        }
        if directory.exists() {
            std::fs::remove_dir_all(&directory).unwrap();
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        pi_js_compat::js_json::stringify_lossless(&json!(failures))
    );
}
