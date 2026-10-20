//! `extensions/session-autoname/src/index.ts`: metadata-authoritative naming.

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
use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
use pi_coding_agent::core::resource_loader::InlineExtension;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, NoTools, create_agent_session};
use pi_coding_agent::core::session_control_db::get_control_db_path;
use pi_coding_agent::core::session_manager::{SessionManager, lock_session_manager};
use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};
use serde_json::json;

fn user_message(content: &str, input_source: Option<pi_ai::types::InputSource>, timestamp: i64) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text(content.to_owned()),
        input_source,
        provider_native: None,
        timestamp,
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
        deferred: None,
        error_message: None,
        raw_stop_reason: None,
        end_turn: None,
        timestamp: 0,
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
    assert_eq!(normalize_title("## Session Title: Fix Login"), "Fix Login");
    assert_eq!(normalize_title("title - Port Control DB"), "Port Control DB");
    assert_eq!(normalize_title("  **\"Quoted Name\"**  "), "Quoted Name");
    assert_eq!(normalize_title("\n\n   spaced   out   \nsecond line"), "spaced out");
    assert_eq!(normalize_title(""), "");
}

#[test]
fn titles_truncate_at_word_boundaries_and_hard_cap() {
    assert_eq!(truncate_title("short title"), "short title");
    let long_words = "a".repeat(120);
    assert_eq!(truncate_title(&long_words), "a".repeat(80));
    let sentence = "Port the control database schema to version fifteen";
    let truncated = truncate_title(sentence);
    assert!(truncated.len() <= 80);
    assert!(sentence.starts_with(&truncated));
}

#[test]
fn generated_titles_map_stop_reasons_to_outcomes() {
    let mut aborted = faux_assistant_message("ignored", FauxAssistantMessageOptions::default());
    aborted.stop_reason = StopReason::Aborted;
    assert_eq!(read_generated_title(&aborted).expect("aborted"), None);

    let mut failed = faux_assistant_message("", FauxAssistantMessageOptions::default());
    failed.stop_reason = StopReason::Error;
    failed.error_message = Some("invalid_api_key".to_owned());
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
        Some("Control DB v15".to_owned())
    );
}

#[test]
fn title_context_bounds_the_transcript_and_labels_missing_responses() {
    let exchange = pi_coding_agent::bundled_extensions::session_autoname::FirstExchange {
        user_text: "Name this".to_owned(),
        assistant_text: String::new(),
    };
    let context = build_title_context(&exchange);
    assert!(
        context
            .system_prompt
            .as_deref()
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
    session_file: String,
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
    let mut credentials = serde_json::Map::new();
    credentials.insert("faux".to_owned(), json!({"type": "api_key", "key": "faux-key"}));
    let model_runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(credentials))),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .expect("model runtime");
    model_runtime.register_native_provider(Arc::clone(&faux.provider));
    let model = faux.get_model(None).expect("faux model");

    let control_db_path = get_control_db_path(std::path::Path::new(&agent_dir));
    let mut manager = SessionManager::create(&cwd, Some(&session_dir), None).expect("session manager");
    manager
        .set_metadata_control_db_path(
            Some(&control_db_path),
            pi_coding_agent::core::session_manager::SetMetadataControlDbPathOptions::default(),
        )
        .expect("control db");
    manager.persist_for_recovery().expect("persist header");
    let session_file = manager.get_session_file().expect("session file").to_owned();

    let factory: pi_coding_agent::core::extensions::ExtensionFactory = Arc::new(|api| {
        let result = pi_coding_agent::bundled_extensions::session_autoname::session_autoname_extension(api);
        Box::pin(async move { result })
    });
    let settings_manager = SettingsManager::in_memory(Default::default(), SettingsManagerCreateOptions::default());
    let result = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(cwd),
        agent_dir: Some(agent_dir),
        model_runtime: Some(model_runtime),
        model: Some(model),
        thinking_level: Some(ThinkingLevel::Off),
        no_tools: Some(NoTools::All),
        session_manager: Some(manager),
        settings_manager: Some(Arc::new(Mutex::new(settings_manager))),
        extension_factories: vec![InlineExtension::Factory {
            extension_path: Some("<inline:session-autoname>".to_owned()),
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
            control_db_path: Some(control_db_path.clone()),
            ..Default::default()
        })
        .await;
    Env {
        session,
        faux,
        control_db_path,
        session_file,
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

async fn wait_for_name(session: &AgentSession) -> Option<String> {
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

fn persisted_session_name(env: &Env) -> Option<String> {
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

    assert_eq!(wait_for_name(&env.session).await.as_deref(), Some("Real User Session"));
    assert_eq!(faux_call_count(&env.faux), 1);
    assert_eq!(persisted_session_name(&env).as_deref(), Some("Real User Session"));
}

#[tokio::test]
async fn historical_jsonl_session_info_does_not_block_naming_without_metadata_state() {
    let env = env().await;
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&env.session_file)
            .expect("open session file");
        use std::io::Write;
        writeln!(
            file,
            "{}",
            json!({
                "type": "session_info",
                "id": "legacy-session-name",
                "parentId": null,
                "timestamp": "2025-01-01T00:00:00.000Z",
                "name": "Historical JSONL Name",
            })
        )
        .expect("append session info");
    }
    let manager_handle = env.session.session_manager_handle();
    {
        let mut manager = lock_session_manager(&manager_handle);
        manager
            .set_session_file(&env.session_file)
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
        wait_for_name(&env.session).await.as_deref(),
        Some("Metadata Authority Name")
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
    env.session.set_session_name("Manual Session Name").expect("set name");
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
            .as_deref(),
        Some("Manual Session Name")
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
    failure.error_message = Some("invalid_api_key".to_owned());
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
        normalize_title("###\u{feff}Session\u{feff}Title:\u{feff}Keep\u{feff}Words"),
        "Keep Words"
    );
    assert_eq!(normalize_title("Keep\u{85}Words"), "Keep\u{85}Words");
}
