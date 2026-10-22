use pi_coding_agent::core::extensions::loader::{
    ExtensionRegistry, inline_extension_name, is_extension_factory_disabled, is_extension_path_disabled,
};
use pi_js_compat::js_json::JsString;

#[test]
fn disabled_extension_identity_preserves_unpaired_units() {
    let name = JsString::from_utf16(vec![b'x' as u16, 0xd800]);
    let inline = JsString::from("<first-party:") + &name + ">";
    assert_eq!(inline_extension_name(&inline), Some(name.clone()));
    assert!(is_extension_factory_disabled(&[name.clone()], &inline));
    assert!(!is_extension_factory_disabled(&["x�".into()], &inline));
    let path = JsString::from("/extensions/") + &name;
    assert!(is_extension_path_disabled(&[name], &path, &path));
    assert!(!is_extension_path_disabled(&["x�".into()], &path, &path));
}

#[test]
fn registry_retains_distinct_native_extension_ids() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let replacement = JsString::from("�");
    let mut registry = ExtensionRegistry::new();
    registry.register(raw.clone(), std::sync::Arc::new(|_| Box::pin(async { Ok(()) })));
    registry.register(replacement.clone(), std::sync::Arc::new(|_| Box::pin(async { Ok(()) })));
    assert_eq!(registry.ids(), vec![raw.clone(), replacement.clone()]);
    assert!(registry.get(&raw).is_some());
    assert!(registry.get(&replacement).is_some());
}
