//! Oracle process lifetime: test children must not outlive the test process.
//!
//! `TsOracle` children are started with a preloaded watchdog
//! (`oracle/parent-watchdog.mjs`) that exits the Node process when the test
//! process disappears or when its tsx wrapper dies. These tests re-execute
//! their own binary as a holder that spawns a real oracle, then cover both
//! shutdown paths:
//!
//! - `SIGKILL` of the holder (no Rust `Drop` runs) — the watchdog sees the
//!   recorded test PID vanish.
//! - normal holder exit — `Drop` kills the tsx wrapper and the watchdog sees
//!   its parent change.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[path = "support/oracle.rs"]
#[allow(dead_code)]
mod oracle;

const HOLDER_ENV: &str = "PI_ORACLE_LIFECYCLE_HOLDER";
const PIDFILE_ENV: &str = "PI_ORACLE_LIFECYCLE_PIDFILE";
const MODE_ENV: &str = "PI_ORACLE_LIFECYCLE_MODE";
const GO_ENV: &str = "PI_ORACLE_LIFECYCLE_GO";
const HOLDER_TEST: &str = "oracle_holder_writes_pids";
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

fn proc_exists(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn process_parent(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let command_end = stat.rfind(')')?;
    let mut fields = stat.get(command_end + 1..)?.split_whitespace();
    fields.nth(1)?.parse().ok()
}

fn all_pids() -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
        .collect()
}

fn descendants(root: u32) -> Vec<u32> {
    let mut found = Vec::new();
    let mut pending = vec![root];
    while let Some(parent) = pending.pop() {
        for pid in all_pids() {
            if found.contains(&pid) {
                continue;
            }
            if process_parent(pid) == Some(parent) {
                found.push(pid);
                pending.push(pid);
            }
        }
    }
    found
}

fn wait_for_descendants(root: u32, timeout: Duration) -> Vec<u32> {
    let deadline = Instant::now() + timeout;
    loop {
        let descendants = descendants(root);
        if !descendants.is_empty() {
            return descendants;
        }
        assert!(Instant::now() < deadline, "oracle node child never started");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn read_pidfile(path: &Path, timeout: Duration) -> Vec<u32> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(contents) = std::fs::read_to_string(path) {
            let pids: Vec<u32> = contents.lines().filter_map(|line| line.parse().ok()).collect();
            if !pids.is_empty() {
                return pids;
            }
        }
        assert!(Instant::now() < deadline, "oracle holder never wrote its pid file");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn describe(pid: u32) -> String {
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let command = String::from_utf8_lossy(&cmdline).replace('\0', " ");
    let environ = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
    let environ = String::from_utf8_lossy(&environ).replace('\0', "\n");
    let relevant: Vec<&str> = environ
        .lines()
        .filter(|line| line.starts_with("NODE_OPTIONS=") || line.starts_with("PI_ORACLE_PARENT_PID="))
        .collect();
    format!("{pid} ppid={:?} {command} env={relevant:?}", process_parent(pid))
}

fn kill_recorded_tree(pids: &[u32]) {
    for pid in pids {
        let _ = Command::new("sh")
            .arg("-c")
            .arg(format!("kill -KILL -{pid} 2>/dev/null || kill -KILL {pid} 2>/dev/null"))
            .status();
    }
}

/// Holder mode: spawn a real oracle, publish the wrapper and descendant PIDs,
/// then either sleep until killed or return so `Drop` runs.
#[test]
fn oracle_holder_writes_pids() {
    if std::env::var(HOLDER_ENV).is_err() {
        return;
    }
    let pidfile = std::env::var(PIDFILE_ENV).expect("holder pid file env");
    let mode = std::env::var(MODE_ENV).expect("holder mode env");
    let oracle = oracle::TsOracle::spawn("lifecycle-oracle.ts", "packages/coding-agent/src");
    let wrapper = oracle.pid();
    let descendants = wait_for_descendants(wrapper, Duration::from_secs(120));
    let mut contents = format!("{wrapper}\n");
    for pid in &descendants {
        contents.push_str(&format!("{pid}\n"));
    }
    let mut file = std::fs::File::create(&pidfile).expect("pid file");
    file.write_all(contents.as_bytes()).expect("pid file contents");

    if mode == "kill" {
        std::thread::sleep(Duration::from_secs(300));
    } else {
        let go = std::env::var(GO_ENV).expect("holder go file env");
        let deadline = Instant::now() + Duration::from_secs(120);
        while !Path::new(&go).exists() {
            assert!(Instant::now() < deadline, "holder never saw the go file");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn assert_oracle_tree_exits(mode: &str, kill_holder: bool) {
    if std::env::var(HOLDER_ENV).is_ok() {
        return;
    }
    let directory = tempfile::tempdir().expect("temp dir");
    let pidfile = directory.path().join("oracle-pids");
    let gofile = directory.path().join("go");
    let mut holder = Command::new(std::env::current_exe().expect("current test binary"))
        .arg("--exact")
        .arg(HOLDER_TEST)
        .arg("--nocapture")
        .env(HOLDER_ENV, "1")
        .env(MODE_ENV, mode)
        .env(PIDFILE_ENV, &pidfile)
        .env(GO_ENV, &gofile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("oracle holder must start");

    let pids = read_pidfile(&pidfile, Duration::from_secs(180));
    assert!(
        pids.iter().all(|pid| proc_exists(*pid)),
        "oracle process tree must be alive before shutdown"
    );

    if kill_holder {
        holder.kill().expect("holder must be killable");
    } else {
        std::fs::write(&gofile, b"go").expect("go file");
    }
    holder.wait().expect("holder must be reaped");

    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    loop {
        let survivors: Vec<u32> = pids.iter().copied().filter(|pid| proc_exists(*pid)).collect();
        if survivors.is_empty() {
            return;
        }
        if Instant::now() >= deadline {
            let survivors: Vec<String> = survivors.iter().map(|pid| describe(*pid)).collect();
            kill_recorded_tree(&pids);
            panic!("oracle processes survived {mode} shutdown: {survivors:?}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn oracle_process_tree_exits_when_the_test_process_is_killed() {
    assert_oracle_tree_exits("kill", true);
}

#[test]
fn oracle_process_tree_exits_when_the_holder_drops_the_oracle() {
    assert_oracle_tree_exits("drop", false);
}
