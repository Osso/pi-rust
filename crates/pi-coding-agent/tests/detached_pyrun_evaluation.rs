//! `extensions/pyrun/src/detached-evaluation.ts` restore and settlement
//! behavior: durable artifact replay, lost runtimes, artifact collisions, and
//! cancelling-job settlement through a real control database.

use crate::shared_executable_script as executable_script;
#[path = "support/bundled_extensions.rs"]
#[allow(dead_code)]
mod support;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pi_coding_agent::bundled_extensions::pyrun::detached_evaluation::{
    DurablePyrunEvaluationInput, run_durable_detachable_pyrun_evaluation,
};
use pi_coding_agent::bundled_extensions::pyrun::detached_runner::{
    DetachedPyrunLaunchManifestData, read_detached_pyrun_launch_manifest, write_detached_pyrun_activation,
    write_detached_pyrun_launch_manifest,
};
use pi_coding_agent::bundled_extensions::pyrun::eval_tool::{
    PyrunEvalParams, create_canonical_pyrun_eval_params, eval_context,
};
use pi_coding_agent::bundled_extensions::pyrun::runner::PyrunRunnerOptions;
use pi_coding_agent::core::detached_job_runner::{
    DetachedJobAgentType, DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions,
    RegisterDetachedJobInput,
};
use pi_coding_agent::core::extensions::types::ExtensionContext;
use pi_coding_agent::core::lifecycle_coordinator::{
    LifecycleCommandResult, LifecycleCoordinator, LifecycleCoordinatorOptions,
};
use pi_coding_agent::core::multi_agent_store::{AgentLifecycle, AgentNode, MultiAgentStore, MultiAgentStoreOptions};
use pi_coding_agent::core::runtime_process::{ProcessIdentity, read_process_identity};
use pi_coding_agent::core::session_control_db::{RuntimeMailboxAddress, read_multi_agent_agent};
use pi_coding_agent::core::session_manager::{SessionManager, SharedSessionManager};
use pi_coding_agent::core::tool_detach_registry::ToolDetachRegistry;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use tempfile::TempDir;

const DEAD_PID: u32 = 999_999_999;

struct Fixture {
    _directory: TempDir,
    root: String,
    control_db_path: String,
    session_path: JsString,
    session_manager: SharedSessionManager,
    store: Arc<MultiAgentStore>,
    controller: DetachedJobLifecycleController,
    context: ExtensionContext,
}

async fn fixture(name: &str) -> Fixture {
    let directory = tempfile::Builder::new()
        .prefix(&format!("pi-pyrun-eval-{name}-"))
        .tempdir()
        .expect("temp dir");
    let root = directory.path().to_str().expect("utf-8 test path").to_owned();
    let control_db_path = directory
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let sessions = directory.path().join("sessions");
    std::fs::create_dir_all(&sessions).expect("sessions dir");
    let mut session_manager = SessionManager::create(
        &JsString::from(&root),
        Some(&JsString::from(sessions.to_str().expect("sessions path"))),
        None,
    )
    .expect("session");
    session_manager
        .set_metadata_control_db_path(Some(&JsString::from(&control_db_path)), Default::default())
        .expect("control db path");
    let session_path: JsString = session_manager.get_session_file().expect("session file").into();
    let session_manager: SharedSessionManager = Arc::new(Mutex::new(session_manager));

    let store = Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default()));
    {
        let session = session_manager.lock().expect("session lock");
        store.set_persistence_session_manager(Some(&*session));
    }
    let controller = DetachedJobLifecycleController::new(DetachedJobLifecycleControllerOptions {
        artifact_root: pi_coding_agent::utils::paths::node_dirname(&session_path),
        control_db_path: control_db_path.clone().into(),
        coordinator: Arc::new(LifecycleCoordinator::new(LifecycleCoordinatorOptions {
            control_db_path: control_db_path.clone().into(),
            create_agent_id: {
                let store = Arc::clone(&store);
                Arc::new(move || {
                    store
                        .allocate_agent_id_for_lifecycle_coordinator("pyrun")
                        .map_err(|error| pi_coding_agent::core::sqlite::ControlDbError::new(error.message))
                })
            },
            now: Arc::new(pi_coding_agent::utils::iso_time::now_iso_string),
            process_identity: process_identity(DEAD_PID),
            session_path: session_path.clone(),
        })),
        owner_agent_id: None,
        owner_session_id: "main".into(),
        session_path: session_path.clone(),
        store: Arc::clone(&store),
        write_bash_launch_manifest: None,
    });
    let harness = support::Harness::load_full(
        &root,
        Arc::clone(&session_manager),
        Arc::new(support::RecordingUi::default()),
        Some(control_db_path.clone()),
        |_api| Ok(()),
    )
    .await;
    let mut context = harness.context();
    context.tool_execution_started_at = Some(1_700_000_000_000.0);
    Fixture {
        _directory: directory,
        root,
        control_db_path,
        session_path,
        session_manager,
        store,
        controller,
        context,
    }
}

