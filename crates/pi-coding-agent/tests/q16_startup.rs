use pi_ai::request::BoxFuture;
use pi_coding_agent::core::resident_console_transport::{ResidentConsoleIdentity, ResidentManagedBy};
use pi_coding_agent::supervisor::ensure_running::*;
use std::sync::{Arc, Mutex};

fn identity(version: &str, manager: ResidentManagedBy) -> ResidentConsoleIdentity {
    ResidentConsoleIdentity {
        version: version.into(),
        pid: 123.into(),
        executable: "/fake/pi".into(),
        entrypoint: None,
        instance_id: version.into(),
        managed_by: manager,
        ready: true,
    }
}

#[tokio::test]
async fn startup_launches_once_and_rejects_external_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let state = Arc::new(Mutex::new(None));
    let launches = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut deps = EnsureSupervisorRunningDependencies::default();
    let observed = state.clone();
    deps.probe = Arc::new(move |_| {
        let value = observed.lock().unwrap().clone();
        Box::pin(async move { Ok(value) })
    });
    let running = state.clone();
    let count = launches.clone();
    deps.launch = Arc::new(move |_| {
        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *running.lock().unwrap() = Some(identity(&pi_coding_agent::config::version(), ResidentManagedBy::Pi));
        Box::pin(async { Ok(()) })
    });
    deps.terminate = Arc::new(|_| {
        Box::pin(async { Err("must not terminate".to_owned()) }) as BoxFuture<'static, Result<(), String>>
    });
    let input = EnsureSupervisorRunningInput {
        control_db_path: dir.path().join("control.sqlite").display().to_string(),
        launch_invocation: Some(SupervisorLaunchInvocation {
            command: "/fake/pi".into(),
            args: vec!["supervisor".into()],
            cwd: "/fake".into(),
            env: Default::default(),
        }),
        poll_interval_ms: Some(1),
        startup_timeout_ms: Some(100),
    };
    let (a, b) = tokio::join!(
        ensure_supervisor_running(&input, &deps),
        ensure_supervisor_running(&input, &deps)
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(launches.load(std::sync::atomic::Ordering::SeqCst), 1);
    *state.lock().unwrap() = Some(identity("old", ResidentManagedBy::External));
    assert!(
        ensure_supervisor_running(&input, &deps)
            .await
            .unwrap_err()
            .contains("externally managed")
    );
    assert!(!std::path::Path::new(&get_supervisor_start_lock_path(&input.control_db_path)).exists());
}

#[test]
fn launch_invocation_matches_compiled_and_script_runtimes() {
    let mut runtime = SupervisorLaunchRuntime {
        argv: vec!["node".into(), "cli.ts".into()],
        exec_argv: vec!["--import".into(), "tsx".into()],
        exec_path: "node".into(),
        home_dir: "/home/test".into(),
        is_compiled_binary: false,
        env: Default::default(),
    };
    let invocation = resolve_supervisor_launch_invocation(&runtime).unwrap();
    assert_eq!(invocation.args, vec!["--import", "tsx", "cli.ts", "supervisor"]);
    assert_eq!(invocation.env.get(SUPERVISOR_AUTOSTART_ENV).unwrap(), "1");
    runtime.is_compiled_binary = true;
    assert_eq!(
        resolve_supervisor_launch_invocation(&runtime).unwrap().args,
        vec!["supervisor"]
    );
    runtime.is_compiled_binary = false;
    runtime.argv.truncate(1);
    assert!(resolve_supervisor_launch_invocation(&runtime).is_err());
}
