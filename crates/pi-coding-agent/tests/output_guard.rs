#[path = "support/env_probe.rs"]
mod env_probe;

use std::ffi::OsString;

use pi_coding_agent::core::output_guard::{
    flush_raw_stdout, is_stdout_taken_over, restore_stdout, take_over_stdout, wait_for_raw_stdout_backpressure,
    write_raw_stdout,
};

const PREFIX: &str = "OUTPUT_GUARD_PROBE:";

fn probe() -> (String, String) {
    let output = env_probe::run_probe("output_guard_probe_child", &[] as &[(&str, OsString)]);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let states: Vec<&str> = stderr
        .lines()
        .filter_map(|line| line.split_once(PREFIX).map(|(_, rest)| rest))
        .collect();
    assert_eq!(states.len(), 1, "probe must report one state line: {stderr}");
    (stdout, states[0].to_owned())
}

#[test]
fn writes_raw_stdout_in_order_and_reports_takeover_state() {
    let (stdout, states) = probe();
    assert_eq!(states, "true,false,false,false");
    let lines = "one\ntwo\nthree";
    let long_line = format!("{}\n", "b".repeat(2050));
    let first = stdout.find(lines).expect("short text must be written");
    let second = stdout.find(&long_line).expect("long text must be written");
    assert!(first < second, "chunks must keep their order: {stdout:?}");
    assert_eq!(stdout.matches(&long_line).count(), 1, "no duplicated output");
}

#[test]
#[ignore = "child process probe"]
fn output_guard_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    take_over_stdout();
    let after_takeover = is_stdout_taken_over();
    write_raw_stdout("one\ntwo\nthree");
    restore_stdout();
    let after_restore = is_stdout_taken_over();
    restore_stdout();
    let after_second_restore = is_stdout_taken_over();
    write_raw_stdout(&format!("{}\n", "b".repeat(2050)));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("probe runtime must build");
    runtime.block_on(async {
        wait_for_raw_stdout_backpressure().await;
        flush_raw_stdout().await;
    });
    eprintln!(
        "{PREFIX}{after_takeover},{after_restore},{after_second_restore},{}",
        is_stdout_taken_over()
    );
}
