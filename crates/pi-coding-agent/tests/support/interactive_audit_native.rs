#[tokio::test(flavor = "current_thread")]
async fn native_audit_live_empty_assistant_error_completes_pending_tool_with_error() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mode, _, _, _temp) = build_mode().await;
            mode.ensure_tool_component("read", "audit-pending", pi_js_compat::js_json!({"path":"fixture.txt"}));
            let view = mode
                .pending_tools
                .borrow()
                .get(&JsString::from("audit-pending"))
                .unwrap()
                .clone();
            let mut message = faux_assistant_message("", Default::default());
            message.stop_reason = pi_ai::types::StopReason::Error;
            message.error_message = Some(JsString::default());
            mode.handle_event(AgentSessionEvent::Agent(AgentEvent::MessageStart {
                message: message.clone().into(),
            }))
            .await;
            mode.handle_event(AgentSessionEvent::Agent(AgentEvent::MessageEnd {
                message: message.into(),
            }))
            .await;
            let output = JsString::join(view.borrow_mut().render(100), "\n");
            assert!(output.contains("Error"), "{output:?}");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn native_audit_summary_edit_precedes_selected_child_steering() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let store = test_store();
            let child = spawn_agent(&store, "Child", Some(1), None, None);
            let (mode, _, _, _temp) = build_mode_with_store(Arc::clone(&store)).await;
            let user: AgentMessage = pi_js_compat::js_json::from_value(
                &pi_js_compat::js_json!({"role":"user","content":"Original context","timestamp":1000}),
            )
            .unwrap();
            let first = mode.session().session_manager().append_message(user).unwrap();
            let id = mode
                .session()
                .session_manager()
                .append_compaction(&"Old summary".into(), &first, 100.0, Default::default())
                .unwrap();
            *mode.pending_compaction_summary_edit_id.borrow_mut() = Some(id.clone());
            store.select_agent_view(&child);
            mode.submit("Rewritten summary".into()).await.unwrap();
            let entry = mode.session().session_manager().get_entry(&id).unwrap().clone();
            let Some(KnownSessionEntry::Compaction(compaction)) = entry.known().as_deref().cloned() else {
                panic!("expected compaction");
            };
            assert_eq!(compaction.summary, "Rewritten summary");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn native_audit_prepared_user_transit_and_duplicate_guard_visibility() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mode, _, _, _temp) = build_mode().await;
            let user: AgentMessage = pi_js_compat::js_json::from_value(
                &pi_js_compat::js_json!({"role":"user","content":"Prepared question","timestamp":1000}),
            )
            .unwrap();
            mode.render_prepared_user_message(&user);
            let pending = JsString::join(mode.chat_container.borrow_mut().render(100), "\n");
            assert!(pending.contains("⧗ sending…"), "{pending:?}");
            mode.handle_event(AgentSessionEvent::Agent(AgentEvent::MessageStart { message: user }))
                .await;
            let started = JsString::join(mode.chat_container.borrow_mut().render(100), "\n");
            assert!(!started.contains("⧗ sending…"));
            let guard: AgentMessage = pi_js_compat::js_json::from_value(
                &pi_js_compat::js_json!({"role":"user","content":"Internal turn guard","timestamp":2000}),
            )
            .unwrap();
            mode.handle_event(AgentSessionEvent::Message(
                crate::core::agent_session::AgentMessageEvent {
                    event: AgentEvent::MessageStart { message: guard },
                    runtime_message_marker: Some(
                        crate::core::runtime_message_markers::RuntimeMessageMarker::DuplicateTurnGuard,
                    ),
                },
            ))
            .await;
            assert_eq!(
                JsString::join(mode.chat_container.borrow_mut().render(100), "\n"),
                started
            );
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn native_audit_main_input_uses_trimmed_submission() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mode, _, faux, _temp) = build_mode().await;
            faux.set_responses(vec![completed_fixture_response("done")]);
            mode.submit("  Main prompt  ".into()).await.unwrap();
            let users = mode
                .session()
                .agent()
                .state()
                .messages
                .iter()
                .filter(|message| matches!(message, AgentMessage::User(_)))
                .map(InteractiveMode::get_user_message_text)
                .collect::<Vec<_>>();
            assert_eq!(users, vec![JsString::from("Main prompt")]);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn native_audit_mode_keeps_frozen_default_output_padding() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mode, _, _, _temp) = build_mode().await;
            mode.settings_mut(|settings| settings.set_output_pad(0));
            let message: AgentMessage = pi_js_compat::js_json::from_value(
                &pi_js_compat::js_json!({"role":"user","content":"Padding contract","timestamp":0}),
            )
            .unwrap();
            mode.add_message_to_chat(&message, false);
            let line = mode
                .chat_container
                .borrow_mut()
                .render(60)
                .into_iter()
                .find(|line| line.contains("Padding contract"))
                .unwrap();
            let plain = pi_tui::utils::strip_terminal_sequences(&line);
            assert!(plain.starts_with(" Padding contract"), "{plain:?}");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn native_audit_submit_during_compaction_updates_editor_and_pending_preview() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let runtime = crate::core::extensions::create_extension_runtime();
            let factory: crate::core::extensions::ExtensionFactory = Arc::new({
                let entered = Arc::clone(&entered);
                let release = Arc::clone(&release);
                move |api| {
                    let entered = Arc::clone(&entered);
                    let release = Arc::clone(&release);
                    api.on_session_compaction_source(
                        move |_: crate::core::extensions::SessionCompactionSourceEvent,
                              _: crate::core::extensions::ExtensionContext|
                              -> crate::core::extensions::BoxFuture<
                            'static,
                            Result<Option<crate::core::extensions::SessionCompactionSourceResult>, HandlerError>,
                        > {
                            let entered = Arc::clone(&entered);
                            let release = Arc::clone(&release);
                            Box::pin(async move {
                                entered.notify_one();
                                release.notified().await;
                                Ok(None)
                            })
                        },
                    )
                    .unwrap();
                    Box::pin(async { Ok(()) })
                }
            });
            let extension = crate::core::extensions::load_extension_from_factory(
                factory,
                &"/tmp".into(),
                Arc::new(crate::core::event_bus::EventBusController::new()),
                runtime.clone(),
                &"/tmp/audit-compaction".into(),
            )
            .await
            .unwrap();
            let loader = Arc::new(StaticResourceLoader {
                extensions: Some(crate::core::extensions::LoadExtensionsResult {
                    extensions: vec![extension],
                    errors: Vec::new(),
                    runtime,
                }),
                ..Default::default()
            });
            let (mode, _, _, _temp) = build_mode_with_loader(loader, None).await;
            let compact = tokio::task::spawn_local({
                let session = mode.session();
                async move { session.compact(None).await }
            });
            entered.notified().await;
            mode.editor()
                .borrow_mut()
                .set_text(&"Queued while compacting".into())
                .unwrap();
            mode.submit("Queued while compacting".into()).await.unwrap();
            let text = mode.editor().borrow().get_text().unwrap();
            let pending = JsString::join(mode.pending_messages_container.borrow_mut().render(100), "\n");
            release.notify_one();
            let _result = compact.await.unwrap();
            assert!(text.is_empty());
            assert!(pending.contains("Steering: Queued while compacting"), "{pending:?}");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn native_audit_bash_renders_output_and_honors_interception() {
    tokio::task::LocalSet::new().run_until(async {
        use crate::core::extensions::types::{UserBashEvent,UserBashEventResult};
        let runtime=crate::core::extensions::create_extension_runtime();
        let factory:crate::core::extensions::ExtensionFactory=Arc::new(|api| {
            api.on_user_bash(|event:UserBashEvent,context:crate::core::extensions::ExtensionContext| -> crate::core::extensions::BoxFuture<'static,Result<Option<UserBashEventResult>,HandlerError>> {
                Box::pin(async move {
                    let manager = context.session_manager()?;
                    let cwd = tokio::task::spawn_blocking(move || manager.get_cwd()).await.map_err(|error|HandlerError::from(error.to_string()))?;
                    assert_eq!(cwd,event.cwd);
                    Ok((event.command == "intercept-audit").then(||UserBashEventResult {result:Some(pi_js_compat::js_json!({"output":"Intercepted output","exitCode":0,"cancelled":false,"truncated":false})),operations:None}))
                })
            }).unwrap();
            Box::pin(async {Ok(())})
        });
        let extension=crate::core::extensions::load_extension_from_factory(factory,&"/tmp".into(),Arc::new(crate::core::event_bus::EventBusController::new()),runtime.clone(),&"/tmp/audit-bash-extension".into()).await.unwrap();
        let loader=Arc::new(StaticResourceLoader {extensions:Some(crate::core::extensions::LoadExtensionsResult {extensions:vec![extension],errors:Vec::new(),runtime}),..Default::default()});
        let (mode,_,_,_temp)=build_mode_with_loader(loader,None).await;
        mode.init().await.unwrap();
        tokio::time::timeout(Duration::from_secs(2),mode.handle_bash_command("printf native-audit-output".into(),false)).await.expect("bash interception can read the current session");
        let chat=JsString::join(mode.chat_container.borrow_mut().render(120),"\n");
        assert!(chat.split('\n').any(|line|pi_tui::utils::strip_terminal_sequences(&line).trim() == "native-audit-output"),"{chat:?}");
        mode.handle_bash_command("intercept-audit".into(),false).await;
        let chat=JsString::join(mode.chat_container.borrow_mut().render(120),"\n");
        assert!(chat.contains("Intercepted output"),"{chat:?}");
        assert!(mode.pending_messages_container.borrow().children().is_empty());
        mode.stop().unwrap();
    }).await;
}

