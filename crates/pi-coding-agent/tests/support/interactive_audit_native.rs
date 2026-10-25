#[tokio::test(flavor = "current_thread")]
async fn native_audit_escape_preserves_plain_input_and_cancels_edits() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mode, _, _, _temp) = build_mode().await;
            mode.editor().borrow_mut().set_text(&"Unsent question".into()).unwrap();
            mode.handle_editor_escape().unwrap();
            assert_eq!(mode.editor().borrow().get_text().unwrap(), "Unsent question");
            mode.editor().borrow_mut().set_text(&"!echo pending".into()).unwrap();
            mode.is_bash_mode.set(true);
            mode.handle_editor_escape().unwrap();
            assert!(mode.editor().borrow().get_text().unwrap().is_empty());
            assert!(!mode.is_bash_mode.get());
            *mode.pending_compaction_summary_edit_id.borrow_mut() = Some("entry-edit".into());
            mode.editor().borrow_mut().set_text(&"Revised summary".into()).unwrap();
            mode.handle_editor_escape().unwrap();
            assert!(mode.editor().borrow().get_text().unwrap().is_empty());
            let chat = JsString::join(mode.chat_container.borrow_mut().render(100), "\n");
            assert!(chat.contains("Compaction summary edit cancelled"));
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn native_audit_bash_input_refreshes_editor_border() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mode, _, _, _temp) = build_mode().await;
            mode.bind_callbacks();
            let border = || {
                mode.default_editor
                    .borrow_mut()
                    .render(80)
                    .into_iter()
                    .find(|line| line.contains("─"))
                    .unwrap()
            };
            let ordinary = border();
            mode.editor().borrow_mut().set_text(&"!echo pending".into()).unwrap();
            mode.drain_actions().await;
            assert_ne!(border(), ordinary);
            mode.editor().borrow_mut().set_text(&"Ordinary input".into()).unwrap();
            mode.drain_actions().await;
            assert_eq!(border(), ordinary);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn native_audit_interrupt_submits_combined_queued_input() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mode, _, faux, _temp) = build_mode().await;
            faux.set_responses(vec![completed_fixture_response("finished")]);
            mode.session().follow_up("Queued follow-up", &[]).await.unwrap();
            mode.compaction_queued_messages
                .borrow_mut()
                .push(CompactionQueuedMessage {
                    text: "Queued steering".into(),
                    mode: "steer",
                });
            mode.editor().borrow_mut().set_text(&"Current question".into()).unwrap();
            mode.cancel_streaming_and_submit_queued_messages().await.unwrap();
            assert!(mode.editor().borrow().get_text().unwrap().is_empty());
            let users = mode
                .session()
                .agent()
                .state()
                .messages
                .iter()
                .filter_map(|message| {
                    matches!(message, AgentMessage::User(_)).then(|| InteractiveMode::get_user_message_text(message))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                users,
                vec![JsString::from(
                    "Queued steering\n\nQueued follow-up\n\nCurrent question"
                )]
            );
            assert!(mode.get_all_queued_messages(false).0.is_empty());
            assert!(mode.get_all_queued_messages(false).1.is_empty());
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn native_audit_compaction_flush_restores_rejected_prompt_queue() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mode, _, _, _temp) = build_mode().await;
            let mut model = mode.session().model();
            model.provider = "unconfigured-audit".to_owned();
            mode.session().agent().set_model(model);
            mode.compaction_queued_messages
                .borrow_mut()
                .push(CompactionQueuedMessage {
                    text: "Queued unauthenticated prompt".into(),
                    mode: "followUp",
                });
            mode.flush_compaction_queue(false).await;
            let pending = JsString::join(mode.pending_messages_container.borrow_mut().render(120), "\n");
            let chat = JsString::join(mode.chat_container.borrow_mut().render(120), "\n");
            assert!(pending.contains("Queued unauthenticated prompt"), "{pending:?}");
            assert!(
                chat.contains("Failed to send queued message: No API key found for unconfigured-audit"),
                "{chat:?}"
            );
        })
        .await;
}
