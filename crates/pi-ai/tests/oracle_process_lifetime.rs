//! Generic oracle process/flag proof, independent from removed product modules.
use serde_json::json;
use std::{fs, process::Command};
#[path = "support/oracle.rs"]
mod oracle;
use oracle::TsOracle;
const MODULE: &str = "packages/ai/src/types.ts";

#[test]
fn oracle_flag_watchdog_owner() {
    if std::env::var_os("PI_MODEL_DATA_WATCHDOG_OWNER").is_none() {
        return;
    }
    let mut oracle = TsOracle::spawn_with_environment(
        "oracle-process-lifetime.mts",
        MODULE,
        &[("NODE_OPTIONS", "--experimental-vm-modules".into())],
    );
    let response = oracle.request(&json!({"op":"watchdog_probe"}));
    assert_eq!(response["ok"], true);
    assert_eq!(response["output"]["callerFlag"], true);
    let pid = rustix::process::Pid::from_raw(response["output"]["pid"].as_i64().unwrap() as i32).unwrap();
    let group = rustix::process::getpgid(Some(pid)).unwrap();
    println!("WATCHDOG_ORACLE_PID={}", pid.as_raw_nonzero());
    println!("WATCHDOG_GROUP_PID={}", group.as_raw_nonzero());
    // Deliberately bypass Drop: the watchdog must reap the orphan process group.
    std::process::exit(0);
}

fn process_running(pid: i32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Ok(stat) => !stat.split_once(") ").is_some_and(|(_, tail)| tail.starts_with('Z')),
        Err(error) => panic!("cannot inspect oracle lifetime: {error}"),
    }
}

#[test]
fn caller_node_flag_and_parent_death_watchdog_both_work() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "oracle_flag_watchdog_owner", "--nocapture"])
        .env("PI_MODEL_DATA_WATCHDOG_OWNER", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let pid: i32 = stdout
        .lines()
        .find_map(|line| line.strip_prefix("WATCHDOG_ORACLE_PID="))
        .expect("oracle pid")
        .parse()
        .unwrap();
    let group: i32 = stdout
        .lines()
        .find_map(|line| line.strip_prefix("WATCHDOG_GROUP_PID="))
        .expect("oracle process-group leader pid")
        .parse()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while process_running(pid) || process_running(group) {
        if std::time::Instant::now() >= deadline {
            use rustix::process::{Pid, Signal, getpgid, kill_process, kill_process_group};
            let pid = Pid::from_raw(pid).unwrap();
            if let Ok(group) = getpgid(Some(pid)) {
                let _ = kill_process_group(group, Signal::KILL);
            }
            let _ = kill_process(pid, Signal::KILL);
            panic!("caller flag worked but parent watchdog failed to terminate orphan oracle");
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}
