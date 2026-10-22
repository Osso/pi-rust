use pi_coding_agent::core::rule_store::PermissionRuleStore;
use pi_js_compat::js_json::{JsString, JsValue};
#[test]
fn native_settings_permission_rules_preserve_raw_content() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let settings = pi_js_compat::js_json!({"permissionRules":{"allow":{"bash":[raw.clone()]}}});
    let store = PermissionRuleStore::from_settings(Some(&settings));
    assert!(store.has_allow_rule("bash", &raw));
    let not_raw = JsString::from("�");
    assert!(!store.has_allow_rule("bash", &not_raw));
    assert!(matches!(settings, JsValue::Object(_)));
}