fn process_identity(pid: u32) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        start_time_ticks: 7,
        incarnation: None,
    }
}

fn job_directory(fixture: &Fixture, job_id: &JsString) -> PathBuf {
    let session_file = PathBuf::from(encode_node_utf8(&fixture.session_path));
    session_file
        .parent()
        .expect("session parent")
        .join("detached-jobs")
        .join(session_file.file_stem().expect("session stem"))
        .join(encode_node_utf8(job_id))
}

struct Artifact {
    directory: PathBuf,
    manifest_path: PathBuf,
    output_path: PathBuf,
    script_path: PathBuf,
}

fn set_runner_process_identity(artifact: &Artifact, identity: ProcessIdentity) {
    let mut manifest = read_detached_pyrun_launch_manifest(&JsString::from(
        artifact.manifest_path.to_str().expect("utf-8 test path"),
    ))
    .expect("manifest");
    manifest.data.runner_process_identity = identity;
    write_detached_pyrun_launch_manifest(
        &JsString::from(artifact.manifest_path.to_str().expect("utf-8 test path")),
        &manifest.data,
    )
    .expect("rewrite manifest");
}

fn write_artifact(fixture: &Fixture, job_id: &str, tool_call_id: Option<&str>, code: &str, output: &str) -> Artifact {
    let directory = job_directory(fixture, &JsString::from(job_id));
    std::fs::create_dir_all(&directory).expect("artifact dir");
    let output_path = directory.join("output.log");
    let script_path = directory.join("script.py");
    let manifest_path = directory.join("launch.json");
    std::fs::write(&output_path, output).expect("output");
    std::fs::write(&script_path, format!("{code}\n")).expect("script");
    let eval_context = eval_context(&fixture.context).expect("eval context");
    let params = create_canonical_pyrun_eval_params(
        &PyrunEvalParams {
            code: code.into(),
            session_id: None,
        },
        &eval_context,
        false,
    )
    .unwrap();
    let session_id = fixture
        .session_manager
        .lock()
        .expect("session lock")
        .get_session_id()
        .into();
    write_detached_pyrun_launch_manifest(
        &JsString::from(manifest_path.to_str().expect("utf-8 test path")),
        &DetachedPyrunLaunchManifestData {
            activation_path: directory
                .join("activation.json")
                .to_str()
                .expect("utf-8 test path")
                .to_owned()
                .into(),
            artifacts: pi_coding_agent::core::detached_job_runner::DetachedJobArtifacts {
                directory: directory.to_str().expect("utf-8 test path").to_owned().into(),
                output_path: output_path.to_str().expect("utf-8 test path").to_owned().into(),
            },
            bridge_request_path: directory
                .join("bridge-requests.jsonl")
                .to_str()
                .expect("utf-8 test path")
                .to_owned()
                .into(),
            bridge_response_path: directory
                .join("bridge-responses.jsonl")
                .to_str()
                .expect("utf-8 test path")
                .to_owned()
                .into(),
            control_db_path: fixture.control_db_path.clone().into(),
            foreground_completion_path: directory
                .join("foreground-completed")
                .to_str()
                .expect("utf-8 test path")
                .to_owned()
                .into(),
            params,
            runner_address: RuntimeMailboxAddress {
                agent_id: Some(job_id.into()),
                session_id,
            },
            runner_options: PyrunRunnerOptions {
                command: Some(
                    directory
                        .join("missing-pyrun")
                        .to_str()
                        .expect("utf-8 test path")
                        .to_owned()
                        .into(),
                ),
                ..Default::default()
            },
            runner_process_identity: process_identity(DEAD_PID),
            session_path: fixture.session_path.clone(),
            started_at: 1_700_000_000_000.0,
            supervisor_process_identity: process_identity(std::process::id()),
            tool_call_id: tool_call_id.map(JsString::from),
        },
    )
    .expect("manifest");
    Artifact {
        directory,
        manifest_path,
        output_path,
        script_path,
    }
}

