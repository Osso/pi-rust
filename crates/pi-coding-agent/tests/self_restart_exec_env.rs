//! A failed self-restart exec must not disturb the process environment.
//!
//! `CommandExt::exec` temporarily swaps the process-global `environ` pointer
//! under a read lock; a concurrent `std::env::vars_os()` can then iterate the
//! temporary array after it is freed, yielding invalid entries. `exec_self_restart`
//! uses `execve` with an explicit environment, so the global environ is never
//! touched.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use pi_coding_agent::core::self_restart::{RestartTarget, SelfRestartRequest, exec_self_restart};

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
        session_file: "/tmp/session.jsonl".to_owned(),
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
