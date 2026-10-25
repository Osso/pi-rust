use super::*;
use std::sync::Mutex;

#[tokio::test]
async fn reload_preserves_flag_values_and_insertion_order_across_fresh_runtimes() {
    use pi_coding_agent::core::extensions::types::FlagValue;
    let fixture = fixture().await;
    let runner = fixture.session.extension_runner();
    runner.set_flag_value("z-enabled", FlagValue::Boolean(true));
    runner.set_flag_value("a-label", FlagValue::String("saved value".to_owned()));
    fixture.session.reload(None).await.unwrap();
    let values = fixture
        .session
        .extension_runner()
        .get_flag_values()
        .into_iter()
        .collect::<Vec<_>>();
    fixture.session.dispose();
    assert_eq!(
        values,
        vec![
            ("z-enabled".to_owned(), FlagValue::Boolean(true)),
            ("a-label".to_owned(), FlagValue::String("saved value".to_owned()))
        ]
    );
}

#[tokio::test]
async fn name_and_clear_extension_effects_start_before_the_setter_returns() {
    use pi_coding_agent::core::extensions::types::{ExtensionContext, SessionInfoChangedEvent};
    use pi_coding_agent::core::extensions::{ExtensionFactory, HandlerError};
    let names = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&names);
    let factory: ExtensionFactory = Arc::new(move |api| {
        let captured = Arc::clone(&captured);
        api.on_session_info_changed(move |event: SessionInfoChangedEvent, _ctx: ExtensionContext| {
            captured.lock().unwrap().push(event.name);
            Box::pin(async { Ok(None) }) as pi_ai::request::BoxFuture<'static, Result<Option<()>, HandlerError>>
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    fixture.session.set_session_name(&"named".into()).unwrap();
    let after_name = names.lock().unwrap().clone();
    fixture.session.clear_session_name().unwrap();
    let after_clear = names.lock().unwrap().clone();
    fixture.session.dispose();
    assert_eq!(after_name, vec![Some(JsString::from("named"))]);
    assert_eq!(after_clear, vec![Some(JsString::from("named")), None]);
}

#[tokio::test]
async fn materialization_auth_failure_clears_busy_state_and_permits_retry() {
    let fixture = fixture().await;
    let mut model = fixture.session.model();
    model.provider = "unconfigured-provider".to_owned();
    fixture.session.agent().set_model(model);
    let entry_id = fixture
        .session
        .session_manager()
        .append_compaction(
            &"checkpoint".into(),
            &"first".into(),
            10.0,
            pi_coding_agent::core::session_manager::CompactionAppendOptions {
                provider_native: Some(pi_ai::types::ProviderNativeContext {
                    provider: fixture.session.model().provider,
                    api: fixture.session.model().api,
                    format: pi_ai::types::ProviderNativeFormat::OpenAIResponsesInput,
                    value: pi_js_compat::js_json::JsValue::from(json!([])),
                }),
                ..Default::default()
            },
        )
        .unwrap();
    let first = fixture
        .session
        .materialize_compaction_summary(&entry_id)
        .await
        .unwrap_err();
    let busy = fixture.session.is_compacting();
    let second = fixture
        .session
        .materialize_compaction_summary(&entry_id)
        .await
        .unwrap_err();
    fixture.session.dispose();
    assert!(first.message().contains("No API key found"), "{:?}", first.message());
    assert!(!busy, "failed authentication must leave the controller's finally scope");
    assert_eq!(
        second.message(),
        first.message(),
        "retry must reach auth, not already-running rejection"
    );
}