fn evaluation_input(
    fixture: &Fixture,
    job_id: &str,
    code: &str,
    tool_call_id: Option<&str>,
) -> DurablePyrunEvaluationInput {
    DurablePyrunEvaluationInput {
        agent_id: Some(job_id.into()),
        ctx: fixture.context.clone(),
        detach_registry: Arc::new(ToolDetachRegistry::new()),
        dispatch_pi_request: Arc::new(|_request, _ctx, _signal, _tool_call_id| {
            Box::pin(async {
                Err(pi_coding_agent::core::extensions::types::HandlerError::new(
                    "Pi bridge disabled",
                ))
            })
        }),
        on_update: None,
        params: PyrunEvalParams {
            code: code.into(),
            session_id: None,
        },
        pi_bridge_enabled: false,
        tool_call_id: tool_call_id.map(JsString::from),
        runner_options: PyrunRunnerOptions {
            command: Some(format!("{}/missing-pyrun", fixture.root).into()),
            ..Default::default()
        },
        signal: None,
        store: Arc::clone(&fixture.store),
    }
}

fn content_text(result: &pi_agent_core::AgentToolResult) -> JsString {
    JsString::join(
        result.content.iter().filter_map(|content| match content {
            pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
            _ => None,
        }),
        "\n",
    )
}

#[tokio::test]
async fn replays_a_complete_terminal_result_without_launching_replacement_code() {
    let fixture = fixture("replay-result").await;
    let output = [
        r#"{"kind":"progress","update":{"stream":"stdout","text":"streamed output\n","type":"console"}}"#,
        r#"{"kind":"result","result":{"console":["terminal output"],"executed":"original code","type":"completed","value":42}}"#,
        "",
    ]
    .join("\n");
    let artifact = write_artifact(
        &fixture,
        "pyrun_1",
        Some("restore-result-call"),
        "original code",
        &output,
    );

    let result = run_durable_detachable_pyrun_evaluation(evaluation_input(
        &fixture,
        "pyrun_1",
        "original code",
        Some("restore-result-call"),
    ))
    .await
    .expect("replayed result");

    assert_eq!(
        result.details.as_ref().expect("details"),
        &json!({"console": ["terminal output"], "executed": "original code", "type": "completed", "value": 42})
    );
    assert_eq!(result.is_error, Some(false));
    assert!(content_text(&result).contains("terminal output"));
    assert_eq!(std::fs::read_to_string(&artifact.output_path).expect("output"), output);
    assert!(artifact.manifest_path.exists());
    assert!(artifact.script_path.exists());
    assert!(!job_directory(&fixture, &JsString::from("pyrun_2")).exists());
    assert!(!artifact.directory.join("foreground-completed").exists());
}

#[tokio::test]
async fn replays_a_complete_terminal_error_with_preceding_console_output() {
    let fixture = fixture("replay-error").await;
    let output = [
        r#"{"kind":"progress","update":{"stream":"stdout","text":"before failure\n","type":"console"}}"#,
        r#"{"error":"failed durably","kind":"error"}"#,
        "",
    ]
    .join("\n");
    let artifact = write_artifact(
        &fixture,
        "pyrun_1",
        Some("restore-error-call"),
        "original code",
        &output,
    );

    let result = run_durable_detachable_pyrun_evaluation(evaluation_input(
        &fixture,
        "pyrun_1",
        "original code",
        Some("restore-error-call"),
    ))
    .await
    .expect("replayed error");

    assert_eq!(
        result.details.as_ref().expect("details"),
        &json!({"console": ["before failure\n"], "error": "failed durably", "type": "error"})
    );
    assert_eq!(result.is_error, Some(true));
    let text = content_text(&result);
    assert!(text.contains("before failure"));
    assert!(text.contains("failed durably"));
    assert_eq!(std::fs::read_to_string(&artifact.output_path).expect("output"), output);
}

