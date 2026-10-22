use pi_coding_agent::core::resource_loader::{
    ResourceLoader, StaticResourceLoader, inline_extension_name, resolve_prompt_input,
};
use pi_js_compat::js_json::JsString;

#[test]
fn literal_prompts_and_inline_names_keep_unpaired_units() {
    let raw = JsString::from_utf16(vec![0xd800, 0xdc00, 0xd800]);
    assert_eq!(resolve_prompt_input(Some(&raw), "system prompt"), Some(raw.clone()));
    let inline = JsString::from("<first-party:") + &raw + ">";
    assert_eq!(inline_extension_name(&inline), Some(raw.clone()));
    let loader = StaticResourceLoader {
        system_prompt: Some(raw.clone()),
        append_system_prompt: vec![raw.clone()],
        ..Default::default()
    };
    assert_eq!(loader.get_system_prompt(), Some(raw.clone()));
    assert_eq!(loader.get_append_system_prompt(), vec![raw]);
}
