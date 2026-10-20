#[path = "../examples/sdk/02_custom_model.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_examples.rs"]
mod support;

#[tokio::test]
async fn selects_first_available_model_and_streams_greeting() {
    let fixture = support::fixture().await;
    let _home = fixture.home;
    let runtime = fixture.options.model_runtime.as_ref().unwrap().clone();
    let available = runtime.get_available(None, Default::default()).await.unwrap();
    let names: Vec<_> = available.iter().map(|m| format!("{}/{}", m.provider, m.id)).collect();
    assert!(!names.is_empty());
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/02-custom-model.ts",
    );
    let models: Vec<_> = [("anthropic", "claude-opus-4-5"), ("my-provider", "my-model")]
        .into_iter()
        .filter_map(|(provider, id)| runtime.get_model(provider, id))
        .collect();
    let expected = source.request(&serde_json::json!({"models":models,"available":available}));
    let output = support::Output::default();
    let _main = example::main;
    example::run(fixture.options, output.clone()).await.unwrap();
    assert!(output.text().contains(&format!(
        "Available models: {}\n",
        serde_json::to_string(&names).unwrap()
    )));
    assert!(output.text().ends_with("Hello!\n"));
    assert_eq!(output.text(), expected["stdout"]);
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["reasoning"], "medium");
    assert_eq!(requests[0]["model"]["id"], available[0].id);
    assert_eq!(
        requests[0]["context"]["messages"][0]["content"][0]["text"],
        "Say hello in one sentence."
    );
}

#[tokio::test]
async fn stream_write_failure_retains_message_and_disposes_owned_session() {
    use std::io::Write;
    struct FailText(support::Output);
    impl Write for FailText {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes == b"Hello!" {
                return Err(std::io::Error::other("fixture write failed"));
            }
            self.0.write(bytes)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let fixture = support::fixture().await;
    let _home = fixture.home;
    let id = fixture
        .options
        .session_manager
        .as_ref()
        .unwrap()
        .get_session_id()
        .to_owned();
    let disposed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = std::sync::Arc::clone(&disposed);
    let registration = pi_ai::session_resources::register_session_resource_cleanup(Box::new(move |session| {
        if session == Some(id.as_str()) {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    }));
    let runtime = fixture.options.model_runtime.as_ref().unwrap();
    let available = runtime.get_available(None, Default::default()).await.unwrap();
    let models: Vec<_> = [("anthropic", "claude-opus-4-5"), ("my-provider", "my-model")]
        .into_iter()
        .filter_map(|(p, id)| runtime.get_model(p, id))
        .collect();
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/02-custom-model.ts",
    );
    let expected = source
        .request(&serde_json::json!({"models":models,"available":available,"writeFailure":"fixture write failed"}));
    let output = support::Output::default();
    let error = example::run(fixture.options, FailText(output.clone()))
        .await
        .unwrap_err();
    assert_eq!(error, expected["error"]);
    assert_eq!(expected["errorName"], "Error");
    assert_eq!(expected["disposed"], serde_json::json!([0]));
    assert_eq!(output.text(), expected["stdout"]);
    assert!(disposed.load(std::sync::atomic::Ordering::SeqCst));
    registration.unregister();
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn streaming_stops_after_first_writer_error() {
    use pi_ai::providers::faux::{
        FauxAssistantMessageOptions, FauxResponseStep, RegisterFauxProviderOptions, faux_assistant_message,
        faux_provider, faux_text, faux_tool_call,
    };
    use std::sync::{Arc, Mutex};
    struct FailDeltas(Arc<Mutex<Vec<String>>>);
    impl std::io::Write for FailDeltas {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let text = String::from_utf8_lossy(bytes).to_string();
            if text == "first" || text == "second" {
                self.0.lock().unwrap().push(text.clone());
                return Err(std::io::Error::other(format!("{text} failure")));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut fixture = support::fixture().await;
    let faux = faux_provider(RegisterFauxProviderOptions::default());
    fixture.options.model = faux.get_model(None);
    fixture
        .options
        .model_runtime
        .as_ref()
        .unwrap()
        .register_native_provider(Arc::clone(&faux.provider));
    faux.set_responses(vec![FauxResponseStep::Message(Box::new(faux_assistant_message(
        vec![
            faux_text("first"),
            faux_text("second"),
            faux_tool_call(
                "end_turn",
                serde_json::Map::from_iter([("reason".into(), serde_json::json!("done"))]),
                Some("done".into()),
            ),
        ],
        FauxAssistantMessageOptions::default(),
    )))]);
    let writes = Arc::new(Mutex::new(Vec::new()));
    let error = example::run(fixture.options, FailDeltas(Arc::clone(&writes)))
        .await
        .unwrap_err();
    assert_eq!(error, "first failure");
    assert_eq!(*writes.lock().unwrap(), ["first"]);
}
