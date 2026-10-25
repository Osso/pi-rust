use super::*;
use pi_coding_agent::core::extensions::types::{ExtensionContext, SessionStartEvent};
use pi_coding_agent::core::extensions::{ExtensionFactory, HandlerError};
use std::sync::Mutex;

#[tokio::test]
async fn extension_custom_send_commits_its_sync_prefix_before_returning() {
    let owner = Arc::new(Mutex::new(None::<AgentSession>));
    let observed = Arc::new(Mutex::new(None));
    let (target, captured) = (Arc::clone(&owner), Arc::clone(&observed));
    let factory: ExtensionFactory = Arc::new(move |api| {
        let sending = api.clone();
        let (target, captured) = (Arc::clone(&target), Arc::clone(&captured));
        api.on_session_start(move |_event: SessionStartEvent, _ctx: ExtensionContext| {
            sending
                .send_message(
                    pi_coding_agent::core::extensions::types::CustomMessagePayload {
                        custom_type: "memo".into(),
                        content: pi_coding_agent::core::extensions::types::CustomMessageContent::Text(
                            "saved memo".into(),
                        ),
                        display: false,
                        details: None,
                    },
                    None,
                )
                .unwrap();
            let session = target.lock().unwrap().as_ref().unwrap().clone();
            *captured.lock().unwrap() = Some((session.messages().len(), session.session_manager().get_entries().len()));
            Box::pin(async { Ok(None) }) as pi_ai::request::BoxFuture<'static, Result<Option<()>, HandlerError>>
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    *owner.lock().unwrap() = Some(fixture.session.clone());
    fixture.session.bind_extensions(Default::default()).await.unwrap();
    *owner.lock().unwrap() = None;
    fixture.session.dispose();
    assert_eq!(*observed.lock().unwrap(), Some((1, 1)));
}

#[tokio::test]
async fn extension_user_send_failures_reach_the_runtime_error_listener() {
    let factory: ExtensionFactory = Arc::new(|api| {
        let sending = api.clone();
        api.on_session_start(move |_event: SessionStartEvent, _ctx: ExtensionContext| {
            sending
                .send_user_message(
                    pi_coding_agent::core::extensions::types::UserMessageContent::Text("hello".into()),
                    None,
                )
                .unwrap();
            Box::pin(async { Ok(None) }) as pi_ai::request::BoxFuture<'static, Result<Option<()>, HandlerError>>
        })
        .unwrap();
        Box::pin(async { Ok(()) })
    });
    let fixture = fixture_with_timeout_extension(factory).await;
    fixture.session.set_working_directory_recovery_handlers(
        pi_coding_agent::core::agent_session::WorkingDirectoryRecoveryHandlers {
            before_run: Arc::new(|| {
                Box::pin(async { Err(pi_ai::request::CallbackError::new("fixture recovery failure")) })
            }),
            after_turn: Arc::new(|| Box::pin(async { Ok(()) })),
        },
    );
    let errors = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&errors);
    let seen = Arc::new(tokio::sync::Notify::new());
    let ready = Arc::clone(&seen);
    let _subscription = fixture.session.extension_runner().on_error(Arc::new(move |event| {
        captured
            .lock()
            .unwrap()
            .push((event.extension_path.clone(), event.event.clone(), event.error.clone()));
        ready.notify_one();
    }));
    fixture.session.bind_extensions(Default::default()).await.unwrap();
    let reported = tokio::time::timeout(std::time::Duration::from_secs(2), seen.notified()).await;
    fixture.session.dispose();
    assert!(reported.is_ok(), "send rejection must not be silently discarded");
    assert_eq!(
        *errors.lock().unwrap(),
        vec![(
            JsString::from("<runtime>"),
            "send_user_message".to_owned(),
            JsString::from("fixture recovery failure")
        )]
    );
}
