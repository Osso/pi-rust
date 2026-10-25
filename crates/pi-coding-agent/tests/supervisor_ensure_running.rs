use crate::shared_oracle as oracle;
use pi_coding_agent::core::resident_console_transport::{ResidentConsoleIdentity, ResidentConsoleManagedBy};
use pi_coding_agent::supervisor::ensure_running::*;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn identity(version: impl Into<JsString>) -> ResidentConsoleIdentity {
    ResidentConsoleIdentity {
        version: version.into(),
        pid: 1234.0,
        executable: "/opt/pi".into(),
        entrypoint: None,
        instance_id: "instance".into(),
        managed_by: ResidentConsoleManagedBy::Pi,
        ready: true,
    }
}
#[test]
fn launch_invocation_matches_frozen() {
    let mut oracle = oracle::TsOracle::spawn(
        "supervisor-ensure-running-oracle.ts",
        "packages/coding-agent/src/supervisor/ensure-running.ts",
    );
    for compiled in [true, false] {
        let runtime = SupervisorLaunchRuntime {
            argv: vec!["/usr/bin/node".into(), "/package/dist/cli.js".into(), "ignored".into()],
            exec_argv: vec!["--enable-source-maps".into()],
            exec_path: "/usr/bin/node".into(),
            home_dir: "/home/user".into(),
            is_compiled_binary: compiled,
            env: json!({"PATH":"/usr/bin","PI_SUPERVISOR_AUTOSTARTED":"previous"})
                .as_object()
                .unwrap()
                .clone(),
        };
        let response =
            oracle.request(&json!({"id":0,"op":"invocation","runtime":js_json::to_value(&runtime).unwrap()}));
        assert_eq!(response["ok"], JsValue::Bool(true));
        assert_eq!(
            js_json::to_value(&resolve_supervisor_launch_invocation(&runtime).unwrap()).unwrap(),
            response["output"]
        );
    }
    let path = JsString::from_utf16(vec![0x2f, 0xd800]);
    let response = oracle.request(&json!({"id":1,"path":path}));
    assert_eq!(
        JsValue::String(get_supervisor_start_lock_path(&path)),
        response["output"]
    );
}
#[tokio::test]
async fn concurrent_starters_share_one_launch_and_release_the_lock() {
    let temp = tempfile::TempDir::new().unwrap();
    let path: JsString = temp.path().join("control.sqlite").to_str().unwrap().into();
    let input = EnsureSupervisorRunningInput {
        control_db_path: path.clone(),
        poll_interval_ms: Some(1.0),
        startup_timeout_ms: Some(500.0),
        launch_invocation: None,
    };
    let resident = Arc::new(Mutex::new(None));
    let launches = Arc::new(AtomicUsize::new(0));
    let probe_resident = resident.clone();
    let launch_resident = resident.clone();
    let launch_count = launches.clone();
    let dependencies = EnsureSupervisorRunningDependencies {
        probe: Some(Arc::new(move |_| {
            let resident = probe_resident.clone();
            Box::pin(async move { Ok(resident.lock().unwrap().clone()) })
        })),
        launch: Some(Arc::new(move |_| {
            let resident = launch_resident.clone();
            let count = launch_count.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                *resident.lock().unwrap() = Some(identity(pi_coding_agent::config::version()));
                Ok(())
            })
        })),
        terminate: None,
    };
    let (one, two) = tokio::join!(
        ensure_supervisor_running(&input, &dependencies),
        ensure_supervisor_running(&input, &dependencies)
    );
    assert_eq!(one.unwrap(), two.unwrap());
    assert_eq!(launches.load(Ordering::SeqCst), 1);
    assert!(
        !std::path::Path::new(&pi_js_compat::js_json::encode_node_utf8(
            &get_supervisor_start_lock_path(&path)
        ))
        .exists()
    );
}

