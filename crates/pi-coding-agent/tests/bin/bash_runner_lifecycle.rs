//! Test-only executable: the same binary owns tool scenarios and real runner children.
use crate::shared_oracle as oracle;
use pi_agent_core::agent::AgentTool;
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::detached_bash_runner::run_detached_bash_runner;
use pi_coding_agent::core::detached_job_runner::{
    DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions,
};
use pi_coding_agent::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};
use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions, is_active_lifecycle};
use pi_coding_agent::core::runtime_process::read_process_identity;
use pi_coding_agent::core::tools::bash::{BashBackgroundJobs, BashTool, BashToolDetachRegistry, BashToolOptions};
use pi_coding_agent::internal_runner::{DetachedRunnerHost, run_internal_detached_runner};
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

struct RunnerHost;
impl DetachedRunnerHost for RunnerHost {
    fn run_bash<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        Box::pin(async move {
            run_detached_bash_runner(path, None)
                .await
                .map(|_| ())
                .map_err(|error| error.message().clone())
        })
    }
    fn run_pyrun<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        Box::pin(async { Err("Unexpected pyrun invocation in bash-only test target".into()) })
    }
}
fn main() {
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if let Some(outcome) = runtime
        .block_on(run_internal_detached_runner(&args, &RunnerHost))
        .unwrap()
    {
        std::process::exit(outcome.exit_code.unwrap_or(0));
    }
    runtime.block_on(run_cases());
}
fn controller(root: &Path) -> Arc<DetachedJobLifecycleController> {
    let control_db_path: JsString = root.join("control.sqlite").to_str().unwrap().into();
    let session_path: JsString = root.join("session.jsonl").to_str().unwrap().into();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    let coordinator = Arc::new(LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: control_db_path.clone(),
        session_path: session_path.clone(),
        create_agent_id: Arc::new(|| Ok("unused".into())),
        now: Arc::new(|| "2026-10-24T00:00:00.000Z".into()),
        process_identity: read_process_identity(std::process::id()).unwrap(),
    }));
    Arc::new(DetachedJobLifecycleController::new(
        DetachedJobLifecycleControllerOptions {
            artifact_root: root.to_str().unwrap().into(),
            control_db_path,
            session_path,
            coordinator,
            store,
            owner_session_id: "main".into(),
            owner_agent_id: None,
            write_bash_launch_manifest: None,
        },
    ))
}
async fn run_cases() {
    pi_js_compat::process_env::set("BASH_REMOVE", "inherited");
    let temp = tempfile::tempdir().unwrap();
    let cases = vec![
        json!({"mode":"plain","command":"printf foreground"}),
        json!({"mode":"environment-undefined","command":"printf '%s:%s' \"${BASH_REMOVE-absent}\" \"${BASH_KEEP-absent}\""}),
        json!({"mode":"nonzero","command":"printf failure; exit 7"}),
        json!({"mode":"timeout","command":"printf ready; sleep 2","timeout":0.05}),
        json!({"mode":"abort","command":"printf ready; sleep 2"}),
        json!({"mode":"abort-term-resistant","command":r#"trap '' TERM; printf ready; sleep .15; printf survived > "$BASH_TEST_ROOT/abort-survived""#}),
        completion_case(
            "completion-fraction",
            r#"{"exitCode":1.25,"timedOut":false,"signal":null}"#,
        ),
        completion_case(
            "completion-empty-reason",
            r#"{"exitCode":0,"cancelReason":"","timedOut":false,"signal":null}"#,
        ),
        completion_case(
            "completion-string",
            r#"{"exitCode":"status-\ud800","timedOut":false,"signal":null}"#,
        ),
        completion_case(
            "completion-bool",
            r#"{"exitCode":false,"timedOut":false,"signal":null}"#,
        ),
        json!({"mode":"completion-directory","command":r#"mkdir "$BASH_TEST_ROOT/detached-jobs/session/bash_1/foreground-completed"; sleep .2"#}),
        json!({"mode":"completion-invalid-utf8","command":r#"printf '\377' > "$BASH_TEST_ROOT/detached-jobs/session/bash_1/foreground-completed"; sleep .2"#}),
        json!({"mode":"output-directory","command":r#"rm "$BASH_TEST_ROOT/detached-jobs/session/bash_1/output.log"; mkdir "$BASH_TEST_ROOT/detached-jobs/session/bash_1/output.log"; sleep .2"#}),
        json!({"mode":"runner-error","command":r#"printf '\357\273\277runner error\357\273\277' > "$BASH_TEST_ROOT/detached-jobs/session/bash_1/launch.json.runner-error"; kill -KILL "$PPID"; sleep .1"#}),
        json!({"mode":"runner-error-directory","command":r#"mkdir "$BASH_TEST_ROOT/detached-jobs/session/bash_1/launch.json.runner-error"; kill -KILL "$PPID"; sleep .1"#}),
        json!({"mode":"detach-activation-error","command":r#"mkdir "$BASH_TEST_ROOT/detached-jobs/session/bash_1/activation.json.tmp"; printf ready; sleep .2"#}),
        completion_case("completion-missing-code", r#"{"timedOut":false,"signal":null}"#),
        completion_case(
            "completion-null-code",
            r#"{"exitCode":null,"timedOut":false,"signal":null}"#,
        ),
        completion_case(
            "completion-truthy-timeout",
            r#"{"exitCode":0,"timedOut":"yes","signal":null}"#,
        ),
        json!({"mode":"detach-completed","command":r#"printf '%s' '{"exitCode":0,"timedOut":false,"signal":null}' > "$BASH_TEST_ROOT/detached-jobs/session/bash_1/foreground-completed"; printf ready; sleep .2"#}),
        json!({"mode":"detach","command":"printf ready; sleep .35; printf tail"}),
        json!({"mode":"detach-abort","command":"printf ready; sleep .35; printf tail"}),
    ];
    let mut source = oracle::TsOracle::spawn(
        "bash-runner-lifecycle-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected = source.request(&json!({"id":28059,"cwd":temp.path().to_str().unwrap(),"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(
        expected["output"][0]["result"]["value"]["content"][0]["text"],
        json!("foreground"),
        "source real runner must execute, not fail startup: {expected:?}"
    );
    assert_eq!(
        expected["output"][cases.len() - 2]["synchronous"][0],
        json!({"detached":true,"activationExists":true}),
        "source detachment must activate synchronously"
    );
    let mut failures = Vec::new();
    for (index, input) in cases.iter().enumerate() {
        let root = temp.path().join(format!("native-{index}"));
        std::fs::create_dir(&root).unwrap();
        let actual = execute_case(&root, input).await;
        let expected_root: JsString = temp.path().join(format!("source-{index}")).to_str().unwrap().into();
        let actual_root: JsString = root.to_str().unwrap().into();
        let actual = normalize(&actual, &actual_root);
        let expected = normalize(&expected["output"][index], &expected_root);
        if actual != expected {
            failures.push(json!({"case":index,"actual":actual,"expected":expected}));
        }
    }
    assert!(failures.is_empty(), "{}", js_json::stringify_lossless(&json!(failures)));
    println!("real bash lifecycle differential: {} histories passed", cases.len());
}
fn completion_case(mode: &str, body: &str) -> JsValue {
    json!({"mode":mode,"command":format!("printf '%s' '{body}' > \"$BASH_TEST_ROOT/detached-jobs/session/bash_1/foreground-completed\"; sleep .2")})
}
async fn execute_case(root: &Path, input: &JsValue) -> JsValue {
    let lifecycle = controller(root);
    let registry = Arc::new(BashToolDetachRegistry::default());
    let abort = AbortController::new();
    let hook_root: JsString = root.to_str().unwrap().into();
    let undefined_env = input["mode"] == json!("environment-undefined");
    let tool = BashTool::with_options(
        hook_root.clone(),
        BashToolOptions {
            background_jobs: Some(Arc::new(BashBackgroundJobs {
                get_lifecycle: None,
                lifecycle: Some(lifecycle.clone()),
            })),
            detach_registry: Some(registry.clone()),
            spawn_hook: Some(Arc::new(move |mut context| {
                context.env.insert("BASH_TEST_ROOT".into(), Some(hook_root.clone()));
                if undefined_env {
                    context.env.insert("BASH_REMOVE".into(), None);
                    context.env.insert("BASH_KEEP".into(), Some("kept".into()));
                }
                Ok(context)
            })),
            ..Default::default()
        },
    );
    let synchronous = Arc::new(Mutex::new(Vec::new()));
    let captures = synchronous.clone();
    let updates = Arc::new(Mutex::new(Vec::new()));
    let update_frames = updates.clone();
    let callback_registry = registry.clone();
    let callback_abort = abort.clone();
    let mode = input["mode"].as_string().unwrap().to_utf8().unwrap();
    let activation = root.join("detached-jobs/session/bash_1/activation.json");
    let fired = AtomicBool::new(false);
    let on_update = Arc::new(move |update: pi_agent_core::agent::AgentToolResult| {
        let text = JsString::join(
            update.content.iter().filter_map(|entry| match entry {
                pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
                _ => None,
            }),
            "",
        );
        update_frames
            .lock()
            .unwrap()
            .push(json!({"text":text,"activationExists":activation.exists()}));
        let ready = update
            .content
            .iter()
            .any(|entry| matches!(entry,pi_ai::types::TextOrImageContent::Text(text) if text.text.contains("ready")));
        if !ready || fired.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        if mode.starts_with("abort") {
            callback_abort.abort(None);
        }
        if mode == "detach-activation-error" {
            let registry = callback_registry.clone();
            let capture = captures.clone();
            let activation = activation.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let detached = registry.detach_running().expect("fixture detach callback");
                capture
                    .lock()
                    .unwrap()
                    .push(json!({"detached":detached,"activationExists":activation.exists()}));
            });
        }
        if mode == "detach" || mode == "detach-abort" || mode == "detach-completed" {
            let detached = callback_registry.detach_running().expect("fixture detach callback");
            captures
                .lock()
                .unwrap()
                .push(json!({"detached":detached,"activationExists":activation.exists()}));
            if mode == "detach-abort" {
                callback_abort.abort(None);
            }
        }
        Ok(())
    });
    let mut params = json!({"command":input["command"]});
    if let Some(timeout) = input.get("timeout") {
        params["timeout"] = timeout.clone();
    }
    let result = match tool
        .execute("runner".into(), params, Some(abort.signal()), on_update, None)
        .await
    {
        Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
        Err(error) => json!({"ok":false,"error":error.message}),
    };
    let mut actual = json!({"result":result,"synchronous":synchronous.lock().unwrap().clone(),"updates":updates.lock().unwrap().clone(),"hasRunningAfter":registry.has_running()});
    if let Some(job) = result["value"]["details"]["backgroundJobId"].as_string() {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(agent) = lifecycle.observe(job).unwrap()
                && !is_active_lifecycle(agent.lifecycle)
            {
                actual["terminal"] = js_json::to_value(&agent.lifecycle).unwrap();
                break;
            }
            assert!(Instant::now() < deadline, "detached runner did not terminate");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
    if input["mode"].as_string().unwrap() == "abort-term-resistant" {
        tokio::time::sleep(Duration::from_millis(250)).await;
        actual["payloadSurvived"] = json!(root.join("abort-survived").exists());
    }
    actual
}
fn normalize(value: &JsValue, root: &JsString) -> JsValue {
    match value {
        JsValue::String(text) => JsValue::String(text.replace(root, "<ROOT>")),
        JsValue::Array(items) => JsValue::Array(items.iter().map(|item| normalize(item, root)).collect()),
        JsValue::Object(items) => JsValue::Object(
            items
                .iter()
                .map(|(key, value)| (key.clone(), normalize(value, root)))
                .collect(),
        ),
        value => value.clone(),
    }
}

// Shared support is compiled once per integration target.
#[path = "../support/oracle.rs"]
pub mod shared_oracle;
