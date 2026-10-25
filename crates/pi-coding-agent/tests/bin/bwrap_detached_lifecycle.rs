//! Test-only executable owns the extension call and its real detached runner.
use crate::shared_bundled as bundled;
use crate::shared_extensions as support;
use pi_ai::request::BoxFuture;
use pi_coding_agent::bundled_extensions::bwrap::index::{BwrapExtensionOptions, install_bwrap_extension};
use pi_coding_agent::core::detached_bash_runner::run_detached_bash_runner;
use pi_coding_agent::core::detached_job_runner::{
    DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions,
};
use pi_coding_agent::core::extensions::{create_extension_runtime, types::ExtensionContextActions};
use pi_coding_agent::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, MultiAgentStore, MultiAgentStoreOptions, is_active_lifecycle,
};
use pi_coding_agent::core::runtime_process::read_process_identity;
use pi_coding_agent::core::tool_detach_registry::ToolDetachRegistry;
use pi_coding_agent::internal_runner::{DetachedRunnerHost, run_internal_detached_runner};
use pi_js_compat::{js_json, js_json::JsString};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

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
        Box::pin(async { Err("unexpected pyrun in bwrap bash fixture".into()) })
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
    runtime.block_on(full_access_bash_forwards_detached_lifecycle_and_registry());
}

fn create_controller(root: &Path, store: Arc<MultiAgentStore>) -> Arc<DetachedJobLifecycleController> {
    let control_db_path: JsString = root.join("control.sqlite").to_str().unwrap().into();
    let session_path: JsString = root.join("session.jsonl").to_str().unwrap().into();
    let coordinator = Arc::new(LifecycleCoordinator::new(LifecycleCoordinatorOptions {
        control_db_path: control_db_path.clone(),
        session_path: session_path.clone(),
        create_agent_id: Arc::new(|| Ok("unused".into())),
        now: Arc::new(|| "2026-10-25T00:00:00.000Z".into()),
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

async fn full_access_bash_forwards_detached_lifecycle_and_registry() {
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    let lifecycle = create_controller(root.path(), store.clone());
    let registry = Arc::new(ToolDetachRegistry::with_auto_detach_after_ms(30.0));
    let runtime = create_extension_runtime();
    let cwd = root.path().to_str().unwrap();
    let extension = support::extension(&runtime, cwd, "<bwrap>", |api| {
        install_bwrap_extension(
            api,
            BwrapExtensionOptions {
                bwrap_command: Some("must-not-run-bwrap".into()),
            },
        )
        .unwrap();
    })
    .await;
    let tool = extension.read().tools["bash"].definition.clone();
    let mut harness = bundled::Harness::new(vec![extension], runtime, bundled::HarnessOptions::new(cwd)).await;
    let detached_registry = registry.clone();
    harness
        .runner
        .bind_core(
            support::actions(),
            ExtensionContextActions {
                get_multi_agent_store: Some(Arc::new(move || Some(store.clone()))),
                get_tool_detach_registry: Some(Arc::new(move || Some(detached_registry.clone()))),
                ..support::context_actions()
            },
            None,
        )
        .unwrap();
    let detached_lifecycle = lifecycle.clone();
    harness
        .runner
        .bind_detached_job_lifecycle(Arc::new(move || Some(detached_lifecycle.clone())));
    let result = (tool.execute)(
        "bwrap-call".into(),
        js_json!({"command":"printf ready; sleep .2; printf done"}),
        None,
        None,
        Some(harness.context()),
    )
    .await
    .unwrap();
    let details = result.details.as_ref().expect("detached details");
    let job = details["backgroundJobId"]
        .as_string()
        .expect("full-access override must retain background detachment");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(agent) = lifecycle.observe(job).unwrap()
            && !is_active_lifecycle(agent.lifecycle)
        {
            assert_eq!(agent.lifecycle, AgentLifecycle::Completed);
            break;
        }
        assert!(Instant::now() < deadline, "detached runner did not settle");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let output_path = root
        .path()
        .join("detached-jobs/session")
        .join(job.to_utf8().unwrap())
        .join("output.log");
    assert_eq!(std::fs::read_to_string(output_path).unwrap(), "readydone");
    assert!(!registry.has_running());
    println!("PASS: bwrap full-access detaches, completes, and preserves output without invoking bwrap");
}

// Shared support is compiled once per integration target.
#[path = "../support/bundled.rs"]
pub mod shared_bundled;
#[path = "../support/extensions.rs"]
pub mod shared_extensions;
