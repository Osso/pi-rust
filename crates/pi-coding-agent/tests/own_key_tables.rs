//! Approved 2026-10-23 exclusion: plain-object prototype leaks are not ported.
use pi_coding_agent::core::idle_compaction::compute_idle_compaction_due_at;
use pi_coding_agent::modes::interactive::login_options::is_api_key_login_provider;
use pi_coding_agent::modes::interactive::theme::get_language_from_path;
use pi_js_compat::js_json::JsString;
use std::collections::HashSet;

#[test]
fn plain_object_tables_use_only_explicit_keys() {
    let oauth = HashSet::from(["constructor".to_owned()]);
    assert!(!is_api_key_login_provider("constructor", &oauth, &HashSet::new()));
    for name in ["constructor", "__proto__", "tostring", "valueof"] {
        assert!(get_language_from_path(&JsString::from(format!("example.{name}"))).is_none());
        let mut message = pi_ai::AssistantMessage::empty(name, "provider", "model", 0.0);
        message.usage.input = 100_000.0;
        assert_eq!(compute_idle_compaction_due_at(&message), None);
    }
}