#[tokio::test]
async fn ignores_an_incomplete_trailing_record_when_reporting_a_lost_runtime() {
    let fixture = fixture("lost-runtime").await;
    let output = concat!(
        r#"{"kind":"progress","update":{"stream":"stdout","text":"preserved output\n","type":"console"}}"#,
        "\n",
        r#"{"kind":"result""#,
    );
    let artifact = write_artifact(
        &fixture,
        "pyrun_1",
        Some("restore-partial-call"),
        "original code",
        output,
    );

    let result = run_durable_detachable_pyrun_evaluation(evaluation_input(
        &fixture,
        "pyrun_1",
        "original code",
        Some("restore-partial-call"),
    ))
    .await
    .expect("lost runtime");

    let details = result.details.as_ref().expect("details");
    assert_eq!(details["type"], json!("lost_runtime"));
    assert_eq!(
        details["outputPath"],
        json!(artifact.output_path.to_str().expect("utf-8 test path"))
    );
    assert_eq!(
        details["error"],
        json!("Pyrun runner exited before producing a result.")
    );
    assert_eq!(result.is_error, Some(true));
    let text = content_text(&result);
    assert!(text.contains("preserved output"));
    assert!(text.contains(format!(
        "Preserved output: {}",
        artifact.output_path.to_str().expect("utf-8 test path")
    )));
    assert_eq!(std::fs::read_to_string(&artifact.output_path).expect("output"), output);
}

#[tokio::test]
async fn rejects_an_artifact_collision_when_the_params_differ() {
    let fixture = fixture("collision").await;
    let output = [
        r#"{"kind":"result","result":{"executed":"original code","type":"completed","value":42}}"#,
        "",
    ]
    .join("\n");
    write_artifact(&fixture, "pyrun_1", Some("collision-call"), "original code", &output);

    let error = run_durable_detachable_pyrun_evaluation(evaluation_input(
        &fixture,
        "pyrun_1",
        "replacement code",
        Some("collision-call"),
    ))
    .await
    .expect_err("collision");

    assert_eq!(error.message, "Pyrun tool-call artifact collision for collision-call");
}

#[tokio::test]
async fn rejects_a_manifest_written_for_another_agent() {
    let fixture = fixture("agent-mismatch").await;
    let artifact = write_artifact(&fixture, "pyrun_1", Some("call-1"), "original code", "");
    let mut manifest = read_detached_pyrun_launch_manifest(&JsString::from(
        artifact.manifest_path.to_str().expect("utf-8 test path"),
    ))
    .expect("manifest");
    manifest.data.runner_address.agent_id = Some("other-agent".into());
    write_detached_pyrun_launch_manifest(
        &JsString::from(artifact.manifest_path.to_str().expect("utf-8 test path")),
        &manifest.data,
    )
    .expect("rewrite manifest");

    let error =
        run_durable_detachable_pyrun_evaluation(evaluation_input(&fixture, "pyrun_1", "original code", Some("call-1")))
            .await
            .expect_err("agent mismatch");
    assert_eq!(
        error.message,
        format!(
            "Pyrun launch manifest agent ID mismatch: {}",
            artifact.manifest_path.to_str().expect("utf-8 test path")
        )
    );
}

#[tokio::test]
async fn rejects_multiple_terminal_records() {
    let fixture = fixture("multiple-terminals").await;
    let output = [
        r#"{"kind":"result","result":{"executed":"original code","type":"completed","value":1}}"#,
        r#"{"kind":"result","result":{"executed":"original code","type":"completed","value":2}}"#,
        "",
    ]
    .join("\n");
    write_artifact(&fixture, "pyrun_1", Some("call-1"), "original code", &output);

    let error =
        run_durable_detachable_pyrun_evaluation(evaluation_input(&fixture, "pyrun_1", "original code", Some("call-1")))
            .await
            .expect_err("multiple terminal records");
    assert_eq!(
        error.message,
        "Pyrun output artifact contains multiple terminal records"
    );
}

