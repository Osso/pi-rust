#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::readonly_session_manager::create_readonly_session_manager;
use pi_coding_agent::core::session_manager::SessionManager;
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry};
use pi_js_compat::js_json;
use pi_js_compat::js_json::{JsString, JsValue};

fn mutate_entry(entry: &mut SessionEntry) {
    match entry {
        SessionEntry::Known(KnownSessionEntry::Custom(entry)) => entry.data = Some(js_json!({"foreign":true})),
        SessionEntry::Unknown(entry) => {
            entry.insert("foreign".into(), JsValue::Bool(true));
        }
        _ => panic!("expected custom entry"),
    }
}
use std::sync::{Arc, Mutex};

#[test]
fn readonly_manager_returns_isolated_reads_and_forwards_live_state_and_goal_mutators() {
    let mut source = oracle::TsOracle::spawn(
        "readonly-session-manager-oracle.mts",
        "packages/coding-agent/src/core/readonly-session-manager.ts",
    );
    let expected = source.request(&js_json!({}));
    let manager = Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&"/private/repo".into()), None).unwrap(),
    ));
    let view = create_readonly_session_manager(Arc::clone(&manager));
    let id = manager
        .lock()
        .unwrap()
        .append_custom_entry(&"probe".into(), Some(js_json!({"nested":{"value":1}})))
        .unwrap();
    let original = view.get_entry(&id).unwrap();
    let mut snapshot = view.get_entry(&id).unwrap();
    mutate_entry(&mut snapshot);
    assert_eq!(view.get_entry(&id).unwrap(), original);
    assert_eq!(view.get_leaf_id(), Some(id.clone()));
    assert_eq!(view.get_cwd(), JsString::from("/private/repo"));
    assert_eq!(view.get_branch(None), vec![original.clone()]);
    assert_eq!(view.build_context_entries(), vec![original.clone()]);
    assert_eq!(view.get_entries(), vec![original.clone()]);
    let mut tree = view.get_tree();
    mutate_entry(&mut tree[0].entry);
    assert_eq!(view.get_tree()[0].entry, original);
    let mut header = view.get_header().unwrap();
    header.fields.insert("cwd".into(), JsValue::String("/changed".into()));
    assert_eq!(view.get_header().unwrap().cwd(), Some(&JsString::from("/private/repo")));
    view.set_session_goal_json(&"{\"goal\":1}".into()).unwrap();
    let goal = view.get_session_goal_json().unwrap();
    let underlying_goal = manager.lock().unwrap().get_session_goal_json().unwrap();
    assert_eq!(view.get_session_goal_json().unwrap(), Some("{\"goal\":1}".into()));
    assert_eq!(
        manager.lock().unwrap().get_session_goal_json().unwrap(),
        Some("{\"goal\":1}".into())
    );
    assert_eq!(
        view.get_session_goal_json_for_session(&"/other.jsonl".into()).unwrap(),
        None
    );
    view.clear_session_goal_json().unwrap();
    assert_eq!(view.get_session_goal_json().unwrap(), None);
    assert_eq!(view.get_session_file(), None);
    assert_eq!(view.get_session_dir(), JsString::default());
    assert_eq!(view.get_leaf_entry(), Some(view.get_entries()[0].clone()));
    assert_eq!(view.get_label(&id), None);
    assert_eq!(view.get_session_name(), None);
    assert!(!view.has_session_name_state());
    assert!(!view.is_subagent_session());
    assert_eq!(view.get_subagent_name(), None);
    assert!(!view.get_session_id().is_empty());
    let actual = js_json!({
        "entryIsolated": view.get_entry(&id).unwrap() == original,
        "headerIsolated": view.get_header().unwrap().cwd() == Some(&JsString::from("/private/repo")),
        "cwd": view.get_cwd(), "sessionDir": view.get_session_dir(), "sessionFile": view.get_session_file(),
        "leafMatches": view.get_leaf_id() == Some(id.clone()), "hasId": !view.get_session_id().is_empty(),
        "label": view.get_label(&id), "name": view.get_session_name(), "nameState": view.has_session_name_state(),
        "goal": goal, "underlyingGoal": underlying_goal,
        "otherGoal": view.get_session_goal_json_for_session(&"/other.jsonl".into()).unwrap(),
        "cleared": view.get_session_goal_json().unwrap(), "subagent": view.is_subagent_session(), "subagentName": view.get_subagent_name()
    });
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn footer_override_exposes_the_live_isolated_session_view() {
    use pi_coding_agent::core::footer_data_provider::{FooterDataProvider, FooterSessionOverride};
    let root = tempfile::tempdir().unwrap();
    let manager = Arc::new(Mutex::new(
        SessionManager::in_memory(Some(&"/private/repo".into()), None).unwrap(),
    ));
    let view = create_readonly_session_manager(Arc::clone(&manager));
    let footer = FooterDataProvider::new(root.path().to_str().unwrap());
    footer.set_session_override(FooterSessionOverride {
        cwd: "/private/repo".into(),
        session_manager: Some(view),
        model: None,
        thinking_level: "off".into(),
        context_usage: None,
    });
    let id = manager
        .lock()
        .unwrap()
        .append_custom_entry(&"probe".into(), Some(js_json!({"nested":1})))
        .unwrap();
    let displayed = footer.get_session_override().unwrap().session_manager.unwrap();
    let mut entry = displayed.get_entry(&id).unwrap();
    mutate_entry(&mut entry);
    assert_ne!(displayed.get_entry(&id).unwrap(), entry);
    displayed.set_session_goal_json(&"{}".into()).unwrap();
    assert_eq!(
        manager.lock().unwrap().get_session_goal_json().unwrap(),
        Some("{}".into())
    );
    footer.dispose();
}
