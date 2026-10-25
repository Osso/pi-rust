use super::*;
use pi_coding_agent::core::extensions::types::{SessionStartEvent, SessionStartReason};
mod oracle {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/oracle.rs"));
}

#[tokio::test]
async fn convenience_factory_emits_the_requested_session_start_event() {
    use pi_coding_agent::core::agent_session_runtime::{DefaultRuntimeFactoryOptions, default_runtime_factory};
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().to_str().unwrap();
    let extension_runtime = create_extension_runtime();
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorded = events.clone();
    let extension = load_extension(&extension_runtime, cwd, "/fixture/initial-event.rs", move |api| {
        api.on_session_start(handler!(SessionStartEvent, [recorded], |event, _ctx| {
            recorded.lock().unwrap().push(event);
            Ok(None)
        }))
        .unwrap();
    })
    .await;
    let loader = Arc::new(StaticResourceLoader {
        extensions: Some(LoadExtensionsResult {
            extensions: vec![extension],
            errors: Vec::new(),
            runtime: extension_runtime,
        }),
        ..Default::default()
    });
    let model = faux_provider(RegisterFauxProviderOptions::default())
        .get_model(None)
        .unwrap();
    let factory = default_runtime_factory(DefaultRuntimeFactoryOptions {
        resource_loader: loader,
        model: Some(model),
        thinking_level: Some(ThinkingLevel::Off),
        scoped_models: Vec::new(),
        tools: Some(Vec::new()),
        exclude_tools: None,
        no_tools: None,
        permission_prompt_tool: None,
        custom_tools: Vec::new(),
        base_tools: Vec::new(),
    });
    let cwd = JsString::from(cwd);
    let event = SessionStartEvent {
        reason: SessionStartReason::Resume,
        previous_session_file: Some("previous.jsonl".into()),
    };
    let manager = SessionManager::in_memory(Some(&cwd), None).unwrap();
    let runtime = create_agent_session_runtime(factory, &cwd, &cwd, manager, None, Some(event.clone()))
        .await
        .unwrap();
    runtime
        .session()
        .bind_extensions(pi_coding_agent::core::agent_session::ExtensionBindings::default())
        .await
        .unwrap();
    assert_eq!(*events.lock().unwrap(), vec![event]);
    runtime.dispose().await.unwrap();
}

#[tokio::test]
async fn initial_runtime_preserves_absent_and_explicit_session_start_events_like_frozen() {
    let mut source = oracle::TsOracle::spawn("runtime-initial-event-oracle.mts", "packages/coding-agent/src");
    for event in [
        None,
        Some(SessionStartEvent {
            reason: SessionStartReason::New,
            previous_session_file: None,
        }),
        Some(SessionStartEvent {
            reason: SessionStartReason::Resume,
            previous_session_file: Some(JsString::from("previous ") + JsString::from_utf16(vec![0xd800])),
        }),
    ] {
        let root = tempfile::tempdir().unwrap();
        let cwd = JsString::from(root.path().to_str().unwrap());
        let input = event
            .clone()
            .map(ExtensionEvent::SessionStart)
            .map(|event| pi_js_compat::js_json::to_value(&event).unwrap())
            .unwrap_or(JsValue::Null);
        let expected = source.request(&pi_js_compat::js_json!({"cwd":cwd,"event":input}));
        let captured = Arc::new(Mutex::new(JsValue::String("factory not called".into())));
        let observed = captured.clone();
        let factory: pi_coding_agent::core::agent_session_runtime::CreateAgentSessionRuntimeFactory =
            Arc::new(move |request| {
                *observed.lock().unwrap() = request
                    .session_start_event
                    .map(ExtensionEvent::SessionStart)
                    .map(|event| pi_js_compat::js_json::to_value(&event).unwrap())
                    .unwrap_or(JsValue::Null);
                Box::pin(async { Err(pi_ai::request::CallbackError::new("fixture factory stop")) })
            });
        let manager = SessionManager::in_memory(Some(&cwd), None).unwrap();
        let error = match create_agent_session_runtime(factory, &cwd, &cwd, manager, None, event).await {
            Ok(_) => panic!("fixture factory must reject"),
            Err(error) => error,
        };
        let error = error.thrown_value().as_error().unwrap().value();
        let actual = pi_js_compat::js_json!({"event":captured.lock().unwrap().clone(),"error":{"name":error.get("name").unwrap(),"message":error.get("message").unwrap()}});
        assert_eq!(actual, expected, "input={input:?}");
    }
}