async fn wedge_cancelling_job(fixture: &Fixture, tool_call_id: &str) -> (Artifact, AgentNode) {
    let job_id = "pyrun_1";
    let ownership = fixture
        .controller
        .register(&RegisterDetachedJobInput {
            agent_type: DetachedJobAgentType::Pyrun,
            cwd: fixture.root.clone().into(),
            display_name: "Pyrun evaluation".into(),
            job_id: job_id.into(),
            process_identity: process_identity(DEAD_PID),
            worker_handle_id: DEAD_PID.to_string().into(),
            tool_call_id: Some(tool_call_id.into()),
            detached: Some(true),
            file_refs: None,
        })
        .expect("register");
    let artifact = write_artifact(fixture, job_id, Some(tool_call_id), "run forever", "");
    write_detached_pyrun_activation(
        &JsString::from(
            artifact
                .directory
                .join("activation.json")
                .to_str()
                .expect("utf-8 test path"),
        ),
        &ownership.identity,
    )
    .expect("activation");
    // The runner died before writing any output; the artifact keeps only the manifest.
    std::fs::remove_file(&artifact.output_path).expect("remove output");
    let cancelling = fixture.controller.cancel(&ownership, None).expect("cancelling");
    let cancelling_agent = match cancelling {
        LifecycleCommandResult::Ok { agent } => agent,
        other => panic!("expected cancelling state: {other:?}"),
    };
    (artifact, *cancelling_agent)
}

#[tokio::test]
async fn settles_a_cancelling_job_to_aborted_instead_of_rerunning_it() {
    let fixture = fixture("cancelling").await;
    let (artifact, cancelling_agent) = wedge_cancelling_job(&fixture, "resume-cancel-call").await;

    let result = run_durable_detachable_pyrun_evaluation(evaluation_input(
        &fixture,
        "pyrun_1",
        "run forever",
        Some("resume-cancel-call"),
    ))
    .await
    .expect("settled aborted");

    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result.details.as_ref().expect("details")["error"],
        json!("Pyrun evaluation aborted")
    );
    let agent = read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, "pyrun_1")
        .expect("read")
        .expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Aborted);
    assert_eq!(agent.revision, cancelling_agent.revision + 1);
    assert!(artifact.output_path.exists());
    assert!(!job_directory(&fixture, &JsString::from("pyrun_2")).exists());
}

#[tokio::test]
async fn settles_a_legacy_cancelling_job_without_a_tool_call_id() {
    let fixture = fixture("legacy-cancelling").await;
    let (_artifact, cancelling_agent) = wedge_cancelling_job(&fixture, "legacy-cancel-call").await;
    let manifest = read_detached_pyrun_launch_manifest(&JsString::from(
        job_directory(&fixture, &JsString::from("pyrun_1"))
            .join("launch.json")
            .to_str()
            .expect("utf-8 test path"),
    ))
    .expect("manifest");
    let mut data = manifest.data;
    data.tool_call_id = None;
    write_detached_pyrun_launch_manifest(
        &JsString::from(
            job_directory(&fixture, &JsString::from("pyrun_1"))
                .join("launch.json")
                .to_str()
                .expect("utf-8 test path"),
        ),
        &data,
    )
    .expect("legacy manifest");

    let result = run_durable_detachable_pyrun_evaluation(evaluation_input(
        &fixture,
        "pyrun_1",
        "run forever",
        Some("some-unrelated-call"),
    ))
    .await
    .expect("settled aborted");

    assert_eq!(result.is_error, Some(true));
    let agent = read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, "pyrun_1")
        .expect("read")
        .expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Aborted);
    assert_eq!(agent.revision, cancelling_agent.revision + 1);
    assert!(!job_directory(&fixture, &JsString::from("pyrun_2")).exists());
}

#[tokio::test]
async fn returns_the_persisted_cancelling_snapshot_when_no_activation_was_written() {
    let fixture = fixture("cancelling-without-activation").await;
    let (artifact, cancelling_agent) = wedge_cancelling_job(&fixture, "no-activation-call").await;
    std::fs::remove_file(artifact.directory.join("activation.json")).expect("remove activation");

    let result = run_durable_detachable_pyrun_evaluation(evaluation_input(
        &fixture,
        "pyrun_1",
        "run forever",
        Some("no-activation-call"),
    ))
    .await
    .expect("persisted cancelling snapshot");

    assert_eq!(result.is_error, Some(true));
    let agent = read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, "pyrun_1")
        .expect("read")
        .expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Cancelling);
    assert_eq!(agent.revision, cancelling_agent.revision);
}

#[tokio::test]
async fn requires_the_tool_lifecycle_start_timestamp() {
    let fixture = fixture("missing-start").await;
    let mut input = evaluation_input(&fixture, "pyrun_1", "1 + 1", Some("call-1"));
    input.ctx.tool_execution_started_at = None;
    let error = run_durable_detachable_pyrun_evaluation(input)
        .await
        .expect_err("missing start");
    assert_eq!(
        error.message,
        "Detached Pyrun requires the tool lifecycle start timestamp"
    );
}

