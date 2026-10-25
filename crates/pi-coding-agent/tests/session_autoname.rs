use pi_js_compat::js_json::JsString;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/tool_context.rs"]
mod tool_context;

// `extensions/session-autoname/src/index.ts`: metadata-authoritative naming.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pi_agent_core::agent::AgentMessage;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, RegisterFauxProviderOptions,
    faux_assistant_message, faux_provider,
};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, StopReason, TextContent, Usage, UserContent, UserMessage,
    UserRole,
};
use pi_coding_agent::bundled_extensions::session_autoname::{
    build_title_context, decide_autoname_turn, normalize_title, read_generated_title, truncate_title,
};
use pi_coding_agent::core::agent_session::{AgentSession, ExtensionBindings};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::types::{
    AgentEndEvent, AgentEndSessionContinuation, ExtensionEvent, ExtensionMode,
};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::InlineExtension;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, NoTools, create_agent_session};
use pi_coding_agent::core::session_control_db::get_control_db_path;
use pi_coding_agent::core::session_manager::{SessionManager, lock_session_manager};
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use pi_js_compat::js_json as json;

fn user_message(content: &str, input_source: Option<pi_ai::types::InputSource>, timestamp: i64) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(content.to_owned().into()),
        input_source,
        provider_native: None,
        timestamp: timestamp as f64,
    })
}

