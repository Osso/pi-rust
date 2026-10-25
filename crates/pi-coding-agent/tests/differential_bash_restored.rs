use crate::shared_oracle as oracle;
use pi_agent_core::agent::{AgentTool, ToolError};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::detached_job_runner::{
    DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions,
};
use pi_coding_agent::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};
use pi_coding_agent::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::runtime_process::read_process_identity;
use pi_coding_agent::core::session_control_db::bootstrap_multi_agent_agent;
use pi_coding_agent::core::tools::bash::{
    BashBackgroundJobs, BashDetachOptions, BashExecOptions, BashExecResult, BashOperations, BashTool,
    BashToolDetachRegistry, BashToolOptions,
};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct RegistryOperations(Arc<AtomicBool>);
impl BashOperations for RegistryOperations {
    fn exec<'a>(
        &'a self,
        _: &'a JsString,
        _: &'a JsString,
        options: BashExecOptions,
    ) -> BoxFuture<'a, Result<BashExecResult, ToolError>> {
        self.0.store(
            options.detach.expect("registry config creates detach").signal.aborted(),
            Ordering::SeqCst,
        );
        Box::pin(async {
            Ok(BashExecResult {
                exit_code: Some(0.0),
                detached: None,
            })
        })
    }
}

// Frozen bash.ts:364-387: static lifecycle works without getLifecycle; restored
// terminal states bypass launch/timeout validation and propagate output read failures.
#[tokio::test]
async fn restored_bash_status_output_read_errors_and_optional_lifecycle_match_frozen() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = JsString::from(temp.path().to_str().unwrap());
    let original_cwd = pi_js_compat::process_cwd::cwd().unwrap();
    pi_js_compat::process_cwd::chdir(&cwd).unwrap();
    let stdin_shell = "C:\\Windows\\System32\\bash.exe";
    std::fs::write(temp.path().join(stdin_shell), "must not execute").unwrap();
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
    cases.push(json!({"agent":cases[0]["agent"],"shellPath":stdin_shell}));
    let mut lost_runtime = cases[1].clone();
    lost_runtime["agent"]["error"] = json!({"code":"lost_runtime","message":"runtime lost"});
    lost_runtime["timeout"] = json!(0);
    cases.push(lost_runtime);
    cases.push(json!({"agent":cases[0]["agent"],"registry":true}));
    let mut source = oracle::TsOracle::spawn(
        "bash-restored-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected = source.request(&json!({"id":28057,"cwd":cwd,"cases":cases}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(
        expected["output"][7],
        json!({"ok":false,"error":"Invalid timeout: must be a finite number of seconds"}),
        "lost_runtime must resume the launch path, not settle the failed restored record"
    );
    assert_eq!(
        expected["output"][6],
        json!({"ok":false,"error":"Detached Bash runner does not support stdin shell transport"})
    );
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
        let is_registry = input["registry"] == JsValue::Bool(true);
        let registry = Arc::new(BashToolDetachRegistry::default());
        let initial = Arc::new(Mutex::new(Vec::new()));
        let detached_at_exec = Arc::new(AtomicBool::new(false));
        let tool = BashTool::with_options(
            cwd.clone(),
            BashToolOptions {
                shell_path: input["shellPath"].as_string().cloned(),
                background_jobs: Some(Arc::new(BashBackgroundJobs {
                    get_lifecycle: None,
                    lifecycle: Some(lifecycle),
                })),
                detach: (!is_registry).then(|| BashDetachOptions {
                    artifacts: None,
                    job_id: None,
                    signal: AbortSignal::default(),
                    adopt: None,
                }),
                detach_registry: is_registry.then(|| registry.clone()),
                operations: is_registry
                    .then(|| Arc::new(RegistryOperations(detached_at_exec.clone())) as Arc<dyn BashOperations>),
                ..Default::default()
            },
        );
        let mut params = json!({"command":"must not launch"});
        if let Some(timeout) = input.get("timeout") {
            params["timeout"] = timeout.clone();
        }
        let observations = initial.clone();
        let callback_registry = registry.clone();
        let on_update = Arc::new(move |_| {
            if is_registry {
                observations.lock().unwrap().push(
                    json!({"hasRunning":callback_registry.has_running(),"detached":callback_registry.detach_running().expect("fixture detach callback")}),
                );
            }
            Ok(())
        });
        let actual = match tool.execute("restore".into(), params, None, on_update, None).await {
            Ok(value) => json!({"ok":true,"value":js_json::to_value(&value).unwrap()}),
            Err(error) => json!({"ok":false,"error":error.message}),
        };
        let actual = if is_registry {
            json!({"result":actual,"initial":initial.lock().unwrap().clone(),"detachedAtExec":detached_at_exec.load(Ordering::SeqCst),"hasRunningAfter":registry.has_running()})
        } else {
            actual
        };
        if actual != expected["output"][index] {
            failures.push(json!({"case":index,"actual":actual,"expected":expected["output"][index]}));
        }
    }
    pi_js_compat::process_cwd::chdir(&original_cwd).unwrap();
    assert!(failures.is_empty(), "{}", js_json::stringify_lossless(&json!(failures)));
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