struct OwnedResidents {
    path: std::path::PathBuf,
}
impl Drop for OwnedResidents {
    fn drop(&mut self) {
        if let Ok(launches) = std::fs::read_to_string(&self.path) {
            for line in launches.lines() {
                let record: serde_json::Value = serde_json::from_str(line).unwrap();
                if let Some(pid) = record["pid"]
                    .as_i64()
                    .and_then(|pid| rustix::process::Pid::from_raw(pid as i32))
                {
                    let command_line =
                        std::fs::read(format!("/proc/{}/cmdline", pid.as_raw_nonzero().get())).unwrap_or_default();
                    let owned_entry = self.path.parent().unwrap().join("packages/coding-agent/dist/cli.js");
                    let belongs_to_test = command_line
                        .split(|byte| *byte == 0)
                        .any(|arg| arg == owned_entry.as_os_str().as_encoded_bytes());
                    if belongs_to_test {
                        let _ = rustix::process::kill_process(pid, rustix::process::Signal::TERM);
                        for _ in 0..100 {
                            let still_running =
                                std::fs::read_to_string(format!("/proc/{}/stat", pid.as_raw_nonzero().get()))
                                    .is_ok_and(|stat| {
                                        !stat.split_once(") ").is_some_and(|(_, tail)| tail.starts_with('Z'))
                                    });
                            if !still_running {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        }
                    }
                }
            }
        }
    }
}
fn real_invocation(
    temp: &tempfile::TempDir,
    path: &JsString,
    version: impl Into<JsString>,
) -> SupervisorLaunchInvocation {
    let version = version.into();
    let entry = temp.path().join("packages/coding-agent/dist/cli.js");
    std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
    std::fs::write(&entry, include_str!("support/supervisor_resident.py")).unwrap();
    SupervisorLaunchInvocation {
        command: "/usr/bin/python3".into(),
        args: vec![entry.to_str().unwrap().into()],
        cwd: temp.path().to_str().unwrap().into(),
        env: json!({"TEST_CONTROL_DB":path,"TEST_VERSION":version,"PI_SUPERVISOR_AUTOSTARTED":"1"})
            .as_object()
            .unwrap()
            .clone(),
    }
}
#[tokio::test]
async fn real_detached_target_probes_reuses_and_replaces_owned_incompatible_process() {
    let temp = tempfile::TempDir::new().unwrap();
    let path: JsString = temp.path().join("control.sqlite").to_str().unwrap().into();
    let _owned = OwnedResidents {
        path: temp.path().join("control.sqlite.launches"),
    };
    let mut input = EnsureSupervisorRunningInput {
        control_db_path: path.clone(),
        poll_interval_ms: Some(5.0),
        startup_timeout_ms: Some(1000.0),
        launch_invocation: Some(real_invocation(&temp, &path, "old-version")),
    };
    // Let the real default launcher create an incompatible process, then observe its identity.
    let error = ensure_supervisor_running(&input, &Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.message, "Supervisor did not become ready within 1000ms");
    let old = pi_coding_agent::core::resident_console_transport::probe_resident_console::<JsValue>(
        pi_js_compat::js_format!("{}.supervisor-console.sock", path),
        pi_coding_agent::core::resident_console_transport::ResidentConsoleService::Supervisor,
    )
    .await
    .unwrap()
    .identity
    .unwrap();
    assert!(pi_coding_agent::core::runtime_process::is_verified_pi_runtime_process(
        old.pid as u32
    ));
    input.launch_invocation = Some(real_invocation(&temp, &path, pi_coding_agent::config::version()));
    let current = ensure_supervisor_running(&input, &Default::default()).await.unwrap();
    assert_ne!(old.instance_id, current.instance_id);
    for _ in 0..100 {
        if !pi_coding_agent::core::runtime_process::is_verified_pi_runtime_process(old.pid as u32) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(!pi_coding_agent::core::runtime_process::is_verified_pi_runtime_process(
        old.pid as u32
    ));
    assert_eq!(
        ensure_supervisor_running(&input, &Default::default()).await.unwrap(),
        current
    );
    let launches = std::fs::read_to_string(temp.path().join("control.sqlite.launches")).unwrap();
    assert_eq!(launches.lines().count(), 2);
    for line in launches.lines() {
        let record: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(record["pid"], record["sid"]);
        assert_eq!(record["cwd"], temp.path().to_str().unwrap());
        assert_eq!(record["autostart"], "1");
    }
}
#[tokio::test]
#[ignore = "owned cross-process startup test entrypoint"]
async fn startup_process_child() {
    let path: JsString = std::env::var("TEST_CONTROL_DB").unwrap().into();
    let invocation_path = std::env::var("TEST_INVOCATION_PATH").unwrap();
    let value = js_json::parse_lossless(std::fs::read_to_string(invocation_path).unwrap()).unwrap();
    let invocation = js_json::from_value::<SupervisorLaunchInvocation>(&value).unwrap();
    let input = EnsureSupervisorRunningInput {
        control_db_path: path,
        launch_invocation: Some(invocation),
        poll_interval_ms: Some(5.0),
        startup_timeout_ms: Some(1000.0),
    };
    ensure_supervisor_running(&input, &Default::default()).await.unwrap();
}
#[test]
fn separate_process_starters_recover_stale_lock_and_launch_once() {
    let temp = tempfile::TempDir::new().unwrap();
    let path: JsString = temp.path().join("control.sqlite").to_str().unwrap().into();
    let _owned = OwnedResidents {
        path: temp.path().join("control.sqlite.launches"),
    };
    let invocation = real_invocation(&temp, &path, pi_coding_agent::config::version());
    let invocation_path = temp.path().join("invocation.json");
    std::fs::write(
        &invocation_path,
        js_json::stringify_lossless(&js_json::to_value(&invocation).unwrap()),
    )
    .unwrap();
    let lock_path = temp.path().join("control.sqlite.supervisor-start.lock");
    std::fs::create_dir(&lock_path).unwrap();
    let stale = std::time::SystemTime::now() - std::time::Duration::from_secs(10);
    std::fs::File::open(&lock_path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(stale))
        .unwrap();
    let spawn = || {
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "startup_process_child", "--ignored", "--nocapture"])
            .env("TEST_CONTROL_DB", pi_js_compat::js_json::encode_node_utf8(&path))
            .env("TEST_INVOCATION_PATH", &invocation_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let first = spawn();
    let second = spawn();
    for child in [first, second] {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        std::fs::read_to_string(temp.path().join("control.sqlite.launches"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(!lock_path.exists());
}

#[tokio::test]
async fn missing_and_nonexecutable_target_errors_are_reported_before_readiness() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::TempDir::new().unwrap();
    let path: JsString = temp.path().join("control.sqlite").to_str().unwrap().into();
    let target = temp.path().join("target");
    let mut invocation = real_invocation(&temp, &path, pi_coding_agent::config::version());
    invocation.command = target.to_str().unwrap().into();
    let input = EnsureSupervisorRunningInput {
        control_db_path: path,
        launch_invocation: Some(invocation.clone()),
        poll_interval_ms: Some(1.0),
        startup_timeout_ms: Some(50.0),
    };
    assert_eq!(
        ensure_supervisor_running(&input, &Default::default())
            .await
            .unwrap_err()
            .message,
        pi_js_compat::js_format!("Could not start Supervisor: spawn {} ENOENT", invocation.command)
    );
    std::fs::write(&target, "not executable").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        ensure_supervisor_running(&input, &Default::default())
            .await
            .unwrap_err()
            .message,
        pi_js_compat::js_format!("Could not start Supervisor: spawn {} EACCES", invocation.command)
    );
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
