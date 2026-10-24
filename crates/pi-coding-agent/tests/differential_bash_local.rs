#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::tools::bash::{
    BashDetachOptions, BashExecOptions, BashOperations, DetachedJobArtifacts, create_local_bash_operations,
};
use pi_js_compat::abort::{AbortController, AbortSignal};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Frozen bash.ts:128-241 + child-process.ts:40-152. Real descendants must keep
// rearming the shared idle grace; durable output polls only once per second.
#[tokio::test]
async fn local_bash_process_output_error_and_lifetime_boundaries_match_frozen() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = JsString::from(temp.path().to_str().unwrap());
    let file = temp.path().join("file");
    std::fs::write(&file, "file").unwrap();
    let cases = vec![
        json!({"command":"printf plain"}),
        json!({"command":"printf '%s' \"$CUSTOM\"","env":{"CUSTOM":"set"}}),
        json!({"command":"exit 7"}),
        json!({"command":"printf early; (for i in 1 2 3 4 5 6; do sleep .06; printf x; done) &"}),
        json!({"command":"printf durable; sleep .15","artifacts":true}),
        json!({"command":"printf durable; sleep 1.2","artifacts":true}),
        json!({"command":"true","artifacts":true,"missingArtifacts":true}),
        json!({"command":"nul\0command"}),
        json!({"command":"true","cwd":JsString::from(file.to_str().unwrap())}),
        json!({"command":"true","cwd":cwd.clone()+"/absent"}),
        json!({"command":"true","preAbort":true}),
        json!({"command":"true","preAbort":true,"timeout":0}),
        json!({"command":"true","number":"NaN"}),
        json!({"command":"true","timeout":2147483.648}),
        json!({"command":"printf before; sleep 2","timeout":0.05}),
        json!({"command":"printf before; sleep 2","abortMs":50}),
    ];
    let mut source = oracle::TsOracle::spawn("bash-local-oracle.ts", "packages/coding-agent/src/core/tools/bash.ts");
    let expected = source.request(&json!({"id":28054,"cwd":cwd,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut failures = Vec::new();
    for (index, input) in cases.iter().enumerate() {
        let directory = temp.path().join(format!("artifact-{index}"));
        let artifacts = if input["artifacts"] == JsValue::Bool(true) {
            if input["missingArtifacts"] != JsValue::Bool(true) {
                std::fs::create_dir(&directory).unwrap();
            }
            Some(DetachedJobArtifacts {
                directory: directory.to_str().unwrap().into(),
                output_path: directory.join("output.log").to_str().unwrap().into(),
            })
        } else {
            None
        };
        let controller = AbortController::new();
        if input["preAbort"] == JsValue::Bool(true) {
            controller.abort(None)
        }
        let abort = input["abortMs"].as_u64().map(|ms| {
            let controller = controller.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(ms)).await;
                controller.abort(None);
            })
        });
        let data = Arc::new(Mutex::new(Vec::new()));
        let capture = data.clone();
        let timeout = match input["number"].as_string() {
            Some(number) => Some(pi_js_compat::number::parse_number(&number.to_utf8().unwrap())),
            None => input["timeout"].as_f64(),
        };
        let env = input["env"].as_object().map(|env| {
            env.iter()
                .map(|(key, value)| (key.clone(), value.as_string().unwrap().clone()))
                .collect()
        });
        let options = BashExecOptions {
            on_data: Arc::new(move |chunk| {
                capture.lock().unwrap().extend_from_slice(chunk);
                Ok(())
            }),
            signal: Some(controller.signal()),
            timeout,
            env,
            detach: artifacts.map(|artifacts| BashDetachOptions {
                artifacts: Some(artifacts),
                job_id: None,
                signal: AbortSignal::default(),
                adopt: None,
            }),
        };
        let result = create_local_bash_operations(input["shellPath"].as_string())
            .exec(
                input["command"].as_string().unwrap(),
                input["cwd"].as_string().unwrap_or(&cwd),
                options,
            )
            .await;
        if let Some(abort) = abort {
            abort.abort();
        }
        let result = match result {
            Ok(value) => json!({"ok":true,"value":{"exitCode":value.exit_code}}),
            Err(error) => json!({"ok":false,"error":error.message}),
        };
        let actual = json!({"result":result,"data":data.lock().unwrap().clone(),"artifactExists":directory.exists()});
        if actual != expected["output"][index] {
            failures.push(json!({"case":index,"actual":actual,"expected":expected["output"][index]}));
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        pi_js_compat::js_json::stringify_lossless(&json!(failures))
    );
}
