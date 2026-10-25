use super::terminate_foreground_bash_runner;
use crate::test_oracle as oracle;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// Frozen bash.ts:515-517 delegates to shell.killProcessTree, whose POSIX branch
// sends SIGKILL. Probe the helper directly: the real runner's parent-death gate
// can mask a TERM/KILL difference by killing its payload after runner death.
#[test]
fn foreground_runner_cancellation_kills_a_term_resistant_process_group() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn(
        "bash-runner-termination-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected: JsValue = source.request(&json!({"id":28060,"cwd":root.path().to_str().unwrap()}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(
        expected["output"],
        json!({"survived":false,"exitCode":null,"signal":"SIGKILL"})
    );
    let ready = root.path().join("native-ready");
    let survived = root.path().join("native-survived");
    let mut child = Command::new("/bin/sh")
        .args([
            "-c",
            &format!(
                "trap '' TERM; printf ready > '{}'; sleep .25; printf survived > '{}'",
                ready.display(),
                survived.display()
            ),
        ])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready.exists() {
        assert!(Instant::now() < deadline, "TERM-resistant fixture did not start");
        std::thread::sleep(Duration::from_millis(5));
    }
    terminate_foreground_bash_runner(child.id());
    let status = child.wait().unwrap();
    let actual = json!({"survived":survived.exists(),"exitCode":status.code(),"signal":status.signal().map(crate::utils::child_process::signal_name)});
    assert_eq!(actual, expected["output"]);
}

#[test]
fn failed_activation_does_not_mask_a_subsequent_completion_read_error() {
    use super::{BashDetachOptions, ForegroundBashObservation, RunnerOwnedOptions};
    use crate::core::detached_job_runner::{
        DetachedJobArtifacts, DetachedJobLifecycleController, DetachedJobLifecycleControllerOptions,
        LaunchedDetachedBashJob,
    };
    use crate::core::lifecycle_coordinator::{LifecycleCoordinator, LifecycleCoordinatorOptions};
    use crate::core::multi_agent_store::{MultiAgentStore, MultiAgentStoreOptions};
    use crate::core::runtime_process::read_process_identity;
    use pi_agent_core::agent::ToolError;
    use pi_js_compat::js_json::JsString;
    use std::sync::Arc;
    let root = tempfile::tempdir().unwrap();
    let cwd = JsString::from(root.path().to_str().unwrap());
    let mut child = Command::new("/usr/bin/sleep")
        .arg("10")
        .process_group(0)
        .spawn()
        .unwrap();
    let child_identity = read_process_identity(child.id()).unwrap();
    let control_db_path = cwd.clone() + "/control.sqlite";
    let session_path = cwd.clone() + "/session.jsonl";
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
            session_path,
            coordinator,
            store: Arc::new(MultiAgentStore::new(MultiAgentStoreOptions::default())),
            owner_session_id: "main".into(),
            owner_agent_id: None,
            write_bash_launch_manifest: None,
        },
    ));
    let runner = LaunchedDetachedBashJob {
        activation_path: cwd.clone() + "/activation.json",
        artifacts: DetachedJobArtifacts {
            directory: cwd.clone(),
            output_path: cwd.clone() + "/output.log",
        },
        // A real directory produces the source readFileSync EISDIR boundary.
        foreground_completion_path: cwd.clone(),
        job_id: "job".into(),
        manifest_path: cwd.clone() + "/launch.json",
        process_identity: child_identity,
        runner_pid: child.id(),
    };
    let options = RunnerOwnedOptions {
        detach: BashDetachOptions {
            artifacts: None,
            job_id: None,
            signal: Default::default(),
            adopt: None,
        },
        env: Default::default(),
        lifecycle,
        on_activated: Arc::new(|| {}),
        on_data: Arc::new(|_| Ok(())),
        shell_path: None,
        signal: None,
        timeout: None,
        tool_call_id: "probe".into(),
    };
    let observation = ForegroundBashObservation::default();
    *observation.activation.lock().unwrap() = Some(Err(ToolError::new("prior activation failed")));
    let outcome = crate::utils::uncaught::catch_panics(|| observation.activate_on_detach(&runner, &cwd, &options));
    child.kill().unwrap();
    child.wait().unwrap();
    let payload = outcome
        .expect_err("a retained activation error is not a successful activation and cannot suppress the next read");
    let error = payload.downcast::<pi_js_compat::node_error::NativeNodeError>().unwrap();
    assert_eq!(error.message, "EISDIR: illegal operation on a directory, read");
}
