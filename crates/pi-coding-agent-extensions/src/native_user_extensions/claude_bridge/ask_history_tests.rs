use super::*;
use crate::core::{
    auth_storage::AuthStorage,
    extensions::{ExtensionRegistry, ExtensionRunner},
    model_registry::ModelRegistry,
    session_manager::SessionManager,
};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn claude_bridge_askclaude_shared_history_imports_once_and_resumes_native_session() {
    for existing in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("claude");
        std::fs::write(&executable, include_str!("fixtures/claude.py")).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bridge = provider::BridgeProvider::new(JsValue::Null, Ok(executable), "AskClaude".into());
        *bridge.sessions.lock().unwrap() = Ok(sessions::SessionStore::at(
            dir.path().join("cc"),
            dir.path().join("agent"),
        ));
        let cwd: JsString = dir.path().to_str().unwrap().into();
        let mut session = SessionManager::in_memory(Some(&cwd), None).unwrap();
        let user = js_json::from_value(
            &js_json::parse_lossless(r#"{"role":"user","content":"prior question \udfff","timestamp":1}"#).unwrap(),
        )
        .unwrap();
        session.append_message(user).unwrap();
        let assistant = js_json::from_value(&js_json::parse_lossless(r#"{"role":"assistant","api":"test","provider":"test","model":"test","content":[{"type":"text","text":"prior answer"}],"stopReason":"stop","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"timestamp":2}"#).unwrap()).unwrap();
        session.append_message(assistant).unwrap();
        if existing {
            bridge
                .sessions
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .record(session.get_session_id(), "existing-native".into(), 2, dir.path())
                .unwrap();
        }
        let mut registry = ExtensionRegistry::new();
        let factory_bridge = bridge.clone();
        registry.register_hidden(
            "<fixture:ask-history>",
            Arc::new(move |pi| {
                let result = ask_claude::register(
                    pi,
                    &js_json::parse_lossless(r#"{"enabled":true}"#).unwrap(),
                    factory_bridge.clone(),
                );
                Box::pin(async { result })
            }),
        );
        let loaded = registry
            .load_extensions(&["<fixture:ask-history>".into()], &cwd, None, None)
            .await
            .unwrap();
        assert!(loaded.errors.is_empty());
        let definition = loaded.extensions[0].read().tools["AskClaude"].definition.clone();
        let models =
            Arc::new(ModelRegistry::in_memory(AuthStorage::in_memory(Vec::<(JsString, JsValue)>::new())).unwrap());
        let runner = ExtensionRunner::new(
            loaded.extensions,
            loaded.runtime,
            cwd,
            Arc::new(Mutex::new(session)),
            models,
            None,
        );
        let prompt = if existing {
            "assert-ask-existing"
        } else {
            "assert-ask-history"
        };
        let args = JsValue::object([
            ("prompt", JsValue::String(prompt.into())),
            ("mode", JsValue::String("none".into())),
        ]);
        let result = (definition.execute)("ask".into(), args, None, None, Some(runner.create_context()))
            .await
            .unwrap();
        assert_ne!(
            result
                .details
                .as_ref()
                .and_then(|d| d.get("error"))
                .and_then(JsValue::as_bool),
            Some(true),
            "{:?}",
            result.content
        );
        let pi_ai::types::TextOrImageContent::Text(text) = &result.content[0] else {
            panic!("answer");
        };
        assert_eq!(text.text, "shared history restored");
    }
}
