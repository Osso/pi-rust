use pi_coding_agent::core::trust_manager::{ProjectTrustDecision, ProjectTrustStore};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
#[test]
fn unreadable_trust_store_error_keeps_raw_path_units() {
    let temporary = tempfile::tempdir().unwrap();
    let root = JsString::from(temporary.path().to_str().unwrap()) + "/" + JsString::from_utf16(vec![0xd800]);
    let store = ProjectTrustStore::new(&root).unwrap();
    std::fs::create_dir_all(encode_node_utf8(store.trust_path())).unwrap();
    let error = store.get(&root).unwrap_err();
    assert!(error.message().contains(&root));
}

#[test]
fn trust_store_persists_native_project_key_through_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let base = JsString::from(temp.path().to_str().unwrap());
    let raw = base.clone() + "/" + JsString::from_utf16(vec![0xd800]);
    std::fs::create_dir_all(encode_node_utf8(&raw)).unwrap();
    let store = ProjectTrustStore::new(&base).unwrap();
    store.set(&raw, ProjectTrustDecision::Trusted).unwrap();
    let reopened = ProjectTrustStore::new(&base).unwrap();
    assert_eq!(reopened.get_entry(&raw).unwrap().unwrap().path, raw);
    let text = std::fs::read_to_string(encode_node_utf8(store.trust_path())).unwrap();
    assert!(text.contains("\\ud800"));
}

struct RejectUi(JsString);
impl pi_coding_agent::core::extensions::types::ExtensionUiContext for RejectUi {
    fn select<'a>(
        &'a self,
        _: &JsString,
        _: &[JsString],
        _: Option<&pi_coding_agent::core::extensions::types::ExtensionUiDialogOptions>,
    ) -> pi_ai::request::BoxFuture<'a, Result<Option<JsString>, pi_ai::request::CallbackError>> {
        let reason = self.0.clone();
        Box::pin(async move { Err(pi_ai::request::CallbackError::new(reason)) })
    }
}

#[tokio::test]
async fn project_trust_selection_failure_propagates_native_error() {
    use pi_coding_agent::core::extensions::types::{ExtensionMode, ProjectTrustContext};
    use pi_coding_agent::core::project_trust::{ResolveProjectTrustedOptions, resolve_project_trusted};
    let temp = tempfile::tempdir().unwrap();
    let cwd = JsString::from(temp.path().to_str().unwrap());
    let config_dir = temp.path().join(
        pi_coding_agent::config::package_config()
            .config_dir_name
            .to_utf8()
            .unwrap(),
    );
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("settings.json"), "{}").unwrap();
    let reason = JsString::from_utf16(vec![0xd800]);
    let context = ProjectTrustContext {
        cwd: cwd.clone(),
        mode: ExtensionMode::Tui,
        has_ui: true,
        ui: std::sync::Arc::new(RejectUi(reason.clone())),
    };
    let store = ProjectTrustStore::new(&cwd).unwrap();
    let error = resolve_project_trusted(ResolveProjectTrustedOptions {
        cwd: &cwd,
        trust_store: &store,
        trust_override: None,
        default_project_trust: None,
        extensions_result: None,
        project_trust_context: &context,
        on_extension_error: None,
    })
    .await
    .unwrap_err();
    assert_eq!(error.message(), &reason);
}

#[test]
fn project_prompt_keeps_raw_cwd_units() {
    let raw = JsString::from_utf16(vec![0xd800]);
    assert!(pi_coding_agent::core::project_trust::format_project_trust_prompt(&raw).contains(&raw));
}
