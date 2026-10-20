#[path = "../examples/sdk/13_session_runtime.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use serde_json::json;

#[tokio::test]
async fn replaces_rebinds_and_switches_the_active_session() {
    let mut fixture = support::fixture().await;
    fixture.options.session_manager = None;
    let output = support::Output::default();
    let _main = example::main;
    let files = example::run(fixture.options, output.clone()).await.unwrap();
    assert_eq!(files.len(), 3);
    assert_ne!(files[0], files[1]);
    assert_eq!(files[0], files[2]);
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/13-session-runtime.ts",
    );
    let expected = source.request(&json!({"runtimeFiles":files}));
    assert_eq!(output.text(), expected["stdout"]);
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert!(fixture.home.path().exists());
}

#[tokio::test]
async fn queue_subscription_rebinds_without_duplicate_or_stale_output() {
    let mut fixture = support::fixture().await;
    fixture.options.session_manager = None;
    let _home = fixture.home;
    let output = std::sync::Arc::new(std::sync::Mutex::new(support::Output::default()));
    let mut runtime = example::create_runtime(fixture.options).await.unwrap();
    let first = example::bind_session(&runtime, None, std::sync::Arc::clone(&output)).await;
    first.0.steer("first", &[]).await.unwrap();
    runtime.new_session(Default::default()).await.unwrap();
    let old = first.0.clone();
    let second = example::bind_session(&runtime, Some(first), std::sync::Arc::clone(&output)).await;
    let before = output.lock().unwrap().text();
    old.clear_queue();
    assert_eq!(output.lock().unwrap().text(), before);
    second.0.steer("second", &[]).await.unwrap();
    second.0.follow_up("third", &[]).await.unwrap();
    assert!(output.lock().unwrap().text().ends_with("Queued: 1\nQueued: 2\n"));
    second.0.unsubscribe(second.1);
    let before = output.lock().unwrap().text();
    second.0.clear_queue();
    assert_eq!(output.lock().unwrap().text(), before);
    runtime.dispose().await;
    assert!(fixture.requests.lock().unwrap().is_empty());
}
