use crate::shared_extensions as support;
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn native_disable_broadcast_filters_only_broadcast_on_session_start() {
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().to_str().unwrap();
    let path = JsString::from(temp.path().join("disable-broadcast.ts").to_str().unwrap());
    std::fs::write(temp.path().join("disable-broadcast.ts"), "").unwrap();
    let mut registry = ExtensionRegistry::new();
    pi_coding_agent_extensions::native_user_extensions::register(&mut registry);
    let runtime = support::runtime();
    let active = Arc::new(Mutex::new(vec![
        "read".to_owned(),
        "broadcast".to_owned(),
        "broadcasting".to_owned(),
        "broadcast".to_owned(),
        "edit".to_owned(),
    ]));
    let mut actions = support::actions();
    let getter = Arc::clone(&active);
    actions.get_active_tools = Arc::new(move || Ok(getter.lock().unwrap().clone()));
    let setter = Arc::clone(&active);
    actions.set_active_tools = Arc::new(move |tools| {
        *setter.lock().unwrap() = tools.to_vec();
        Ok(())
    });
    runtime.set_actions(actions);
    let result = registry
        .load_extensions(
            std::slice::from_ref(&path),
            &JsString::from(cwd),
            None,
            Some(runtime.clone()),
        )
        .await
        .unwrap();
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.extensions.len(), 1);
    assert_eq!(active.lock().unwrap().len(), 5, "loading must not change active tools");
    let runner = support::runner_for(result.extensions, cwd, runtime, support::model_registry().await);
    let mut event = ExtensionEvent::SessionStart(SessionStartEvent {
        reason: SessionStartReason::Startup,
        previous_session_file: None,
    });
    runner.emit(&mut event).await;
    assert_eq!(*active.lock().unwrap(), ["read", "broadcasting", "edit"]);
    *active.lock().unwrap() = vec!["broadcast".to_owned(), "write".to_owned()];
    runner.emit(&mut event).await;
    assert_eq!(*active.lock().unwrap(), ["write"]);
}
