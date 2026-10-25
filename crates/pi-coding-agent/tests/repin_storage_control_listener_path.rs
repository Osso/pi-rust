use super::repin_storage_history as oracle;
use pi_coding_agent::core::session_control_db::{
    RuntimeMailboxAddress, RuntimeMailboxRegistrationOptions, read_runtime_mailbox_listener,
    register_runtime_mailbox_listener,
};
use pi_js_compat::js_json::JsString;
use serde_json::json;

#[test]
fn listener_path_assertion_distinguishes_absent_empty_and_real_paths() {
    let root = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let recipient = RuntimeMailboxAddress {
        session_id: "recipient".into(),
        agent_id: None,
    };
    for (index, path) in [None, Some(""), Some("/fixture/recipient.jsonl")]
        .into_iter()
        .enumerate()
    {
        let source_db = root.path().join(format!("source{index}.sqlite"));
        let native_db = JsString::from(root.path().join(format!("native{index}.sqlite")).to_str().unwrap());
        let mut args = vec![json!(source_db), json!(recipient), json!(std::process::id())];
        if let Some(path) = path {
            args.push(json!(path));
        }
        let result = source.request(&json!({"id":index,"steps":[{"op":"call","args":["registerRuntimeMailboxListener",args]},{"op":"call","args":["readRuntimeMailboxListener",[source_db,recipient]]}]}));
        assert_eq!(result["output"][0]["ok"], true, "{result}");
        assert_eq!(result["output"][1]["ok"], true, "{result}");
        register_runtime_mailbox_listener(
            &native_db,
            &recipient,
            std::process::id(),
            path.map(JsString::from).as_ref(),
            RuntimeMailboxRegistrationOptions::default(),
        )
        .unwrap();
        let native = read_runtime_mailbox_listener(&native_db, &recipient).unwrap().unwrap();
        assert_eq!(
            native.session_path.as_deref(),
            result["output"][1]["value"]["sessionPath"].as_str(),
            "input path {path:?}"
        );
    }
}
