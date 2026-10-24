#[path = "support/oracle.rs"]
mod oracle;
use pi_agent_core::agent::AgentTool;
use pi_coding_agent::core::detached_job_runner::{
    DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions,
};
use pi_coding_agent::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};
use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::runtime_process::read_process_identity;
use pi_coding_agent::core::session_control_db::bootstrap_multi_agent_agent;
use pi_coding_agent::core::tools::bash::{BashBackgroundJobs, BashDetachOptions, BashTool, BashToolOptions};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::Arc;

// Frozen bash.ts:364-387: static lifecycle works without getLifecycle; restored
// terminal states bypass launch/timeout validation and propagate output read failures.
#[tokio::test]
async fn restored_bash_status_output_read_errors_and_optional_lifecycle_match_frozen() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = JsString::from(temp.path().to_str().unwrap());
    let output = temp.path().join("output.log");
    std::fs::write(&output, b"restored").unwrap();
    let mut cases = Vec::new();
    for (index, (lifecycle, error, path, timeout)) in [
        ("completed", None, output.as_path(), None),
        ("failed", Some("ordinary"), output.as_path(), None),
        ("aborted", None, output.as_path(), None),
        ("failed", Some("runner timed out"), output.as_path(), None),
        ("failed", Some("runner timed out"), output.as_path(), Some(1.25)),
        ("completed", None, temp.path(), None),
    ]
    .into_iter()
    .enumerate()
    {
        let agent = json!({"id":format!("restored-{index}"),"parentId":"main","displayName":"Bash command","agentType":"bash","lifecycle":lifecycle,"revision":1,"createdAt":"2026-10-24T00:00:00.000Z","updatedAt":"2026-10-24T00:00:00.000Z","cwd":cwd,"permission":{"policy":"inherit","narrowed":false},"result":{"toolCallId":"restore","fileRefs":[{"label":"Bash output","path":path.to_str().unwrap()}]}});
        let mut input = json!({"agent":agent});
        if let Some(error) = error {
            input["agent"]["error"] = json!({"message":error});
        }
        if let Some(timeout) = timeout {
            input["timeout"] = json!(timeout);
        }
        cases.push(input);
    }
    let mut source = oracle::TsOracle::spawn(
        "bash-restored-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected = source.request(&json!({"id":28057,"cwd":cwd,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut failures = Vec::new();
    for (index, input) in cases.iter().enumerate() {
        let control_db_path: JsString = temp
            .path()
            .join(format!("control-{index}.sqlite"))
            .to_str()
            .unwrap()
            .into();
        let session_path: JsString = temp
            .path()
            .join(format!("session-{index}.jsonl"))
            .to_str()
            .unwrap()
            .into();
        bootstrap_multi_agent_agent(
            control_db_path.clone(),
            session_path.clone(),
            input["agent"]["id"].as_string().unwrap(),
            &input["agent"],
        )
        .unwrap();
        let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
        let coordinator = Arc::new(LifecycleCoordinator::new(LifecycleCoordinatorOptions {
            control_db_path: control_db_path.clone(),
            session_path: session_path.clone(),
            create_agent_id: Arc::new(|| Ok("unused".into())),
            now: Arc::new(|| "2026-10-24T00:00:00.000Z".into()),
            process_identity: read_process_identity(std::process::id()).unwrap(),
        }));
        let lifecycle = Arc::new(DetachedJobLifecycleController::new(
            DetachedJobLifecycleControllerOptions {
                artifact_root: cwd.clone(),
                control_db_path,
                coordinator,
                owner_agent_id: None,
                owner_session_id: "main".into(),
                session_path,
                store,
                write_bash_launch_manifest: None,
            },
        ));
        let tool = BashTool::with_options(
            cwd.clone(),
            BashToolOptions {
                background_jobs: Some(Arc::new(BashBackgroundJobs {
                    get_lifecycle: None,
                    lifecycle: Some(lifecycle),
                })),
                detach: Some(BashDetachOptions {
                    artifacts: None,
                    job_id: None,
                    signal: AbortSignal::default(),
                    adopt: None,
                }),
                ..Default::default()
            },
        );
        let mut params = json!({"command":"must not launch"});
        if let Some(timeout) = input.get("timeout") {
            params["timeout"] = timeout.clone();
        }
        let actual = match tool
            .execute("restore".into(), params, None, Arc::new(|_| Ok(())), None)
            .await
        {
            Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
            Err(error) => json!({"ok":false,"error":error.message}),
        };
        if actual != expected["output"][index] {
            failures.push(json!({"case":index,"actual":actual,"expected":expected["output"][index]}));
        }
    }
    assert!(failures.is_empty(), "{}", js_json::stringify_lossless(&json!(failures)));
}