fn spawn_live_runner(artifact: &Artifact, body: &str) -> std::process::Child {
    let script = artifact.directory.join("fake-runner");
    let body = body.replace("OUTPUT_PATH", artifact.output_path.to_str().expect("utf-8 test path"));
    executable_script::write_executable_script(&script, &format!("#!/usr/bin/env python3\n{body}"));
    std::process::Command::new(&script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("live runner")
}

async fn wait_for(predicate: impl Fn() -> bool, what: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !predicate() {
        assert!(std::time::Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn resumes_a_live_runner_and_reports_its_terminal_result() {
    let fixture = fixture("resume-live").await;
    let artifact = write_artifact(&fixture, "pyrun_1", Some("live-call"), "original code", "");
    let mut runner = spawn_live_runner(
        &artifact,
        "import json, time, pathlib\nwhile not pathlib.Path(r'''OUTPUT_PATH''').with_name('continue').exists():\n    time.sleep(0.01)\nwith open(r'''OUTPUT_PATH''', 'a') as handle:\n    handle.write(json.dumps({'kind': 'result', 'result': {'executed': 'original code', 'type': 'completed', 'value': 7}}) + '\\n')\n",
    );
    set_runner_process_identity(&artifact, read_process_identity(runner.id()).expect("runner identity"));

    let registry = Arc::new(ToolDetachRegistry::new());
    let mut input = evaluation_input(&fixture, "pyrun_1", "original code", Some("live-call"));
    input.detach_registry = registry.clone();
    let evaluation = tokio::spawn(run_durable_detachable_pyrun_evaluation(input));
    wait_for(|| registry.has_running(), "foreground observer registration").await;
    std::fs::write(artifact.directory.join("continue"), "ready").unwrap();
    let result = evaluation.await.expect("join observer").expect("resumed result");
    runner.wait().expect("reap runner");

    assert_eq!(result.details.as_ref().expect("details")["value"], json!(7));
    assert_eq!(result.is_error, Some(false));
    assert_eq!(
        std::fs::read_to_string(artifact.directory.join("foreground-completed")).expect("completion"),
        "completed\n"
    );
}

#[tokio::test]
async fn detaches_to_a_handle_while_the_runner_keeps_running() {
    let fixture = fixture("detach-live").await;
    let artifact = write_artifact(&fixture, "pyrun_1", Some("detach-call"), "original code", "");
    let mut runner = spawn_live_runner(
        &artifact,
        "import json, time\nwith open(r'''OUTPUT_PATH''', 'a') as handle:\n    handle.write(json.dumps({'kind': 'progress', 'update': {'type': 'progress', 'message': 'started'}}) + '\\n')\ntime.sleep(30)\n",
    );
    let runner_identity = read_process_identity(runner.id()).expect("runner identity");
    set_runner_process_identity(&artifact, runner_identity.clone());
    let registry = Arc::new(ToolDetachRegistry::new());
    let mut input = evaluation_input(&fixture, "pyrun_1", "original code", Some("detach-call"));
    input.detach_registry = Arc::clone(&registry);
    let evaluation = tokio::spawn(run_durable_detachable_pyrun_evaluation(input));
    wait_for(
        || {
            registry.has_running()
                && std::fs::read_to_string(&artifact.output_path).is_ok_and(|output| output.contains("started"))
        },
        "foreground observation",
    )
    .await;

    assert!(registry.detach_running().expect("fixture detach callback"));
    let result = evaluation.await.expect("join").expect("detached");
    let details = result.details.as_ref().expect("details");
    assert_eq!(details["type"], json!("detached"));
    assert_eq!(details["backgroundJobId"], json!("pyrun_1"));
    assert_eq!(details["executed"], json!("original code"));
    assert!(artifact.directory.join("activation.json").exists());
    let agent = read_multi_agent_agent(&fixture.control_db_path, &fixture.session_path, "pyrun_1")
        .expect("read")
        .expect("agent");
    assert_eq!(agent.lifecycle, AgentLifecycle::Running);

    let _ = pi_coding_agent::core::detached_bash_runner::terminate_foreground_runner(
        runner_identity.pid,
        rustix::process::Signal::KILL,
    );
    runner.wait().expect("reap runner");
}

#[tokio::test]
async fn aborts_a_foreground_runner_when_the_signal_aborted() {
    let fixture = fixture("abort-live").await;
    let artifact = write_artifact(&fixture, "pyrun_1", Some("abort-call"), "original code", "");
    let mut runner = spawn_live_runner(&artifact, "import time\ntime.sleep(30)\n");
    set_runner_process_identity(&artifact, read_process_identity(runner.id()).expect("runner identity"));
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let registry = Arc::new(ToolDetachRegistry::new());
    let mut input = evaluation_input(&fixture, "pyrun_1", "original code", Some("abort-call"));
    input.detach_registry = Arc::clone(&registry);
    input.signal = Some(signal.clone());
    let evaluation = tokio::spawn(run_durable_detachable_pyrun_evaluation(input));
    wait_for(|| registry.has_running(), "foreground observation").await;

    controller_signal.abort(None);
    let error = evaluation.await.expect("join").expect_err("aborted runner");
    assert_eq!(
        error.message,
        "Foreground Pyrun runner exited without producing a result"
    );
    runner.wait().expect("reap runner");
}

#[tokio::test]
async fn repin_replays_capped_artifacts_without_treating_truncation_as_terminal() {
    let fixture = fixture("truncation-replay").await;
    let artifact = write_artifact(
        &fixture,
        "pyrun_1",
        Some("call-1"),
        "code",
        concat!(
            "{\"kind\":\"truncation\",\"message\":\"omitted\",\"omittedBytes\":12,\"omittedRecords\":1}\n",
            "{\"kind\":\"result\",\"result\":{\"type\":\"completed\",\"value\":42}}\n"
        ),
    );
    let result = run_durable_detachable_pyrun_evaluation(evaluation_input(&fixture, "pyrun_1", "code", Some("call-1")))
        .await
        .unwrap();
    assert_eq!(result.details.unwrap()["value"], json!(42));
    assert!(!artifact.directory.join("foreground-completed").exists());
}

#[tokio::test]
async fn repin_restored_runner_error_writes_failure_before_lost_runtime() {
    for diagnostic in [" runner setup failed\n", " \n"] {
        let fixture = fixture("runner-error-replay").await;
        let artifact = write_artifact(&fixture, "pyrun_1", Some("call-1"), "code", "");
        std::fs::write(artifact.manifest_path.with_extension("json.runner-error"), diagnostic).unwrap();
        let error =
            run_durable_detachable_pyrun_evaluation(evaluation_input(&fixture, "pyrun_1", "code", Some("call-1")))
                .await
                .expect_err("runner diagnostic must not become lost_runtime");
        let expected = if diagnostic.trim().is_empty() {
            "Pyrun runner failed without diagnostic output."
        } else {
            "runner setup failed"
        };
        assert_eq!(error.message, expected);
        assert_eq!(
            std::fs::read_to_string(artifact.directory.join("foreground-completed")).unwrap(),
            "failed\n"
        );
    }
}

#[tokio::test]
async fn repin_observed_runner_error_wakes_and_keeps_the_original_diagnostic() {
    let fixture = fixture("runner-error-live").await;
    let artifact = write_artifact(&fixture, "pyrun_1", Some("call-1"), "code", "");
    // Publish complete text: frozen readers intentionally report a different error for an empty file.
    let mut runner = spawn_live_runner(
        &artifact,
        "import pathlib, time\ntime.sleep(0.15)\nerror = pathlib.Path(r\"OUTPUT_PATH\").with_name('launch.json.runner-error')\npending = error.with_suffix('.pending')\npending.write_text('live runner failed\\n')\npending.replace(error)\ntime.sleep(30)\n",
    );
    set_runner_process_identity(&artifact, read_process_identity(runner.id()).unwrap());
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        run_durable_detachable_pyrun_evaluation(evaluation_input(&fixture, "pyrun_1", "code", Some("call-1"))),
    )
    .await;
    if result.is_err() {
        runner.kill().unwrap();
    }
    runner.wait().unwrap();
    assert_eq!(
        result
            .expect("runner diagnostic must wake observer")
            .unwrap_err()
            .message,
        "live runner failed"
    );
    assert_eq!(
        std::fs::read_to_string(artifact.directory.join("foreground-completed")).unwrap(),
        "failed\n"
    );
}