#[tokio::test(flavor = "current_thread")]
async fn native_audit_manual_compaction_has_single_event_display_authority() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mode, _, _, _temp) = build_mode().await;
            let mut model = mode.session().model();
            model.provider = "unconfigured-audit".to_owned();
            mode.session().agent().set_model(model);
            let loader = mode.create_working_loader(mode.get_working_loader_indicator(WorkingLoaderView::Main));
            *mode.loading_animation.borrow_mut() = Some(Rc::clone(&loader));
            mode.status_container.borrow_mut().add_child(loader);
            mode.handle_compact_command(None).await;
            assert!(mode.chat_container.borrow_mut().render(120).is_empty());
            assert!(mode.status_container.borrow_mut().render(120).is_empty());
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn native_audit_global_interrupt_cancels_review_without_editor_callback() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let (mode, terminal, _, _temp) = build_mode().await;
            mode.init().await.unwrap();
            mode.default_editor.borrow_mut().on_escape = None;
            let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let seen = Arc::clone(&cancelled);
            let _registration = mode
                .session()
                .register_supervisor_review_cancellation(Arc::new(move || {
                    seen.store(true, std::sync::atomic::Ordering::SeqCst);
                }));
            terminal.borrow_mut().send_input("\u{1b}");
            mode.ui.process_events();
            assert!(cancelled.load(std::sync::atomic::Ordering::SeqCst));
            mode.stop().unwrap();
        })
        .await;
}

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
                .filter(|message| matches!(message, AgentMessage::User(_)))
                .map(InteractiveMode::get_user_message_text)
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
