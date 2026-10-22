use pi_agent_core::AgentError;
use pi_agent_core::harness::session::{InMemorySessionStorage, Session};
use pi_agent_core::harness::*;
use pi_ai::models::{CreateModelsOptions, create_models};
use pi_ai::providers::faux::{RegisterFauxProviderOptions, faux_provider};
use pi_ai::types::{AssistantContent, StopReason};
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

fn harness() -> AgentHarness {
    let env = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: "/tmp".into(),
        ..Default::default()
    }));
    let storage = Arc::new(InMemorySessionStorage::new(Default::default()).unwrap());
    AgentHarness::new(AgentHarnessOptions::new(
        env,
        Session::new(storage),
        Arc::new(create_models(CreateModelsOptions::default())),
        faux_provider(RegisterFauxProviderOptions::default())
            .get_model(None)
            .unwrap(),
    ))
    .unwrap()
}

#[tokio::test]
async fn rejected_before_provider_request_is_persisted_as_exact_failure_message() {
    let harness = harness();
    let error = JsString::from_utf16(vec![0xd800, 0x21]);
    let hook_error = error.clone();
    let _remove_hook = harness.on(
        HarnessHookKind::BeforeProviderRequest,
        Arc::new(move |_| {
            let error = hook_error.clone();
            Box::pin(async move { Err(AgentError::new(error)) })
        }),
    );
    let events = Arc::new(Mutex::new(Vec::new()));
    let observed = events.clone();
    let _remove_listener = harness.subscribe(Arc::new(move |event, _| {
        observed.lock().unwrap().push(event.kind());
        Box::pin(async { Ok(()) })
    }));
    let result = harness.prompt("hello".into(), None).await.unwrap();
    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(result.error_message, Some(error));
    assert!(matches!(result.content.as_slice(), [AssistantContent::Text(text)] if text.text.is_empty()));
    let context = harness.session().build_context().await.unwrap();
    assert_eq!(context.messages.len(), 2);
    assert_eq!(context.messages[1].as_assistant(), Some(&result));
    assert_eq!(events.lock().unwrap().last(), Some(&"settled"));
    harness.wait_for_idle().await;
}

#[tokio::test]
async fn unpaired_header_name_rejects_before_provider_dispatch() {
    let harness = harness();
    harness
        .set_stream_options(AgentHarnessStreamOptions {
            headers: Some([(JsString::from_utf16(vec![0xd800]), "value".into())].into()),
            ..Default::default()
        })
        .await;
    let result = harness.prompt("hello".into(), None).await.unwrap();
    assert_eq!(result.stop_reason, StopReason::Error);
    assert!(result.error_message.unwrap().to_utf8().unwrap().contains("surrogate"));
    assert!(matches!(result.content.as_slice(), [AssistantContent::Text(text)] if text.text.is_empty()));
}
