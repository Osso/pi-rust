use crate::shared_oracle as oracle;

use pi_coding_agent::utils::terminal_current_directory::format_terminal_current_directory_sequence;
use serde_json::{Value, json};

const REQUEST_ID: u64 = 26_006;

const CWD: [&str; 12] = [
    "/syncthing/Sync/Projects/wow/wow-ui-sim",
    "/tmp/project with spaces/#branch",
    "/tmp/caf\u{e9} 100%/x~y",
    "/tmp/a/../b:@!$&'()*+,;=",
    "/",
    "/tmp/",
    "/tmp//double//slash",
    "/tmp/a/./b",
    "/tmp/a/b/../../c",
    "/tmp/[]{}|\\^`\"<>",
    "/tmp/%2F%25",
    "/tmp/\u{4f60}\u{597d}",
];

#[test]
fn coding_agent_terminal_current_directory_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-terminal-current-directory-oracle.ts",
        "packages/coding-agent/src/utils/terminal-current-directory.ts",
    );
    let expected = oracle.request(&request());
    let actual = response();

    assert_eq!(actual, expected, "coding-agent terminal-current-directory mismatch");
}

fn request() -> Value {
    let cases = CWD
        .iter()
        .enumerate()
        .map(|(index, cwd)| json!({"name": format!("case {index}"), "cwd": cwd}))
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "operation": "coding_agent_terminal_current_directory", "cases": cases})
}

fn response() -> Value {
    let results = CWD
        .iter()
        .enumerate()
        .map(|(index, cwd)| {
            json!({
                "name": format!("case {index}"),
                "value": format_terminal_current_directory_sequence(&(*cwd).into()).unwrap().to_utf8().unwrap(),
            })
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "ok": true, "output": {"results": results}})
}
