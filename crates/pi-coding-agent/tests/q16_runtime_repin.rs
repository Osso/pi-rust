#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::cli::args::Args;
use pi_coding_agent::core::self_restart::{
    SelfRestartHandoff, apply_self_restart_request, consume_self_restart_request,
};
use pi_coding_agent::core::session_control_db::SupervisorRequestKind;
use pi_coding_agent::supervisor::service::parse_supervisor_response;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
fn project_remote_and_unreadable_configuration_match_frozen() {
    use pi_coding_agent::supervisor::project_resolver::extract_remote_repository_identity;
    let cases = json!([
        ["https://host"],
        ["ssh://server/owner/repo.git/"],
        ["owner/repo"],
        ["\u{feff}git@host:owner/repo.git\u{feff}"]
    ]);
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/supervisor/project-resolver.ts",
    );
    let expected = ts.request(&json!({"id":10,"operation":"extractRemoteRepositoryIdentity","cases":cases}));
    assert_eq!(expected["ok"], true, "{expected}");
    let actual: Vec<Value> = cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| json!(extract_remote_repository_identity(case[0].as_str().unwrap())))
        .collect();
    assert_eq!(json!(actual), expected["output"]);
}

#[test]
fn unreadable_project_config_is_not_an_empty_configuration() {
    use pi_coding_agent::supervisor::project_resolver::read_supervisor_project_config;
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/supervisor/project-resolver.ts",
    );
    let directory = tempfile::tempdir().unwrap();
    let expected = ts.request(&json!({"id":11,"operation":"readSupervisorProjectConfig","cases":[[directory.path()]]}));
    assert_eq!(expected["ok"], false, "{expected}");
    assert!(read_supervisor_project_config(directory.path()).is_err());
}

#[tokio::test]
async fn architect_spec_reading_rejects_dotdot_prefix_and_follows_project_symlinks() {
    use pi_coding_agent::architect::project_spec::read_project_spec;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    let specs = root.join("docs/specs");
    std::fs::create_dir_all(&specs).unwrap();
    std::fs::write(specs.join("README.md"), "index").unwrap();
    std::fs::write(specs.join("..private.md"), "hidden").unwrap();
    let alias = directory.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/architect/project-spec.ts",
    );
    let expected = ts.request(&json!({"id":12,"operation":"readProjectSpec","cases":[[alias,"README.md"]]}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(
        read_project_spec(&alias.display().to_string(), "README.md")
            .await
            .unwrap(),
        expected["output"][0]
    );
}

#[tokio::test]
async fn architect_spec_dotdot_prefixed_filename_is_rejected() {
    use pi_coding_agent::architect::project_spec::read_project_spec;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    let specs = root.join("docs/specs");
    std::fs::create_dir_all(&specs).unwrap();
    std::fs::write(specs.join("README.md"), "index").unwrap();
    std::fs::write(specs.join("..private.md"), "hidden").unwrap();
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/architect/project-spec.ts",
    );
    let expected = ts.request(&json!({"id":13,"operation":"readProjectSpec","cases":[[root,"..private.md"]]}));
    assert_eq!(expected["ok"], false, "{expected}");
    assert_eq!(
        read_project_spec(&root.display().to_string(), "..private.md")
            .await
            .unwrap_err(),
        expected["error"]["message"]
    );
}

#[test]
fn empty_content_does_not_hide_a_structured_supervisor_response() {
    let cases = json!([
        ["approval_review", {"kind":"approve","reason":"ok","content":[{"type":"text","text":""}]}],
        ["approval_review", {"kind":"approve","reason":"ok","content":[{"type":"text","text":" "}]}]
    ]);
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/supervisor/service.ts",
    );
    let response = ts.request(&json!({"id":1,"operation":"parseSupervisorResponse","cases":cases}));
    assert_eq!(response["ok"], true, "{response}");
    let actual: Vec<Value> = cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            serde_json::to_value(parse_supervisor_response(
                SupervisorRequestKind::ApprovalReview,
                &case[1],
            ))
            .unwrap()
        })
        .collect();
    assert_eq!(json!(actual), response["output"]);
}

#[test]
fn restart_rejects_empty_session_and_restores_saved_model_selection() {
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/core/self-restart.ts",
    );
    let env = json!({"PI_SELF_RESTART_REQUEST":"1", "PI_SELF_RESTART_SESSION":""});
    let response = ts.request(&json!({"id":2,"operation":"consumeSelfRestartRequest","cases":[[env,100,42]]}));
    assert_eq!(response["ok"], true, "{response}");
    let mut env: BTreeMap<String, String> = serde_json::from_value(env).unwrap();
    assert_eq!(
        serde_json::to_value(consume_self_restart_request(&mut env, 100, 42).map(|value| value.session_file)).unwrap(),
        response["output"][0]
    );
}

