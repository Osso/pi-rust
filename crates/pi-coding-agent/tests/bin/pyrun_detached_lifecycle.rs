//! Own the real initial launch and the internal runner dispatch in one executable.
#[path = "../support/bundled.rs"]
mod bundled;
#[path = "../support/extensions.rs"]
mod support;
use pi_ai::request::BoxFuture;
use pi_coding_agent::bundled_extensions::pyrun::{
    detached_evaluation::{DurablePyrunEvaluationInput, run_durable_detachable_pyrun_evaluation},
    detached_runner::run_detached_pyrun_runner,
    eval_tool::PyrunEvalParams,
    runner::PyrunRunnerOptions,
};
use pi_coding_agent::core::{
    extensions::create_extension_runtime,
    multi_agent_store::{AgentLifecycle, MultiAgentStore, MultiAgentStoreOptions},
    session_control_db::read_multi_agent_agent,
    session_manager::SessionManager,
    tool_detach_registry::ToolDetachRegistry,
};
use pi_coding_agent::internal_runner::{DetachedRunnerHost, run_internal_detached_runner};
use pi_js_compat::{js_json as json, js_json::JsString};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

struct RunnerHost;
impl DetachedRunnerHost for RunnerHost {
    fn run_bash<'a>(&'a self, _: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        Box::pin(async { Err("unexpected bash runner in Pyrun fixture".into()) })
    }
    fn run_pyrun<'a>(&'a self, path: &'a JsString) -> BoxFuture<'a, Result<(), JsString>> {
        Box::pin(async move { run_detached_pyrun_runner(path).await.map(|_| ()) })
    }
}

fn main() {
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some(outcome) = runtime
        .block_on(run_internal_detached_runner(&args, &RunnerHost))
        .unwrap()
    {
        std::process::exit(outcome.exit_code.unwrap_or(0));
    }
    runtime.block_on(async {
        run_fresh_evaluation(false).await;
        run_fresh_evaluation(true).await;
    });
    println!("PASS: fresh Pyrun launch completes foreground and detached through the real runner");
}

async fn run_fresh_evaluation(detach: bool) {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_str().unwrap();
    let control_path: JsString = directory.path().join("control.sqlite").to_str().unwrap().into();
    let mut session = SessionManager::create(&cwd.into(), Some(&cwd.into()), None).unwrap();
    session
        .set_metadata_control_db_path(Some(&control_path), Default::default())
        .unwrap();
    let session_path: JsString = session.get_session_file().unwrap().into();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    store.set_persistence_session_manager(Some(&session));
    let mut options = bundled::HarnessOptions::new(cwd);
    options.session_manager = Arc::new(Mutex::new(session));
    let harness = bundled::Harness::new(Vec::new(), create_extension_runtime(), options).await;
    let mut ctx = harness.context();
    ctx.tool_execution_started_at = Some(pi_js_compat::date::js_now_millis());
    let registry = Arc::new(ToolDetachRegistry::new());
    let runner_path = directory.path().join("runner.py");
    std::fs::write(&runner_path, "import json,sys,time,pathlib\nrequest=json.loads(sys.stdin.readline())\nprint(json.dumps({'type':'progress','message':'ready'}),flush=True)\nif request['code']=='detached':\n while not pathlib.Path(sys.argv[1]).exists(): time.sleep(0.01)\nprint(json.dumps({'type':'completed','executed':request['code'],'value':42}),flush=True)\n").unwrap();
    let release = directory.path().join("release");
    let script_root = std::path::Path::new(&json::encode_node_utf8(&session_path))
        .file_stem()
        .unwrap()
        .to_owned();
    let artifacts = directory.path().join("detached-jobs").join(script_root).join("pyrun_1");
    assert!(!artifacts.exists(), "the test must launch, not replay or resume");
    let detach_registry = registry.clone();
    let release_output = release.clone();
    let result = run_durable_detachable_pyrun_evaluation(DurablePyrunEvaluationInput {
        agent_id: Some("pyrun_1".into()),
        ctx,
        detach_registry: registry.clone(),
        dispatch_pi_request: Arc::new(|_, _, _, _| Box::pin(async { Ok(None) })),
        on_update: Some(Arc::new(move |_| {
            if detach {
                assert!(detach_registry.detach_running().unwrap());
                std::fs::write(&release_output, "ready").unwrap();
            }
            Ok(())
        })),
        params: PyrunEvalParams {
            code: if detach { "detached" } else { "foreground" }.into(),
            session_id: None,
        },
        pi_bridge_enabled: false,
        tool_call_id: Some("fresh-call".into()),
        runner_options: PyrunRunnerOptions {
            command: Some("python3".into()),
            args: Some(vec![
                runner_path.to_str().unwrap().into(),
                release.to_str().unwrap().into(),
            ]),
            ..Default::default()
        },
        signal: None,
        store,
    })
    .await
    .unwrap();
    assert!(!registry.has_running());
    let details = result.details.unwrap();
    if detach {
        assert_eq!(details["type"], json!("detached"));
        assert_eq!(details["backgroundJobId"], json!("pyrun_1"));
        wait_for_completed_job(&control_path, &session_path).await;
    } else {
        assert_eq!(details["value"], json!(42));
        assert_eq!(
            std::fs::read_to_string(artifacts.join("foreground-completed")).unwrap(),
            "completed\n"
        );
    }
    let records = std::fs::read_to_string(artifacts.join("output.log"))
        .unwrap()
        .lines()
        .map(|line| json::parse_lossless(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == json!("result"))
            .count(),
        1
    );
    assert_eq!(records.last().unwrap()["result"]["value"], json!(42));
    assert_eq!(
        records.last().unwrap()["result"]["executed"],
        json!(if detach { "detached" } else { "foreground" })
    );
    let manifest = pi_coding_agent::bundled_extensions::pyrun::detached_runner::read_detached_pyrun_launch_manifest(
        &artifacts.join("launch.json").to_str().unwrap().into(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while pi_coding_agent::core::detached_bash_runner::is_process_identity_alive(&manifest.data.runner_process_identity)
    {
        assert!(Instant::now() < deadline, "fresh runner did not exit after settlement");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_completed_job(control: &JsString, session: &JsString) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(agent) = read_multi_agent_agent(control, session, "pyrun_1").unwrap()
            && agent.lifecycle == AgentLifecycle::Completed
        {
            assert_eq!(agent.revision, 2);
            return;
        }
        assert!(Instant::now() < deadline, "fresh detached runner did not finalize");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
