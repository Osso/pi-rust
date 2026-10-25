//! Regression coverage for the test-script spawn race.
//!
//! A script written with `fs::write` leaves its write descriptor open until the
//! write returns; a concurrent `fork` in another test thread inherits it and
//! `execve` then fails with `ETXTBSY`. `write_executable_script` writes through
//! a short-lived `sh` child, so the test process never holds the descriptor.
//! This test drives both sides of the race at full parallelism.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::shared_executable_script as executable_script;

const SCRIPT_BODY: &str = "#!/bin/sh\nexit 0\n";

fn spawn_storm(stop: Arc<AtomicBool>, failures: Arc<Mutex<Vec<String>>>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let status = Command::new("sh").arg("-c").arg("true").status().expect("storm spawn");
            if !status.success() {
                failures.lock().unwrap().push("storm spawn failed".to_owned());
            }
        }
    })
}

fn write_and_exec(directory: &Path, worker: usize, failures: &Mutex<Vec<String>>) {
    for iteration in 0..40 {
        let path = directory.join(format!("script-{worker}-{iteration}"));
        executable_script::write_executable_script(&path, SCRIPT_BODY);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SCRIPT_BODY);

        let output = Command::new(&path).output().expect("script exec");
        if !output.status.success() {
            failures.lock().unwrap().push(format!(
                "{} failed: status={:?} stderr={}",
                path.display(),
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
}

#[test]
fn concurrent_script_creation_and_spawning_never_reports_etxtbsy() {
    let directory = tempfile::tempdir().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let failures = Arc::new(Mutex::new(Vec::new()));

    let storm: Vec<_> = (0..4)
        .map(|_| spawn_storm(Arc::clone(&stop), Arc::clone(&failures)))
        .collect();

    let writers: Vec<_> = (0..4)
        .map(|worker| {
            let directory = directory.path().to_path_buf();
            let failures = Arc::clone(&failures);
            std::thread::spawn(move || write_and_exec(&directory, worker, &failures))
        })
        .collect();

    for writer in writers {
        writer.join().expect("writer thread");
    }
    stop.store(true, Ordering::SeqCst);
    for thread in storm {
        thread.join().expect("storm thread");
    }

    let failures = failures.lock().unwrap();
    assert!(failures.is_empty(), "spawn race failures: {failures:?}");
}
