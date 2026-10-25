#[path = "../examples/sdk/13_session_runtime.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use serde_json::json;

struct QueueFailure;
impl std::io::Write for QueueFailure {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.starts_with(b"Queued:") {
            return Err(std::io::Error::other("queue output failed"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn queue_output_failure_report_child() {
    if std::env::var_os("PI_SDK13_QUEUE_REPORT_CHILD").is_none() {
        return;
    }
    let fixture = support::fixture().await;
    let runtime = example::create_runtime(fixture.options).await.unwrap();
    let subscription = example::bind_session(&runtime, None, std::sync::Arc::new(std::sync::Mutex::new(QueueFailure)))
        .await
        .unwrap();
    subscription.0.clear_queue();
    subscription.0.unsubscribe(subscription.1);
    runtime.dispose().await.unwrap();
}

#[tokio::test]
async fn queue_output_failure_does_not_interrupt_replacement() {
    let lock = oracle::read_source_lock();
    oracle::verify_source_head(&lock);
    let source = oracle::spawn_oracle_process(
        &lock,
        "sdk-runtime-order-oracle.ts",
        "packages/coding-agent/src/core/agent-session-runtime.ts",
        &[],
        std::process::Stdio::piped(),
    )
    .wait_with_output()
    .unwrap();
    assert!(source.status.success(), "{}", String::from_utf8_lossy(&source.stderr));
    let expected: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        expected,
        json!({"error":null,"disposed":true,"replacements":1,"sameSession":false,"bindStarts":1})
    );

    let fixture = support::fixture().await;
    let runtime = example::create_runtime(fixture.options).await.unwrap();
    let subscription = example::bind_session(&runtime, None, std::sync::Arc::new(std::sync::Mutex::new(QueueFailure)))
        .await
        .unwrap();
    subscription.0.clear_queue();
    let original_id = subscription.0.session_id();
    let transition = runtime.new_session(Default::default()).await;
    let actual = json!({
        "error": transition.err().map(|error| error.message),
        "sameSession": runtime.session().session_id() == original_id,
    });
    runtime.dispose().await.unwrap();
    assert_eq!(
        actual,
        json!({"error":expected["error"],"sameSession":expected["sameSession"]})
    );
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "queue_output_failure_report_child", "--nocapture"])
        .env("PI_SDK13_QUEUE_REPORT_CHILD", "1")
        .output()
        .unwrap();
    assert!(child.status.success(), "{child:?}");
    let source_stderr = String::from_utf8(source.stderr).unwrap();
    let expected_report = source_stderr.lines().next().unwrap();
    assert_eq!(
        expected_report,
        "Session event listener error (queue_update): Error: queue output failed"
    );
    let stderr = String::from_utf8(child.stderr).unwrap();
    let reports: Vec<_> = stderr
        .lines()
        .filter(|line| line.starts_with("Session event listener error"))
        .collect();
    assert_eq!(reports, vec![expected_report]);
    assert!(!String::from_utf8(child.stdout).unwrap().contains(expected_report));
    let fixture = support::fixture().await;
    let result = example::run(fixture.options, QueueFailure).await;
    assert!(result.is_ok(), "frozen catches queue subscriber failures: {result:?}");
}

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
    let starts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = std::sync::Arc::clone(&starts);
    let mut registry = pi_coding_agent::core::extensions::loader::ExtensionRegistry::new();
    registry.register(
        "fixture",
        std::sync::Arc::new(move |api| {
            let observed = std::sync::Arc::clone(&observed);
            let result = api.on_session_start(
                move |_: pi_coding_agent::core::extensions::types::SessionStartEvent,
                      _: pi_coding_agent::core::extensions::types::ExtensionContext| {
                    observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Box::pin(async { Ok(None) })
                        as pi_coding_agent::core::extensions::types::BoxFuture<
                            'static,
                            Result<Option<()>, pi_coding_agent::core::extensions::types::HandlerError>,
                        >
                },
            );
            Box::pin(async move { result.map_err(pi_coding_agent::core::extensions::types::HandlerError::new) })
        }),
    );
    let extensions = registry
        .load_extensions(&["fixture".into()], fixture.options.cwd.as_ref().unwrap(), None, None)
        .await
        .unwrap();
    assert!(extensions.errors.is_empty());
    fixture.options.resource_loader = Some(std::sync::Arc::new(
        pi_coding_agent::core::resource_loader::StaticResourceLoader {
            extensions: Some(extensions),
            ..Default::default()
        },
    ));
    let runtime = example::create_runtime(fixture.options).await.unwrap();
    let first = example::bind_session(&runtime, None, std::sync::Arc::clone(&output))
        .await
        .unwrap();
    assert_eq!(starts.load(std::sync::atomic::Ordering::SeqCst), 1);
    first.0.steer("first", &[]).await.unwrap();
    let replacement_fixture = support::fixture().await;
    let replacement = example::create_runtime(replacement_fixture.options).await.unwrap();
    let old = first.0.clone();
    let second = example::bind_session(&replacement, Some(first), std::sync::Arc::clone(&output))
        .await
        .unwrap();
    let live_events = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = std::sync::Arc::clone(&live_events);
    old.subscribe(std::sync::Arc::new(move |_| {
        observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    let before = output.lock().unwrap().text();
    old.clear_queue();
    assert_eq!(
        live_events.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "outgoing session still emits events before disposal"
    );
    assert_eq!(output.lock().unwrap().text(), before);
    second.0.steer("second", &[]).await.unwrap();
    second.0.follow_up("third", &[]).await.unwrap();
    assert!(output.lock().unwrap().text().ends_with("Queued: 1\nQueued: 2\n"));
    second.0.unsubscribe(second.1);
    let before = output.lock().unwrap().text();
    second.0.clear_queue();
    assert_eq!(output.lock().unwrap().text(), before);
    replacement.dispose().await.expect("runtime disposal succeeds");
    runtime.dispose().await.expect("runtime disposal succeeds");
    assert!(fixture.requests.lock().unwrap().is_empty());
}
