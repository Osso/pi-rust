use pi_coding_agent::core::settings_manager::{PackageFilter, PackageSource};
use pi_js_compat::js_json::JsString;

#[test]
fn configured_source_and_resource_patterns_keep_native_units() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let package = PackageSource::Filter(PackageFilter {
        source: raw.clone(),
        skills: Some(vec![raw.clone()]),
        ..Default::default()
    });
    assert_eq!(package.source(), &raw);
    assert_eq!(package.filter().unwrap().skills.as_ref().unwrap(), &vec![raw]);
}
