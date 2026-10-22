//! A failed self-restart exec must not disturb the process environment.
//!
//! `CommandExt::exec` temporarily swaps the process-global `environ` pointer
//! under a read lock; a concurrent `std::env::vars_os()` can then iterate the
//! temporary array after it is freed, yielding invalid entries. `exec_self_restart`
//! uses `execve` with an explicit environment, so the global environ is never
//! touched.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use pi_coding_agent::core::self_restart::{RestartTarget, SelfRestartRequest, exec_self_restart, spawn_self_restart};

const NON_UNICODE_CHILD: &str = "PI_SELF_RESTART_NON_UNICODE_CHILD";

fn request() -> SelfRestartRequest {
    SelfRestartRequest {
        session_file: JsString::from("/tmp/session.jsonl"),
        prompt: None,
        old_pid: None,
    }
}

/// Child mode: a non-UTF-8 process environment must not panic the restart path.
#[test]
fn non_unicode_environment_child() {
    if std::env::var(NON_UNICODE_CHILD).is_err() {
        return;
    }
    let target = RestartTarget {
        argv: vec!["/bin/sh".to_owned(), "-c".to_owned(), "exit 0".to_owned()],
        executable: Some("/bin/sh".to_owned()),
        ..RestartTarget::default()
    };
    let exit_code = spawn_self_restart(&request(), &target, true).expect("restart must spawn");
    assert_eq!(exit_code, 0);
}

#[test]
fn spawns_a_restart_when_the_environment_has_non_unicode_entries() {
    use std::os::unix::ffi::OsStringExt;

    let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", "non_unicode_environment_child", "--nocapture"])
        .env(NON_UNICODE_CHILD, "1")
        .env(
            "PI_SELF_RESTART_NON_UNICODE_VALUE",
            std::ffi::OsString::from_vec(vec![0xff]),
        )
        .output()
        .expect("non-Unicode restart child must run");
    assert!(
        output.status.success(),
        "restart child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn is_valid_environment_entry(key: &std::ffi::OsStr, value: &std::ffi::OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;

    std::str::from_utf8(key.as_bytes()).is_ok() && std::str::from_utf8(value.as_bytes()).is_ok()
}

#[test]
fn failed_self_restart_exec_never_corrupts_the_environment() {
    let stop = Arc::new(AtomicBool::new(false));
    let corrupt = Arc::new(AtomicUsize::new(0));
    let reader = {
        let stop = Arc::clone(&stop);
        let corrupt = Arc::clone(&corrupt);
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                for (key, value) in std::env::vars_os() {
                    if !is_valid_environment_entry(&key, &value) {
                        corrupt.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        })
    };

    let target = RestartTarget {
        executable: Some("/definitely/missing/pi".to_owned()),
        ..RestartTarget::default()
    };
    let request = SelfRestartRequest {
        session_file: JsString::from("/tmp/session.jsonl"),
        prompt: None,
        old_pid: None,
    };
    for _ in 0..5_000 {
        let error = exec_self_restart(&request, &target);
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    stop.store(true, Ordering::Relaxed);
    reader.join().expect("environment reader");
    assert_eq!(
        corrupt.load(Ordering::Relaxed),
        0,
        "concurrent environment reads observed invalid entries"
    );
}