fn assistant_message(text: &str) -> AgentMessage {
    AgentMessage::Assistant(Box::new(AssistantMessage {
        role: AssistantRole::Tag,
        content: vec![AssistantContent::Text(TextContent::new(text))],
        api: "faux".to_owned(),
        provider: "faux".to_owned(),
        model: "faux-1".to_owned(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0.0,
    }))
}

#[test]
fn turn_decision_tracks_pending_real_user_turns_across_continuations() {
    let extension_only = AgentEndEvent {
        messages: vec![user_message("extension", Some(pi_ai::types::InputSource::Extension), 1)],
        session_continuation: None,
    };
    let decision = decide_autoname_turn(&extension_only, false);
    assert!(!decision.should_attempt);
    assert!(!decision.pending_real_user_turn);

    let continuation_with_real_turn = AgentEndEvent {
        messages: vec![user_message("real", Some(pi_ai::types::InputSource::Interactive), 1)],
        session_continuation: Some(AgentEndSessionContinuation::CwdRelocation),
    };
    let decision = decide_autoname_turn(&continuation_with_real_turn, false);
    assert!(!decision.should_attempt, "continuations never attempt directly");
    assert!(decision.pending_real_user_turn);

    let decision = decide_autoname_turn(&extension_only, true);
    assert!(
        decision.should_attempt,
        "the pending real turn survives extension-only activity"
    );
    assert!(!decision.pending_real_user_turn);

    let decision = decide_autoname_turn(&extension_only, false);
    assert!(!decision.should_attempt, "the pending flag is consumed once");
}

#[test]
fn titles_are_normalized_like_the_typescript_helper() {
    assert_eq!(
        normalize_title(&JsString::from("## Session Title: Fix Login")),
        "Fix Login"
    );
    assert_eq!(
        normalize_title(&JsString::from("title - Port Control DB")),
        "Port Control DB"
    );
    assert_eq!(
        normalize_title(&JsString::from("  **\"Quoted Name\"**  ")),
        "Quoted Name"
    );
    assert_eq!(
        normalize_title(&JsString::from("\n\n   spaced   out   \nsecond line")),
        "spaced out"
    );
    assert_eq!(normalize_title(&JsString::from("")), "");
}

#[test]
fn titles_truncate_at_word_boundaries_and_hard_cap() {
    assert_eq!(truncate_title(&JsString::from("short title")), "short title");
    let long_words = "a".repeat(120);
    assert_eq!(truncate_title(&JsString::from(long_words)), "a".repeat(80));
    let sentence = "Port the control database schema to version fifteen";
    let truncated = truncate_title(&JsString::from(sentence));
    assert!(truncated.len() <= 80);
    assert!(JsString::from(sentence).starts_with(&truncated));
}

#[test]
fn generated_titles_map_stop_reasons_to_outcomes() {
    let mut aborted = faux_assistant_message("ignored", FauxAssistantMessageOptions::default());
    aborted.stop_reason = StopReason::Aborted;
    assert_eq!(read_generated_title(&aborted).expect("aborted"), None);

    let mut failed = faux_assistant_message("", FauxAssistantMessageOptions::default());
    failed.stop_reason = StopReason::Error;
    failed.error_message = Some("invalid_api_key".to_owned().into());
    assert_eq!(
        read_generated_title(&failed).expect_err("error").to_string(),
        "invalid_api_key"
    );

    let mut limited = faux_assistant_message("", FauxAssistantMessageOptions::default());
    limited.stop_reason = StopReason::Length;
    assert_eq!(
        read_generated_title(&limited).expect_err("length").to_string(),
        "title request exceeded its output limit"
    );

    let empty = faux_assistant_message("   ", FauxAssistantMessageOptions::default());
    assert_eq!(
        read_generated_title(&empty).expect_err("empty").to_string(),
        "title request returned no text"
    );

    let titled = faux_assistant_message("Session Title: Control DB v15", FauxAssistantMessageOptions::default());
    assert_eq!(
        read_generated_title(&titled).expect("title"),
        Some(JsString::from("Control DB v15"))
    );
}

#[test]
fn title_context_bounds_the_transcript_and_labels_missing_responses() {
    let exchange = pi_coding_agent::bundled_extensions::session_autoname::FirstExchange {
        user_text: "Name this".to_owned().into(),
        assistant_text: String::new().into(),
    };
    let context = build_title_context(&exchange);
    assert!(
        context
            .system_prompt
            .clone()
            .unwrap_or_default()
            .contains("session title")
    );
    assert_eq!(context.messages.len(), 1);
    let message = &context.messages[0];
    let pi_ai::types::Message::User(user) = message else {
        panic!("expected a user message");
    };
    let pi_ai::types::UserContent::Text(text) = &user.content else {
        panic!("expected text content");
    };
    assert!(text.contains("User request:\nName this"));
    assert!(text.contains("Assistant response:\n(none)"));
}

struct Env {
    session: AgentSession,
    faux: FauxProviderHandle,
    control_db_path: String,
    session_file: JsString,
    extension_name_events: Arc<Mutex<Vec<Option<JsString>>>>,
    _dir: tempfile::TempDir,
}

async fn env() -> Env {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path().join("repo").display().to_string();
    std::fs::create_dir_all(&cwd).expect("cwd");
    let agent_dir = dir.path().join("agent").display().to_string();
    std::fs::create_dir_all(&agent_dir).expect("agent dir");
    let session_dir = dir.path().join("sessions").display().to_string();
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "faux-1".to_owned(),
            name: Some("Faux Model".to_owned()),
            reasoning: Some(true),
            input: None,
            cost: None,
            context_window: Some(100_000.0),
            max_tokens: Some(4096.0),
        }]),
        ..Default::default()
    });
    let mut credentials = pi_js_compat::js_json::JsObject::new();
    credentials.insert(JsString::from("faux"), json!({"type": "api_key", "key": "faux-key"}));
    let model_registry = ModelRegistry::in_memory(AuthStorage::in_memory(credentials)).unwrap();
    scripted_registry::register_scripted_provider(&model_registry, Arc::clone(&faux.provider));
    let model = faux.get_model(None).expect("faux model");

    let control_db_path = get_control_db_path(pi_js_compat::node_fs::decode_node_path(
        (std::path::Path::new(&agent_dir)).as_os_str(),
    ))
    .to_utf8()
    .expect("fixture control path is scalar");
    let mut manager = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(session_dir)), None)
        .expect("session manager");
    manager
        .set_metadata_control_db_path(
            Some(&JsString::from(&control_db_path)),
            pi_coding_agent::core::session_manager::SetMetadataControlDbPathOptions::default(),
        )
        .expect("control db");
    manager.persist_for_recovery().expect("persist header");
    let session_file = manager.get_session_file().expect("session file").to_owned();

    let extension_name_events = Arc::new(Mutex::new(Vec::new()));
    let event_sink = Arc::clone(&extension_name_events);
    let factory: pi_coding_agent::core::extensions::ExtensionFactory = Arc::new(move |api| {
        let event_sink = Arc::clone(&event_sink);
        let result =
            pi_coding_agent::bundled_extensions::session_autoname::session_autoname_extension(api).and_then(|()| {
                api.on_session_info_changed(
                    move |event: pi_coding_agent::core::extensions::types::SessionInfoChangedEvent, _ctx| {
                        let event_sink = Arc::clone(&event_sink);
                        Box::pin(async move {
                            event_sink.lock().expect("extension events").push(event.name);
                            Ok(None)
                        }) as pi_ai::request::BoxFuture<'static, _>
                    },
                )
                .map_err(pi_coding_agent::core::extensions::types::HandlerError::new)
            });
        Box::pin(async move { result })
    });
    let settings_manager = SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default());
    let result = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd.into()),
        agent_dir: Some(agent_dir.into()),
        model_registry: Some(model_registry),
        model: Some(model),
        thinking_level: Some(ThinkingLevel::Off),
        no_tools: Some(NoTools::All),
        session_manager: Some(manager),
        settings_manager: Some(Arc::new(Mutex::new(settings_manager))),
        extension_factories: vec![InlineExtension::Factory {
            extension_path: Some("<inline:session-autoname>".to_owned().into()),
            factory,
        }],
        ..Default::default()
    })
    .await
    .expect("session");
    assert!(
        result.extensions_result.errors.is_empty(),
        "{:?}",
        result.extensions_result.errors
    );
    let session = result.session;
    session
        .bind_extensions(ExtensionBindings {
            mode: Some(ExtensionMode::Tui),
            control_db_path: Some(control_db_path.clone().into()),
            ..Default::default()
        })
        .await
        .expect("bind extensions");
    Env {
        session,
        faux,
        control_db_path,
        session_file,
        extension_name_events,
        _dir: dir,
    }
}

