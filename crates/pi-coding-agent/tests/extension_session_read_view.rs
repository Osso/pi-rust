#[path = "support/extensions.rs"]
mod support;

use pi_coding_agent::core::extensions::ExtensionRunner;
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry, lock_session_manager};
use pi_js_compat::js_json;

#[tokio::test]
async fn readonly_context_snapshot_is_isolated_and_retained_view_tracks_live_writes() {
    let session = support::session_manager("/private/context");
    let runner = ExtensionRunner::new(
        Vec::new(),
        support::runtime(),
        "/private/context",
        session.clone(),
        support::model_registry().await,
        None,
    );
    let context = runner.create_context();
    let view = context.session_manager().unwrap();
    let id = lock_session_manager(&session)
        .append_custom_entry(&"probe".into(), Some(js_json!({"nested":{"value":1}})))
        .unwrap();
    let mut snapshot = view.get_entry(&id).unwrap();
    let SessionEntry::Known(KnownSessionEntry::Custom(entry)) = &mut snapshot else {
        panic!("custom entry");
    };
    entry.data = Some(js_json!({"nested":{"value":99}}));
    assert_ne!(view.get_entry(&id), Some(snapshot));
    assert_eq!(view.get_leaf_id(), Some(id));
    view.set_session_goal_json(&"{\"goal\":1}".into()).unwrap();
    assert_eq!(
        lock_session_manager(&session).get_session_goal_json().unwrap(),
        Some("{\"goal\":1}".into())
    );
    view.clear_session_goal_json().unwrap();
    assert_eq!(lock_session_manager(&session).get_session_goal_json().unwrap(), None);
    runner.invalidate(Some("extension runtime replaced"));
    assert_eq!(
        context.session_manager().err().unwrap().message,
        "extension runtime replaced"
    );
    assert_eq!(view.get_cwd(), "/private/context");
}
