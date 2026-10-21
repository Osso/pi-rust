#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::{date, process_env};
use pi_tui::{ProcessTerminal, Terminal};
use serde_json::json;

#[test]
fn write_log_filename_date_and_failure_side_effects_match_frozen() {
    let mut oracle = oracle::TsOracle::spawn("tui-core-oracle.ts", "packages/tui/src");
    let expected = oracle.request(&json!({"id":"log", "operation":"terminal_write_log"}));
    assert_eq!(expected["ok"], true, "{expected}");
    let directory = tempfile::tempdir().unwrap();
    process_env::set("PI_TUI_WRITE_LOG", directory.path().to_str().unwrap());
    let before = date::js_now_millis() as i64;
    let mut terminal = ProcessTerminal::with_output(Box::new(Vec::<u8>::new()));
    terminal.write("payload\n");
    let after = date::js_now_millis() as i64;
    let entry = std::fs::read_dir(directory.path()).unwrap().next().unwrap().unwrap();
    let filename = entry.file_name().into_string().unwrap();
    let pattern = regex::Regex::new(r"^tui-(\d{4}-\d{2}-\d{2})_(\d{2})-(\d{2})-(\d{2})-(\d+)\.log$").unwrap();
    let captures = pattern.captures(&filename).expect("frozen local-date log filename");
    let timestamp = format!("{}T{}:{}:{}", &captures[1], &captures[2], &captures[3], &captures[4]);
    let millis = date::date_parse(&timestamp) as i64;
    assert!(
        millis >= before.div_euclid(1000) * 1000 && millis <= after.div_euclid(1000) * 1000,
        "log timestamp {timestamp} not local now"
    );
    assert_eq!(&captures[5], std::process::id().to_string());
    let contents = std::fs::read_to_string(entry.path()).unwrap();
    let missing = directory.path().join("absent/log.txt");
    process_env::set("PI_TUI_WRITE_LOG", missing.to_str().unwrap());
    ProcessTerminal::with_output(Box::new(Vec::<u8>::new())).write("ignored log error");
    process_env::remove("PI_TUI_WRITE_LOG");
    assert_eq!(
        json!({"normalized":"tui-TIME-PID.log", "contents":contents, "createdMissingParent":missing.parent().unwrap().exists()}),
        expected["output"]
    );
}
