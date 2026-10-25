use pi_coding_agent::core::model_resolver::resolve_model_scope_from_models;
use pi_js_compat::js_json::JsString;
#[test]
fn unmatched_raw_scope_pattern_is_reported_without_replacement() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let result = resolve_model_scope_from_models(std::slice::from_ref(&raw), &[]);
    assert!(result.scoped_models.is_empty());
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].pattern, raw.clone());
    assert!(result.diagnostics[0].message.contains(&raw));
}