fn faux_call_count(faux: &FauxProviderHandle) -> u64 {
    faux.state().lock().expect("faux state").call_count
}

async fn emit_agent_end(session: &AgentSession, event: AgentEndEvent) {
    let mut event = ExtensionEvent::AgentEnd(event);
    session.extension_runner().emit(&mut event).await;
}

async fn wait_for_name(session: &AgentSession) -> Option<JsString> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(name) = lock_session_manager(&session.session_manager_handle()).get_session_name() {
            return Some(name);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

fn persisted_session_name(env: &Env) -> Option<JsString> {
    pi_coding_agent::core::session_control_db::read_session_metadata(&env.control_db_path, &env.session_file)
        .expect("metadata")
        .and_then(|metadata| metadata.name)
}

#[tokio::test]
async fn extension_only_activity_after_a_historical_real_turn_does_not_name_the_session() {
    let env = env().await;
    {
        let handle = env.session.session_manager_handle();
        let mut manager = lock_session_manager(&handle);
        manager
            .append_message(user_message("Historical real-user request.", None, 1))
            .expect("user");
        manager
            .append_message(assistant_message("Historical response."))
            .expect("assistant");
        manager
            .append_message(user_message(
                "Extension continuation.",
                Some(pi_ai::types::InputSource::Extension),
                2,
            ))
            .expect("extension user");
        manager
            .append_message(assistant_message("Extension response."))
            .expect("extension assistant");
    }
    env.faux.set_responses(vec![
        faux_assistant_message("Incorrect Historical Name", FauxAssistantMessageOptions::default()).into(),
    ]);

    emit_agent_end(
        &env.session,
        AgentEndEvent {
            messages: vec![
                user_message("Extension continuation.", Some(pi_ai::types::InputSource::Extension), 2),
                assistant_message("Extension response."),
            ],
            session_continuation: None,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(faux_call_count(&env.faux), 0);
    assert_eq!(
        lock_session_manager(&env.session.session_manager_handle()).get_session_name(),
        None
    );
}

#[tokio::test]
async fn the_first_real_user_turn_after_extension_activity_names_the_session() {
    let env = env().await;
    {
        let handle = env.session.session_manager_handle();
        let mut manager = lock_session_manager(&handle);
        manager
            .append_message(user_message(
                "Internal extension response.",
                Some(pi_ai::types::InputSource::Extension),
                1,
            ))
            .expect("extension user");
        manager
            .append_message(assistant_message("Internal extension response."))
            .expect("extension assistant");
        manager
            .append_message(user_message(
                "First real request.",
                Some(pi_ai::types::InputSource::Interactive),
                2,
            ))
            .expect("real user");
        manager
            .append_message(assistant_message("Response to the first real user prompt."))
            .expect("assistant");
    }
    env.faux.set_responses(vec![
        faux_assistant_message("Real User Session", FauxAssistantMessageOptions::default()).into(),
    ]);

    emit_agent_end(
        &env.session,
        AgentEndEvent {
            messages: vec![user_message(
                "Internal extension response.",
                Some(pi_ai::types::InputSource::Extension),
                1,
            )],
            session_continuation: Some(AgentEndSessionContinuation::CwdRelocation),
        },
    )
    .await;
    emit_agent_end(
        &env.session,
        AgentEndEvent {
            messages: vec![user_message(
                "First real request.",
                Some(pi_ai::types::InputSource::Interactive),
                2,
            )],
            session_continuation: None,
        },
    )
    .await;

    assert_eq!(
        wait_for_name(&env.session).await.clone(),
        Some(JsString::from("Real User Session"))
    );
    assert_eq!(faux_call_count(&env.faux), 1);
    assert_eq!(
        persisted_session_name(&env).clone(),
        Some(JsString::from("Real User Session"))
    );
}

#[tokio::test]
async fn historical_jsonl_session_info_does_not_block_naming_without_metadata_state() {
    let env = env().await;
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(env.session_file.to_utf8().expect("utf-8 test path"))
            .expect("open session file");
        use std::io::Write;
        writeln!(
            file,
            "{}",
            pi_js_compat::js_json::stringify_lossless(&json!({
                "type": "session_info",
                "id": "legacy-session-name",
                "parentId": null,
                "timestamp": "2025-01-01T00:00:00.000Z",
                "name": "Historical JSONL Name",
            }))
        )
        .expect("append session info");
    }
    let manager_handle = env.session.session_manager_handle();
    {
        let mut manager = lock_session_manager(&manager_handle);
        manager
            .set_session_file(&JsString::from(env.session_file))
            .expect("reload session file");
        manager
            .append_message(user_message(
                "Current real-user request.",
                Some(pi_ai::types::InputSource::Interactive),
                3,
            ))
            .expect("real user");
        manager
            .append_message(assistant_message("Current response."))
            .expect("assistant");
        assert_eq!(manager.get_session_name(), None);
        assert!(!manager.has_session_name_state());
    }
    env.faux.set_responses(vec![
        faux_assistant_message("Metadata Authority Name", FauxAssistantMessageOptions::default()).into(),
    ]);

    emit_agent_end(
        &env.session,
        AgentEndEvent {
            messages: vec![user_message(
                "Current real-user request.",
                Some(pi_ai::types::InputSource::Interactive),
                3,
            )],
            session_continuation: None,
        },
    )
    .await;

    assert_eq!(
        wait_for_name(&env.session).await.clone(),
        Some(JsString::from("Metadata Authority Name"))
    );
}

#[tokio::test]
async fn an_explicit_clear_before_the_first_exchange_blocks_naming() {
    let env = env().await;
    env.session.clear_session_name().expect("clear name");
    {
        let handle = env.session.session_manager_handle();
        let mut manager = lock_session_manager(&handle);
        manager
            .append_message(user_message(
                "Keep this session unnamed.",
                Some(pi_ai::types::InputSource::Interactive),
                4,
            ))
            .expect("real user");
        manager
            .append_message(assistant_message("Substantive answer."))
            .expect("assistant");
    }
    env.faux.set_responses(vec![
        faux_assistant_message(
            "Generated Name Should Not Replace Clear",
            FauxAssistantMessageOptions::default(),
        )
        .into(),
    ]);

    emit_agent_end(
        &env.session,
        AgentEndEvent {
            messages: vec![user_message(
                "Keep this session unnamed.",
                Some(pi_ai::types::InputSource::Interactive),
                4,
            )],
            session_continuation: None,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(faux_call_count(&env.faux), 0);
    assert_eq!(
        lock_session_manager(&env.session.session_manager_handle()).get_session_name(),
        None
    );
    assert!(lock_session_manager(&env.session.session_manager_handle()).has_session_name_state());
    assert_eq!(persisted_session_name(&env), None);
}

#[tokio::test]
async fn a_name_set_before_the_first_exchange_blocks_naming() {
    let env = env().await;
    env.session
        .set_session_name(&JsString::from("Manual Session Name"))
        .expect("set name");
    {
        let handle = env.session.session_manager_handle();
        let mut manager = lock_session_manager(&handle);
        manager
            .append_message(user_message(
                "Substantive request.",
                Some(pi_ai::types::InputSource::Interactive),
                5,
            ))
            .expect("real user");
        manager
            .append_message(assistant_message("Substantive answer."))
            .expect("assistant");
    }
    env.faux.set_responses(vec![
        faux_assistant_message("Unexpected Name", FauxAssistantMessageOptions::default()).into(),
    ]);

    emit_agent_end(
        &env.session,
        AgentEndEvent {
            messages: vec![user_message(
                "Substantive request.",
                Some(pi_ai::types::InputSource::Interactive),
                5,
            )],
            session_continuation: None,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(faux_call_count(&env.faux), 0);
    assert_eq!(
        lock_session_manager(&env.session.session_manager_handle())
            .get_session_name()
            .clone(),
        Some(JsString::from("Manual Session Name"))
    );
}

#[tokio::test]
async fn naming_failures_are_reported_without_setting_a_name() {
    let env = env().await;
    {
        let handle = env.session.session_manager_handle();
        let mut manager = lock_session_manager(&handle);
        manager
            .append_message(user_message(
                "Please name me.",
                Some(pi_ai::types::InputSource::Interactive),
                6,
            ))
            .expect("real user");
        manager.append_message(assistant_message("Sure.")).expect("assistant");
    }
    let mut failure = faux_assistant_message("", FauxAssistantMessageOptions::default());
    failure.stop_reason = StopReason::Error;
    failure.error_message = Some("invalid_api_key".to_owned().into());
    env.faux.set_responses(vec![failure.into()]);

    emit_agent_end(
        &env.session,
        AgentEndEvent {
            messages: vec![user_message(
                "Please name me.",
                Some(pi_ai::types::InputSource::Interactive),
                6,
            )],
            session_continuation: None,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(faux_call_count(&env.faux), 1);
    assert_eq!(
        lock_session_manager(&env.session.session_manager_handle()).get_session_name(),
        None
    );
    assert_eq!(persisted_session_name(&env), None);
}

#[test]
fn titles_use_javascript_whitespace_not_rust_unicode_whitespace() {
    assert_eq!(
        normalize_title(&JsString::from(
            "###\u{feff}Session\u{feff}Title:\u{feff}Keep\u{feff}Words"
        )),
        "Keep Words"
    );
    assert_eq!(normalize_title(&JsString::from("Keep\u{85}Words")), "Keep\u{85}Words");
}

fn seed_exchange(env: &Env, stop: StopReason, assistant_text: &str) -> Vec<AgentMessage> {
    let messages = vec![
        user_message("Fix schema migration", Some(pi_ai::types::InputSource::Interactive), 1),
        {
            let mut message = faux_assistant_message(assistant_text, Default::default());
            message.stop_reason = stop;
            AgentMessage::from(message)
        },
    ];
    let handle = env.session.session_manager_handle();
    let mut manager = lock_session_manager(&handle);
    for message in &messages {
        manager.append_message(message.clone()).expect("seed message");
    }
    messages
}

#[tokio::test]
async fn title_outputs_match_the_real_c248_extension() {
    let mut oracle = oracle::TsOracle::spawn(
        "session-autoname-oracle.ts",
        "session-names:packages/coding-agent/extensions/session-autoname/src/index.ts",
    );
    let cases = [
        "Schema Migration Repair".to_owned(),
        "\n### \"Automatic Session Naming\"\n".to_owned(),
        "Session Title: Control DB v15".to_owned(),
        "name - Preserve Session Names".to_owned(),
        "**'Quoted Title'**".to_owned(),
        "\u{feff}###\u{feff}Session\u{feff}Title:\u{feff}Keep\u{feff}Words".to_owned(),
        "Keep\u{85}Words".to_owned(),
        "Port\t  SQLite\tNames\nignored second line".to_owned(),
        "界".repeat(95),
        "x".repeat(120),
        "Longword ".repeat(20),
        format!("{}🙂", "x".repeat(80)),
    ];
    for title in cases {
        let expected = oracle.request(&json!({"title": title}));
        let env = env().await;
        let names = Arc::new(Mutex::new(Vec::<Option<JsString>>::new()));
        let sink = Arc::clone(&names);
        env.session.subscribe(Arc::new(
            move |event: &pi_coding_agent::core::agent_session::AgentSessionEvent| {
                if let pi_coding_agent::core::agent_session::AgentSessionEvent::SessionInfoChanged(event) = event {
                    sink.lock().expect("events").push(event.name.clone());
                }
            },
        ));
        let handle = env.session.session_manager_handle();
        lock_session_manager(&handle)
            .append_message(user_message("Fix schema migration", None, 1))
            .expect("user");
        env.faux
            .set_responses(vec![faux_assistant_message(title.clone(), Default::default()).into()]);
        emit_agent_end(
            &env.session,
            AgentEndEvent {
                messages: vec![user_message("Fix schema migration", None, 1)],
                session_continuation: None,
            },
        )
        .await;
        assert!(wait_for_name(&env.session).await.is_some(), "title {title:?}");
        let actual = json!({
            "names": names.lock().expect("events").clone(),
            "errors": [],
            "calls": faux_call_count(&env.faux),
            "hasStoredValue": lock_session_manager(&handle).has_session_name_state(),
        });
        assert_eq!(
            pi_js_compat::js_json::stringify_lossless(&actual),
            pi_js_compat::js_json::stringify_lossless(&expected),
            "title {title:?}"
        );
    }
}

#[tokio::test]
async fn modes_and_ephemeral_subagent_boundaries_control_naming() {
    for (mode, persisted, subagent, expected_calls) in [
        (ExtensionMode::Rpc, true, false, 1),
        (ExtensionMode::Print, true, false, 0),
        (ExtensionMode::Json, true, false, 0),
        (ExtensionMode::Tui, false, false, 0),
        (ExtensionMode::Tui, true, true, 0),
    ] {
        let env = env().await;
        let handle = env.session.session_manager_handle();
        if !persisted {
            *lock_session_manager(&handle) =
                SessionManager::in_memory(Some(&JsString::from("/tmp")), None).expect("ephemeral");
        }
        if subagent {
            lock_session_manager(&handle)
                .new_session(Some(pi_coding_agent::core::session_manager::NewSessionOptions {
                    is_subagent: true,
                    ..Default::default()
                }))
                .expect("subagent");
        }
        env.session
            .bind_extensions(ExtensionBindings {
                mode: Some(mode),
                ..Default::default()
            })
            .await
            .expect("rebind extensions");
        let messages = seed_exchange(&env, StopReason::Stop, "Substantive answer");
        env.faux.set_responses(vec![
            faux_assistant_message("RPC Session Autonaming", Default::default()).into(),
        ]);
        emit_agent_end(
            &env.session,
            AgentEndEvent {
                messages,
                session_continuation: None,
            },
        )
        .await;
        if expected_calls == 1 {
            assert_eq!(
                wait_for_name(&env.session).await.clone(),
                Some(JsString::from("RPC Session Autonaming"))
            );
        } else {
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert_eq!(lock_session_manager(&handle).get_session_name(), None);
        }
        assert_eq!(faux_call_count(&env.faux), expected_calls);
    }
}

#[tokio::test]
async fn empty_failed_aborted_and_partially_failed_exchanges_still_name_from_the_first_user() {
    for (reason, response) in [
        (StopReason::Stop, ""),
        (StopReason::Error, ""),
        (StopReason::Aborted, ""),
        (StopReason::Length, "partial discarded"),
    ] {
        let env = env().await;
        let mut messages = seed_exchange(&env, reason, response);
        if reason == StopReason::Aborted {
            let second = user_message("Tell me more", None, 2);
            env.session
                .session_manager()
                .append_message(second.clone())
                .expect("second user");
            messages.push(second);
        }
        env.faux.set_responses(vec![
            faux_assistant_message("Named From User Prompt", Default::default()).into(),
        ]);
        emit_agent_end(
            &env.session,
            AgentEndEvent {
                messages,
                session_continuation: None,
            },
        )
        .await;
        assert_eq!(
            wait_for_name(&env.session).await.clone(),
            Some(JsString::from("Named From User Prompt"))
        );
    }
    let env = env().await;
    let mut messages = seed_exchange(&env, StopReason::Stop, "Partial progress before failure");
    let mut failure = faux_assistant_message("", Default::default());
    failure.stop_reason = StopReason::Error;
    env.session
        .session_manager()
        .append_message(AgentMessage::from(failure.clone()))
        .expect("failure");
    messages.push(AgentMessage::from(failure));
    env.faux.set_responses(vec![
        faux_assistant_message("Failed Work Session", Default::default()).into(),
    ]);
    emit_agent_end(
        &env.session,
        AgentEndEvent {
            messages,
            session_continuation: None,
        },
    )
    .await;
    assert_eq!(
        wait_for_name(&env.session).await.clone(),
        Some(JsString::from("Failed Work Session"))
    );
}

#[tokio::test]
async fn cwd_continuation_defers_until_the_following_agent_end() {
    let env = env().await;
    let messages = seed_exchange(&env, StopReason::ToolUse, "Relocation started");
    env.faux.set_responses(vec![
        faux_assistant_message("Relocated Project Work", Default::default()).into(),
    ]);
    emit_agent_end(
        &env.session,
        AgentEndEvent {
            messages,
            session_continuation: Some(AgentEndSessionContinuation::CwdRelocation),
        },
    )
    .await;
    assert_eq!(faux_call_count(&env.faux), 0);
    emit_agent_end(
        &env.session,
        AgentEndEvent {
            messages: vec![],
            session_continuation: None,
        },
    )
    .await;
    assert_eq!(
        wait_for_name(&env.session).await.clone(),
        Some(JsString::from("Relocated Project Work"))
    );
}

async fn wait_for_calls(faux: &FauxProviderHandle, calls: u64) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while faux_call_count(faux) < calls {
        assert!(Instant::now() < deadline, "title request did not start");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

fn delayed_title(release: Arc<tokio::sync::Notify>) -> pi_ai::providers::faux::FauxResponseStep {
    pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(move |_context, _options, _state, _model| {
        let release = Arc::clone(&release);
        Box::pin(async move {
            release.notified().await;
            Ok(faux_assistant_message("Generated Session Name", Default::default()))
        })
    }))
}

#[tokio::test]
async fn shutdown_manual_clear_and_manual_name_cancel_pending_title_requests() {
    for action in ["shutdown", "clear", "name"] {
        let env = env().await;
        let messages = seed_exchange(&env, StopReason::Stop, "Substantive answer");
        let release = Arc::new(tokio::sync::Notify::new());
        env.faux.set_responses(vec![delayed_title(Arc::clone(&release))]);
        emit_agent_end(
            &env.session,
            AgentEndEvent {
                messages,
                session_continuation: None,
            },
        )
        .await;
        wait_for_calls(&env.faux, 1).await;
        match action {
            "shutdown" => {
                let mut event =
                    ExtensionEvent::SessionShutdown(pi_coding_agent::core::extensions::types::SessionShutdownEvent {
                        reason: pi_coding_agent::core::extensions::types::SessionShutdownReason::Reload,
                        target_session_file: None,
                    });
                env.session.extension_runner().emit(&mut event).await;
            }
            "clear" => env.session.clear_session_name().expect("manual clear"),
            "name" => env
                .session
                .set_session_name(&JsString::from("Manual Name During Generation"))
                .expect("manual name"),
            _ => unreachable!(),
        }
        release.notify_one();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let expected = (action == "name").then_some("Manual Name During Generation");
        assert_eq!(
            env.session.session_name().clone(),
            expected.map(JsString::from),
            "{action}"
        );
    }
}

#[tokio::test]
async fn prompt_settles_before_the_background_title_response() {
    let env = env().await;
    let release = Arc::new(tokio::sync::Notify::new());
    let main = faux_assistant_message(
        vec![
            pi_ai::providers::faux::faux_text("Substantive answer"),
            pi_ai::providers::faux::faux_tool_call(
                "end_turn",
                pi_js_compat::js_json::JsObject::from_iter([(JsString::from("reason"), json!("answered"))]),
                Some("end-main".to_owned()),
            ),
        ],
        Default::default(),
    );
    env.faux
        .set_responses(vec![main.into(), delayed_title(Arc::clone(&release))]);
    let settled = tokio::time::timeout(
        Duration::from_secs(2),
        env.session.prompt(
            "Name this coding session",
            pi_coding_agent::core::agent_session::PromptOptions::default(),
        ),
    )
    .await;
    release.notify_one();
    settled
        .expect("prompt must not wait for title generation")
        .expect("prompt");
    assert_eq!(
        wait_for_name(&env.session).await.clone(),
        Some(JsString::from("Generated Session Name"))
    );
    assert_eq!(faux_call_count(&env.faux), 2);
}

#[tokio::test]
async fn split_surrogate_titles_match_typescript_at_the_sqlite_boundary() {
    let mut oracle = oracle::TsOracle::spawn(
        "session-autoname-oracle.ts",
        "session-names:packages/coding-agent/extensions/session-autoname/src/index.ts",
    );
    let input = format!("{}🙂", "x".repeat(79));
    let source = oracle.request(&json!({"title":input,"probeUTF16":true}));
    assert_eq!(
        source["helperNameJson"],
        json!(format!("\"{}\\ud83d\"", "x".repeat(79)))
    );
    assert_eq!(
        source["helperUnits"].as_array().expect("units").last(),
        Some(&json!(0xd83d))
    );
    assert_eq!(source["storedNameJson"], json!(format!("\"{}�\"", "x".repeat(79))));
    let env = env().await;
    let rust_title = normalize_title(&JsString::from(input));
    env.session.set_session_name(&rust_title).expect("persist Rust name");
    let expected_stored: String = pi_js_compat::js_json::from_value(&source["storedNameJson"].clone())
        .and_then(|encoded: String| serde_json::from_str(&encoded))
        .expect("TypeScript stored name");
    assert_eq!(persisted_session_name(&env), Some(JsString::from(expected_stored)));
}

#[tokio::test]
async fn child_agent_context_is_ineligible_even_without_a_subagent_header() {
    let env = env().await;
    let handle = env.session.session_manager_handle();
    let (_, context) = tool_context::TestContextBuilder::new(&env._dir.path().display().to_string(), handle)
        .mode(ExtensionMode::Rpc)
        .agent_id("child-agent")
        .build()
        .await;
    assert!(
        !pi_coding_agent::bundled_extensions::session_autoname::can_autoname_session(&context).expect("eligibility")
    );
}

#[tokio::test]
async fn title_requests_carry_the_source_limits_and_abort_after_fifteen_seconds() {
    let env = env().await;
    let messages = seed_exchange(&env, StopReason::Stop, "Substantive answer");
    let aborted = Arc::new(tokio::sync::Notify::new());
    let observe_abort = Arc::clone(&aborted);
    env.faux
        .set_responses(vec![pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
            move |context, options, _state, model| {
                let observe_abort = Arc::clone(&observe_abort);
                Box::pin(async move {
                    assert_eq!(model.id, "faux-1");
                    assert!(context.system_prompt.expect("title prompt").contains("session title"));
                    let options = options.expect("simple request options");
                    assert_eq!(options.stream.max_tokens, Some(64.0));
                    assert_eq!(options.stream.request.max_retries, Some(1.0));
                    assert_eq!(options.stream.request.max_retry_delay_ms, Some(3000.0));
                    assert_eq!(options.stream.request.timeout_ms, Some(15000.0));
                    options.stream.request.signal.expect("abort signal").wait().await;
                    observe_abort.notify_one();
                    let mut response = faux_assistant_message("", Default::default());
                    response.stop_reason = StopReason::Aborted;
                    Ok(response)
                })
            },
        ))]);
    emit_agent_end(
        &env.session,
        AgentEndEvent {
            messages,
            session_continuation: None,
        },
    )
    .await;
    tokio::time::timeout(Duration::from_secs(25), aborted.notified())
        .await
        .expect("15-second abort");
    assert_eq!(env.session.session_name(), None);
    assert_eq!(faux_call_count(&env.faux), 1);
    assert!(!env.session.session_manager().has_session_name_state());
}

#[tokio::test]
async fn set_and_clear_publish_name_events_to_extensions_in_order() {
    let env = env().await;
    env.session
        .set_session_name(&JsString::from("first"))
        .expect("first name");
    env.session
        .set_session_name(&JsString::from("second"))
        .expect("second name");
    env.session.clear_session_name().expect("clear name");
    let deadline = Instant::now() + Duration::from_secs(2);
    while env.extension_name_events.lock().expect("events").len() < 3 {
        assert!(Instant::now() < deadline, "extension name events missing");
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert_eq!(
        *env.extension_name_events.lock().expect("events"),
        vec![Some(JsString::from("first")), Some(JsString::from("second")), None,]
    );
    assert!(env.session.session_manager().has_session_name_state());
}

#[path = "support/scripted_registry.rs"]
mod scripted_registry;
