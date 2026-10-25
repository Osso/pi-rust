use crate::shared_oracle as oracle;
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
    let descendant_ready = temp.path().join("descendant-ready");
    let descendant_command = format!(
        "ready='{}'; rm -f \"$ready\"; printf early; (for i in 1 2 3 4 5 6; do printf x; done; : > \"$ready\") & while [ ! -f \"$ready\" ]; do sleep .01; done",
        descendant_ready.display()
    );
    let cases = vec![
        json!({"command":"printf plain"}),
        json!({"command":"printf '%s' \"$CUSTOM\"","env":{"CUSTOM":"set"}}),
        json!({"command":"exit 7"}),
        json!({"command":descendant_command}),
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
        json!({"command":"true","artifacts":true,"replaceDirectory":true}),
        json!({"command":"true","artifacts":true,"cleanupDirectory":"nul\0directory"}),
        json!({"command":"true","artifacts":true,"cleanupDirectory":JsString::from(file.to_str().unwrap())+"/child"}),
        json!({"command":"sleep 2","timeout":0.05,"artifacts":true,"cleanupDirectory":"nul\0directory"}),
        json!({"command":"true","artifacts":true,"shellPath":JsString::from(file.to_str().unwrap())}),
        json!({"command":"true","artifacts":true,"shellPath":JsString::from(file.to_str().unwrap()),"cleanupDirectory":"nul\0directory"}),
        json!({"command":"x".repeat(200_000),"artifacts":true}),
    ];
    let mut source = oracle::TsOracle::spawn("bash-local-oracle.ts", "packages/coding-agent/src/core/tools/bash.ts");
    let expected = source.request(&json!({"id":28054,"cwd":cwd,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut failures = Vec::new();
    for (index, input) in cases.iter().enumerate() {
        let directory = temp.path().join(format!("artifact-{index}"));
        if directory.exists() {
            if directory.is_dir() {
                std::fs::remove_dir_all(&directory).unwrap();
            } else {
                std::fs::remove_file(&directory).unwrap();
            }
        }
        let artifacts = if input["artifacts"] == JsValue::Bool(true) {
            if input["missingArtifacts"] != JsValue::Bool(true) {
                std::fs::create_dir(&directory).unwrap();
            }
            Some(DetachedJobArtifacts {
                directory: input["cleanupDirectory"]
                    .as_string()
                    .cloned()
                    .unwrap_or_else(|| directory.to_str().unwrap().into()),
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
                .map(|(key, value)| (key.clone(), Some(value.as_string().unwrap().clone())))
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
        let command = if input["replaceDirectory"] == JsValue::Bool(true) {
            format!(
                "rm -rf '{}'; printf sentinel > '{}'",
                directory.display(),
                directory.display()
            )
            .into()
        } else {
            input["command"].as_string().unwrap().clone()
        };
        let result = create_local_bash_operations(input["shellPath"].as_string())
            .exec(&command, input["cwd"].as_string().unwrap_or(&cwd), options)
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
