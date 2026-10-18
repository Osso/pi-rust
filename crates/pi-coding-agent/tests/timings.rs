#[path = "support/env_probe.rs"]
mod env_probe;

use std::ffi::OsString;

use pi_coding_agent::core::timings::{TimingLabel, print_timings, reset_timings, time};
use regex::Regex;

fn probe_stderr(timing: Option<&str>) -> String {
    let env: Vec<(&str, OsString)> = timing
        .map(|value| vec![("PI_TIMING", OsString::from(value))])
        .unwrap_or_default();
    let output = env_probe::run_probe("timings_probe_child", &env);
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn prints_each_namespace_in_first_use_order_with_totals_when_enabled() {
    let stderr = probe_stderr(Some("1"));
    let pattern = Regex::new(concat!(
        r"^\n--- Startup Timings: extensions ---\n  ext-load: (\d+)ms\n  TOTAL: (\d+)ms\n-{35}\n\n",
        r"\n--- Startup Timings: main ---\n  first: (\d+)ms\n  second: (\d+)ms\n  TOTAL: (\d+)ms\n-{29}\n\n$",
    ))
    .unwrap();
    let captures = pattern
        .captures(&stderr)
        .unwrap_or_else(|| panic!("unexpected timing output: {stderr:?}"));
    let value = |index: usize| captures[index].parse::<i64>().unwrap();
    assert_eq!(value(1), value(2));
    assert!(value(4) >= 20, "second label measures the sleep since the first");
    assert_eq!(value(3) + value(4), value(5));
}

#[test]
fn prints_nothing_unless_pi_timing_is_exactly_1() {
    assert_eq!(probe_stderr(None), "");
    assert_eq!(probe_stderr(Some("true")), "");
}

#[test]
#[ignore = "child process probe"]
fn timings_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    time("ext-load", TimingLabel::Extensions);
    reset_timings(TimingLabel::Main);
    time("first", TimingLabel::Main);
    std::thread::sleep(std::time::Duration::from_millis(25));
    time("second", TimingLabel::Main);
    print_timings();
}
