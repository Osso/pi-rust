use crate::shared_oracle as oracle;
use pi_agent_core::harness::env::{NodeExecutionEnv, NodeExecutionEnvOptions};
use pi_agent_core::harness::session::{JsonlSessionListOptions, JsonlSessionRepo, JsonlSessionRepoOptions};
use pi_agent_core::harness::types::OperationFailure;
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue, stringify_lossless},
};
use std::sync::Arc;

fn header(id: JsString, date: &str, cwd: &str) -> JsValue {
    json!({"type":"session","version":3,"id":id,"timestamp":date,"cwd":cwd})
}
fn write_header(path: &std::path::Path, value: JsValue, tail: &str) {
    std::fs::write(path, format!("{}\n{tail}", stringify_lossless(&value))).unwrap();
}
fn populate(root: &std::path::Path) {
    let sessions = root.join("sessions");
    let fixture = sessions.join("--fixture--");
    let other = sessions.join("unexpected-name");
    std::fs::create_dir_all(&fixture).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    for index in 0..80 {
        let date = if index % 5 == 0 {
            "not a date".to_owned()
        } else {
            format!("20{:02}-01-01T00:00:00.000Z", (index * 37) % 36)
        };
        let id = JsString::from(format!("id-{index}")) + JsString::from_utf16(vec![0xd800]);
        let cwd = if index == 1 { "/misplaced-header" } else { "/fixture" };
        write_header(&fixture.join(format!("{index:03}.jsonl")), header(id, &date, cwd), "");
    }
    for (name, content) in [
        ("invalid.jsonl", "not json\n"),
        ("empty.jsonl", "\n"),
        ("version.jsonl", r#"{"type":"session","version":4}"#),
        (
            "parent.jsonl",
            r#"{"type":"session","version":3,"id":"bad","timestamp":"t","cwd":"/","parentSession":null}"#,
        ),
    ] {
        std::fs::write(fixture.join(name), content).unwrap();
    }
    std::fs::create_dir(fixture.join("directory.jsonl")).unwrap();
    write_header(
        &fixture.join("invalid-body.jsonl"),
        header("header-only".into(), "2020-01-01T00:00:00.000Z", "/fixture"),
        "not JSON\n",
    );
    write_header(
        &fixture.join("ignored.JSONL"),
        header("ignored".into(), "2020-01-01T00:00:00.000Z", "/fixture"),
        "",
    );
    write_header(
        &fixture.join("ignored.jsonl.tmp"),
        header("ignored".into(), "2020-01-01T00:00:00.000Z", "/fixture"),
        "",
    );
    write_header(
        &sessions.join("root-file.jsonl"),
        header("ignored-root-file".into(), "2020-01-01T00:00:00.000Z", "/fixture"),
        "",
    );
    write_header(
        &other.join("other.jsonl"),
        header("other".into(), "2021-01-01T00:00:00.000Z", "/other"),
        "",
    );
    std::os::unix::fs::symlink("002.jsonl", fixture.join("link.jsonl")).unwrap();
    std::os::unix::fs::symlink("--fixture--", sessions.join("linked-dir")).unwrap();
}

#[tokio::test]
async fn jsonl_inventory_filters_only_invalid_headers_and_matches_node_inconsistent_date_sort() {
    let root = tempfile::tempdir().unwrap();
    populate(root.path());
    let mut oracle = oracle::TsOracle::spawn(
        "session-inventory-oracle.ts",
        "packages/agent-core/src/harness/session/jsonl-repo.ts",
    );
    for cwd in [None, Some("/fixture"), Some("/missing"), Some("")] {
        let input = json!({"root":root.path(),"cwd":cwd});
        let expected = oracle.request(&input);
        let repo = JsonlSessionRepo::new(JsonlSessionRepoOptions {
            fs: Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
                cwd: root.path().to_str().unwrap().into(),
                ..Default::default()
            })),
            sessions_root: root.path().join("sessions").to_str().unwrap().into(),
        });
        let actual = match repo
            .list(JsonlSessionListOptions {
                cwd: cwd.map(JsString::from),
            })
            .await
        {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(OperationFailure::Returned(error)) => {
                json!({"ok":false,"error":{"name":"SessionError","code":error.code.as_str(),"message":error.message}})
            }
            Err(OperationFailure::Rejected(error)) => panic!("unexpected rejection: {error:?}"),
        };
        assert_eq!(actual, expected, "cwd={cwd:?}");
    }
}
