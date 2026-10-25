use pi_coding_agent::utils::version_check::{compare_package_versions, is_newer_package_version};
use pi_js_compat::js_json::JsString;
#[test]
fn invalid_semver_uses_native_trimmed_identity_comparison() {
    let raw = JsString::from_utf16(vec![0xd800]);
    assert_eq!(compare_package_versions(&raw, &"1.0.0".into()), None);
    assert!(is_newer_package_version(&raw, &"�".into()));
    assert!(!is_newer_package_version(&(JsString::from(" ") + &raw + " "), &raw));
}

#[test]
fn user_agent_retains_native_version_before_header_encoding() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let text = pi_coding_agent::utils::pi_user_agent::get_pi_user_agent(&raw);
    assert!(text.contains(&raw));
}
