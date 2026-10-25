#[path = "support/agent_session_fixture.rs"]
mod agent_session_fixture;
use crate::shared_extensions as support;

use pi_coding_agent::core::extensions::ExtensionRunner;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn extension_context_supervisor_cancellation_binds_and_unregisters_exact_owner() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_str().unwrap();
    let mut runner = ExtensionRunner::new(
        Vec::new(),
        support::runtime(),
        cwd,
        support::session_manager(cwd),
        support::model_registry().await,
        None,
    );
    let unbound = runner.create_context();
    unbound
        .register_supervisor_review_cancellation(Arc::new(|| panic!("unbound callback")))
        .unwrap()();
    let owner = Arc::new(Mutex::new(None::<Arc<dyn Fn() + Send + Sync>>));
    let mut actions = support::context_actions();
    let registration_owner = owner.clone();
    actions.register_supervisor_review_cancellation = Some(Arc::new(move |handler| {
        *registration_owner.lock().unwrap() = Some(handler.clone());
        let owner = registration_owner.clone();
        Ok(Arc::new(move || {
            let mut current = owner.lock().unwrap();
            if current.as_ref().is_some_and(|current| Arc::ptr_eq(current, &handler)) {
                *current = None;
            }
        }) as Arc<dyn Fn() + Send + Sync>)
    }));
    runner.bind_core(support::actions(), actions, None).unwrap();
    let context = runner.create_context();
    let hits = Arc::new(AtomicUsize::new(0));
    let old = context
        .register_supervisor_review_cancellation(Arc::new(|| panic!("replaced owner")))
        .unwrap();
    let counted = hits.clone();
    let current = context
        .register_supervisor_review_cancellation(Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        }))
        .unwrap();
    old();
    owner.lock().unwrap().as_ref().unwrap()();
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    current();
    assert!(owner.lock().unwrap().is_none());
    runner.invalidate(Some("fixture runner replaced"));
    assert!(
        context
            .register_supervisor_review_cancellation(Arc::new(|| {}))
            .is_err()
    );
}

#[tokio::test]
async fn extension_context_dropped_session_registration_returns_error() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = agent_session_fixture::fixture(directory.path(), pi_agent_core::harness::ThinkingLevel::Off).await;
    fixture.session.bind_extensions(Default::default()).await.unwrap();
    let context = fixture.session.extension_runner().create_context();
    drop(fixture);
    let error = match context.register_supervisor_review_cancellation(Arc::new(|| {})) {
        Ok(_) => panic!("registration after session drop must fail"),
        Err(error) => error,
    };
    assert_eq!(error.message, "Agent session is no longer available");
}

#[tokio::test]
async fn extension_context_cancellation_reaches_agent_session_owner() {
    let directory = tempfile::tempdir().unwrap();
    let fixture = agent_session_fixture::fixture(directory.path(), pi_agent_core::harness::ThinkingLevel::Off).await;
    fixture.session.bind_extensions(Default::default()).await.unwrap();
    let context = fixture.session.extension_runner().create_context();
    let hits = Arc::new(AtomicUsize::new(0));
    let counted = hits.clone();
    let unregister = context
        .register_supervisor_review_cancellation(Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        }))
        .unwrap();
    assert!(fixture.session.cancel_supervisor_review());
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert!(!fixture.session.cancel_supervisor_review());
    unregister();
    fixture.session.dispose();
}