#[test]
fn restart_pending_results_match_frozen() {
    use pi_agent_core::agent::AgentMessage;
    use pi_coding_agent::core::self_restart::resolve_restart_interrupted_tool_calls;
    use pi_coding_agent::core::session_manager::SessionManager;
    let message = json!({"role":"assistant","content":[{"type":"toolCall","id":"a","name":"restart_self","arguments":{}},{"type":"toolCall","id":"b","name":"read","arguments":{"path":"file"}}],"api":"faux","provider":"faux","model":"test","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"toolUse","timestamp":0});
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/core/self-restart.ts",
    );
    let expected = ts.request(&json!({"id":4,"operation":"restartPending","cases":[[message]]}));
    assert_eq!(expected["ok"], true, "{expected}");
    let mut manager = SessionManager::in_memory(Some("/fixture"), None).unwrap();
    manager
        .append_message(serde_json::from_value::<AgentMessage>(message).unwrap())
        .unwrap();
    resolve_restart_interrupted_tool_calls(&mut manager).unwrap();
    let actual: Vec<Value> = manager
        .build_session_context()
        .messages
        .iter()
        .filter_map(|message| match message {
            AgentMessage::ToolResult(_) => {
                let mut value = serde_json::to_value(message).unwrap();
                value["timestamp"] = json!(0);
                Some(value)
            }
            _ => None,
        })
        .collect();
    assert_eq!(json!(actual), expected["output"][0]);
    let count = manager.get_entries().len();
    resolve_restart_interrupted_tool_calls(&mut manager).unwrap();
    assert_eq!(manager.get_entries().len(), count);
}

#[test]
fn restart_clears_model_overrides() {
    let mut args = Args {
        provider: Some("stale".into()),
        model: Some("old-model".into()),
        thinking: Some(pi_agent_core::harness::ThinkingLevel::High),
        ..Default::default()
    };
    apply_self_restart_request(
        &mut args,
        Some(&SelfRestartHandoff {
            session_file: "saved.jsonl".into(),
            prompt: None,
            old_pid: None,
        }),
    );
    assert_eq!((args.provider, args.model, args.thinking), (None, None, None));
}

#[tokio::test]
async fn architect_specs_decode_node_utf8_instead_of_rejecting_file_bytes() {
    use pi_coding_agent::architect::project_spec::read_project_spec;
    let dir = tempfile::tempdir().unwrap();
    let specs = dir.path().join("docs/specs");
    std::fs::create_dir_all(&specs).unwrap();
    std::fs::write(specs.join("README.md"), "index").unwrap();
    std::fs::write(specs.join("bytes.md"), [0x41, 0xf0, 0x80, 0x80, 0x42, 0xff]).unwrap();
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/architect/project-spec.ts",
    );
    let expected = ts.request(&json!({"id":20,"operation":"readProjectSpec","cases":[[dir.path(),"bytes.md"]]}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(
        json!(
            read_project_spec(&dir.path().display().to_string(), "bytes.md")
                .await
                .unwrap()
        ),
        expected["output"][0]
    );
}

#[tokio::test]
async fn architect_root_discovery_accepts_a_non_unicode_index_like_node() {
    use pi_coding_agent::architect::project_spec::read_project_spec;
    let dir = tempfile::tempdir().unwrap();
    let specs = dir.path().join("docs/specs");
    std::fs::create_dir_all(&specs).unwrap();
    std::fs::write(specs.join("README.md"), [0xff]).unwrap();
    std::fs::write(specs.join("valid.md"), "body").unwrap();
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/architect/project-spec.ts",
    );
    let expected = ts.request(&json!({"id":21,"operation":"readProjectSpec","cases":[[dir.path(),"valid.md"]]}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(
        json!(
            read_project_spec(&dir.path().display().to_string(), "valid.md")
                .await
                .unwrap()
        ),
        expected["output"][0]
    );
}

#[tokio::test]
async fn architect_missing_spec_reports_the_node_realpath_error() {
    use pi_coding_agent::architect::project_spec::read_project_spec;
    let dir = tempfile::tempdir().unwrap();
    let specs = dir.path().join("docs/specs");
    std::fs::create_dir_all(&specs).unwrap();
    std::fs::write(specs.join("README.md"), "index").unwrap();
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/architect/project-spec.ts",
    );
    let expected = ts.request(&json!({"id":22,"operation":"readProjectSpec","cases":[[dir.path(),"missing.md"]]}));
    assert_eq!(expected["ok"], false, "{expected}");
    assert_eq!(
        json!(
            read_project_spec(&dir.path().display().to_string(), "missing.md")
                .await
                .unwrap_err()
        ),
        expected["error"]["message"]
    );
}
