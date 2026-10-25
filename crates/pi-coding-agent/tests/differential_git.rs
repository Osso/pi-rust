use crate::shared_oracle as oracle;

use pi_coding_agent::utils::git::parse_git_url;
use pi_js_compat::js_json::JsString;
use serde_json::{Value, json};

const REQUEST_ID: u64 = 26_008;

const SOURCES: [&str; 55] = [
    "https://github.com/user/repo",
    "https://github.com/user/repo@v1.0.0",
    "https://github.com/user/repo.git",
    "https://github.com/user/repo/tree/main",
    "https://github.com/user/repo#readme",
    "https://github.com/user/repo@main#x",
    "https://github.com/user/repo@",
    "https://github.com/user",
    "https://github.com/",
    "https://github.com/user/repo/",
    "https://www.github.com/user/repo",
    "https://github.com:443/user/repo",
    "https://user:pass@github.com/user/repo",
    "HTTPS://github.com/user/repo",
    "http://github.com/user/repo",
    "ssh://git@github.com/user/repo.git",
    "git://github.com/user/repo",
    "git+https://github.com/user/repo",
    "git+ssh://git@github.com/user/repo.git",
    "git@github.com:user/repo",
    "github.com/user/repo",
    "user/repo",
    "https://gitlab.com/group/sub/repo",
    "https://gitlab.com/group/sub/repo@v2",
    "https://gitlab.com/group/sub/repo/-/tree/main",
    "https://bitbucket.org/user/repo",
    "https://bitbucket.org/user/repo/get/main.tar.gz",
    "https://gist.github.com/user/abc123",
    "https://gist.github.com/abc123",
    "https://gist.github.com/user/abc123/raw/file.txt",
    "https://git.sr.ht/~user/repo",
    "https://git.sr.ht/~user/repo/archive/main.tar.gz",
    "https://example.com/user/repo",
    "https://example.com/user/repo@v1",
    "git:git@github.com:user/repo",
    "git:github.com/user/repo",
    "git:git@github.com:user/repo@v1.0.0",
    "git:user/repo",
    "git:github.com/user/repo@v1",
    "git:example.com/user/repo",
    "git:localhost/user/repo",
    "git:example/user/repo",
    "git:github.com/user/repo#branch",
    "git:github.com/user/repo@feature/x",
    "git:git@github.com:user/repo@feature/x",
    "git:https://example.com/user/repo",
    "git:git@evil.example:../../victim/repo",
    "https://evil.example/..%2F..%2Fvictim/repo",
    "https://evil.example/..%2F..%2Fvictim/repo%",
    "git:git@evil.example:/absolute/repo",
    "git:git@evil.example:user\\repo/name",
    "git:git@evil.example:user/repo\0name",
    "  git:github.com/user/repo  ",
    "git:   ",
    "",
];

#[test]
fn coding_agent_git_matches_the_pinned_typescript_source() {
    let mut oracle = oracle::TsOracle::spawn("coding-agent-git-oracle.ts", "packages/coding-agent/src/utils/git.ts");
    let expected = oracle.request(&request());
    let actual = response();

    compare_results(&actual, &expected, "coding-agent git");
}

fn compare_results(actual: &Value, expected: &Value, label: &str) {
    let actual_results = actual["output"]["results"].as_array().expect("actual results");
    let expected_results = expected["output"]["results"].as_array().expect("expected results");
    assert_eq!(actual_results.len(), expected_results.len(), "{label} result count");
    for (actual_case, expected_case) in actual_results.iter().zip(expected_results) {
        assert_eq!(
            actual_case, expected_case,
            "{label} mismatch: {}",
            expected_case["name"]
        );
    }
}

fn request() -> Value {
    let cases = SOURCES
        .iter()
        .enumerate()
        .map(|(index, source)| json!({"name": format!("case {index}"), "source": source}))
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "operation": "coding_agent_git", "cases": cases})
}

fn response() -> Value {
    let results = SOURCES
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let value = parse_git_url(&JsString::from(*source))
                .map(|parsed| serde_json::to_value(parsed).expect("GitSource must serialize"))
                .unwrap_or(Value::Null);
            json!({"name": format!("case {index}"), "value": value})
        })
        .collect::<Vec<_>>();
    json!({"id": REQUEST_ID, "ok": true, "output": {"results": results}})
}
