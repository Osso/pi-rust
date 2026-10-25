use super::finalize::oracle;
use super::*;
use pi_coding_agent::core::event_bus::{EventBus, EventBusController};
use pi_coding_agent::core::extensions::types::{
    ExtensionContext, LoadExtensionsResult, RuntimeMailboxEvent, RuntimeMailboxEventResult,
};
use pi_coding_agent::core::extensions::{ExtensionFactory, create_extension_runtime, load_extension_from_factory};
use pi_coding_agent::core::runtime_mailbox_transcript::read_accepted_runtime_mailbox_store_refs;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};

#[tokio::test(start_paused = true)]
async fn intercept_acceptance_precedes_next_interceptor_and_only_unhandled_messages_prompt() {
    let source_dir = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("storage-mailbox-oracle.mts", "packages/coding-agent/src/core");
    for handle_all in [true, false] {
        let trace = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let path_cell = Arc::new(std::sync::Mutex::new(None::<JsString>));
        let handler_trace = Arc::clone(&trace);
        let handler_path = Arc::clone(&path_cell);
        let runtime = create_extension_runtime();
        let factory: ExtensionFactory = Arc::new(move |api| {
            let trace = Arc::clone(&handler_trace);
            let path = Arc::clone(&handler_path);
            api.on_runtime_mailbox(move |event: RuntimeMailboxEvent, _context: ExtensionContext| {
                let trace = Arc::clone(&trace);
                let path = path.lock().unwrap().clone().unwrap();
                Box::pin(async move {
                    let accepted_count = read_accepted_runtime_mailbox_store_refs(Some(&path)).unwrap().len();
                    let id = event.message.store_ref.message_id;
                    trace
                        .lock()
                        .unwrap()
                        .push(json!({"id":id,"acceptedCount":accepted_count}));
                    Ok(Some(RuntimeMailboxEventResult {
                        handled: handle_all || id == "first",
                    }))
                })
                    as pi_ai::request::BoxFuture<
                        'static,
                        Result<
                            Option<RuntimeMailboxEventResult>,
                            pi_coding_agent::core::extensions::types::HandlerError,
                        >,
                    >
            })
            .unwrap();
            Box::pin(async { Ok(()) })
        });
        let bus: Arc<dyn EventBus> = Arc::new(EventBusController::new());
        let extension = load_extension_from_factory(
            factory,
            &"/fixture".into(),
            bus,
            runtime.clone(),
            &"/fixture/mailbox.rs".into(),
        )
        .await
        .unwrap();
        let env = test_env_with_resources(
            false,
            Vec::new(),
            Some(LoadExtensionsResult {
                extensions: vec![extension],
                errors: Vec::new(),
                runtime,
            }),
        )
        .await;
        *path_cell.lock().unwrap() = Some(session_path(&env));
        env.session
            .bind_extensions(ExtensionBindings {
                control_db_path: Some(env.control_db.clone().into()),
                ..Default::default()
            })
            .await
            .unwrap();
        env.faux
            .set_responses(vec![faux_assistant_message("done", Default::default()).into()]);
        let ids = [
            enqueue_stored_message(&env, "First 漢😀", "first", None),
            enqueue_stored_message(&env, "Second 漢😀", "second", None),
        ];
        let messages = ids
            .iter()
            .map(|id| read_runtime_mailbox_message(&env.control_db, *id).unwrap().unwrap())
            .collect::<Vec<_>>();
        let directory = source_dir.path().join(if handle_all { "all" } else { "mixed" });
        std::fs::create_dir(&directory).unwrap();
        let expected = source.request(&json!({"id":1,"op":"deliveryTrace","dir":directory,
            "db":directory.join("control.sqlite"),"messages":messages,"recipientSessionId":session_id(&env),
            "handled":if handle_all{vec!["first","second"]}else{vec!["first"]}}));
        assert_eq!(expected["ok"], true, "{expected}");
        env.session.drain_runtime_coordination().await.unwrap();
        assert_eq!(json!(*trace.lock().unwrap()), expected["output"]["trace"]);
        assert_eq!(
            expected["output"]["trace"],
            json!([{"id":"first","acceptedCount":0},{"id":"second","acceptedCount":1}])
        );
        let prompt = user_texts(&env.session).first().cloned();
        assert_eq!(serde_json::to_value(prompt).unwrap(), expected["output"]["prompt"]);
        assert_eq!(user_texts(&env.session).len(), usize::from(!handle_all));
        let refs = read_accepted_runtime_mailbox_store_refs(Some(&session_path(&env))).unwrap();
        assert_eq!(JsValue::Array(refs.clone()), JsValue::from(&expected["output"]["refs"]));
        assert_eq!(refs.len(), 2);
        for id in ids {
            assert!(read_runtime_mailbox_message(&env.control_db, id).unwrap().is_none());
        }
        assert!(!env.session.is_streaming());
    }
}
