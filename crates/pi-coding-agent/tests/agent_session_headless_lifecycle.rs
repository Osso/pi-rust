use super::*;
#[path = "support/env_probe.rs"]
mod env_probe;

#[test]
fn headless_start_reports_ready_and_waits_for_parent_release_before_session_start() {
    let temp = tempfile::tempdir().unwrap();
    let release = temp.path().join("release");
    let child_path = release.clone();
    let worker = std::thread::spawn(move || {
        env_probe::run_probe(
            "headless_lifecycle::release_handshake_child",
            &[("PI_HEADLESS_SESSION_START_RELEASE_PATH", child_path.into_os_string())],
        )
    });
    let ready = release.with_extension("ready");
    let started = release.with_extension("started");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ready.exists() && !started.exists() && !worker.is_finished() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let before_release = (ready.exists(), started.exists());
    std::fs::write(&release, b"release").unwrap();
    worker.join().unwrap();
    assert_eq!(
        before_release,
        (true, false),
        "ready acknowledgement must precede session startup and parent release"
    );
    assert_eq!(std::fs::read(ready).unwrap(), b"ready");
    assert_eq!(std::fs::read(started).unwrap(), b"started");
}

#[tokio::test]
#[ignore = "subprocess probe executed by the handshake test"]
async fn release_handshake_child() {
    if !env_probe::in_probe() {
        return;
    }
    use pi_coding_agent::core::extensions::types::{ExtensionContext, SessionStartEvent};
    use pi_coding_agent::core::extensions::{ExtensionFactory, HandlerError};
    let release = std::path::PathBuf::from(std::env::var_os("PI_HEADLESS_SESSION_START_RELEASE_PATH").unwrap());
    let started = release.with_extension("started");
    let factory: ExtensionFactory = Arc::new(move |api| {
        let started = started.clone();
        api.on_session_start(move |_event: SessionStartEvent, _ctx: ExtensionContext| {
            std::fs::write(&started, b"started").unwrap();
            Box::pin(async { Ok(None) }) as pi_ai::request::BoxFuture<'static, Result<Option<()>, HandlerError>>
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    fixture
        .session
        .bind_extensions(ExtensionBindings::default())
        .await
        .unwrap();
    fixture.session.dispose();
}

#[test]
fn headless_auto_detach_uses_the_configured_delay() {
    env_probe::run_probe(
        "headless_lifecycle::auto_detach_child",
        &[("PI_HEADLESS_TOOL_AUTO_DETACH_MS", " 2e1 ".into())],
    );
}

#[tokio::test]
#[ignore = "subprocess probe executed by the auto-detach test"]
async fn auto_detach_child() {
    if !env_probe::in_probe() {
        return;
    }
    let fixture = fixture().await;
    let registry = fixture
        .session
        .extension_runner()
        .create_context()
        .tool_detach_registry()
        .unwrap()
        .unwrap();
    let detached = Arc::new(AtomicBool::new(false));
    let subscription = registry.register(Arc::new(RecordingDetachHandle {
        detached: Arc::clone(&detached),
        accepts: true,
    }));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !detached.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let observed = detached.load(Ordering::SeqCst);
    drop(subscription);
    fixture.session.dispose();
    assert!(
        observed,
        "configured 20ms auto-detach must not use the default 600000ms delay"
    );
}
