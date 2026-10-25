//! Behavioral tests for the compaction extension boundary in
//! `core/agent-session.ts`: the `session_before_compact` preflight, the
//! `compaction` provider event, and the background-cache `compaction` override.

#[path = "support/oracle.rs"]
mod callbacks_oracle;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::{Arc, Mutex};

use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxModelDefinition, FauxProviderHandle, RegisterFauxProviderOptions,
    faux_assistant_message, faux_provider,
};
use pi_coding_agent::core::agent_session::{AgentSession, AgentSessionEvent};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::compaction::{CompactionResult, CompactionSourceInfo, CompactionSourceType};
use pi_coding_agent::core::extensions::types::HandlerError;
use pi_coding_agent::core::extensions::types::{
    CompactionEvent, CompactionEventResult, CompactionReason, SessionBeforeCompactEvent, SessionBeforeCompactResult,
};
use pi_coding_agent::core::model_registry::ModelRegistry;
use pi_coding_agent::core::resource_loader::InlineExtension;
use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, NoTools, create_agent_session};
use pi_coding_agent::core::session_manager::{SessionEntry, SessionManager, lock_session_manager};
use pi_coding_agent::core::settings_manager::{Settings, SettingsManager, SettingsManagerCreateOptions};
use serde_json::json;

const DISABLED_MESSAGE: &str = "Built-in compaction is disabled; enable compaction or configure a compaction extension";

fn faux_model() -> FauxModelDefinition {
    FauxModelDefinition {
        id: "faux-1".to_owned(),
        name: Some("Faux Model".to_owned()),
        reasoning: Some(true),
        input: None,
        cost: None,
        context_window: Some(100_000.0),
        max_tokens: Some(4096.0),
    }
}

async fn model_registry(faux: &FauxProviderHandle) -> ModelRegistry {
    let mut data = serde_json::Map::new();
    data.insert("faux".to_owned(), json!({"type": "api_key", "key": "faux-key"}));
    let runtime = ModelRegistry::in_memory(AuthStorage::in_memory(data)).unwrap();
    scripted_registry::register_scripted_provider(&runtime, Arc::clone(&faux.provider));
    runtime
}

fn settings(compaction: serde_json::Value) -> Settings {
    let mut map = serde_json::Map::new();
    map.insert("compaction".to_owned(), compaction);
    map.into_iter()
        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
        .collect::<pi_js_compat::js_json::JsObject>()
}

struct Captured {
    before: Vec<SessionBeforeCompactEvent>,
    compaction: Vec<CompactionEvent>,
    background: Vec<&'static str>,
    background_end_gate: Option<Arc<tokio::sync::Notify>>,
}

impl Captured {
    fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            before: Vec::new(),
            compaction: Vec::new(),
            background: Vec::new(),
            background_end_gate: None,
        }))
    }
}

fn extension_result(
    summary: &str,
    preparation: &pi_coding_agent::core::compaction::CompactionPreparation,
) -> CompactionResult {
    CompactionResult {
        summary: JsString::from(summary.to_owned()),
        first_kept_entry_id: preparation.first_kept_entry_id.clone(),
        tokens_before: preparation.tokens_before,
        duration_ms: None,
        estimated_tokens_after: None,
        kept_from_previous_context_tokens: None,
        compacted_result_tokens: None,
        compacted_result_bytes: None,
        source: Some(CompactionSourceInfo {
            r#type: CompactionSourceType::Local,
            provider: "faux".to_owned(),
            model: "faux-1".to_owned(),
            endpoint: None,
        }),
        provider_native: None,
        details: Some(JsValue::from(json!({"origin": "extension"}))),
    }
}

struct Env {
    session: AgentSession,
    faux: FauxProviderHandle,
    captured: Arc<Mutex<Captured>>,
    events: Arc<Mutex<Vec<AgentSessionEvent>>>,
    _dir: tempfile::TempDir,
}

fn seed_compactable_session(manager: &mut SessionManager) {
    manager
        .append_message(pi_agent_core::agent::types::AgentMessage::User(
            pi_ai::types::UserMessage {
                role: pi_ai::types::UserRole::Tag,
                content: pi_ai::types::UserContent::Text(JsString::from("older message to compact")),
                input_source: None,
                provider_native: None,
                timestamp: 0.0,
            },
        ))
        .expect("older user message");
    manager
        .append_message(pi_agent_core::agent::types::AgentMessage::Assistant(Box::new(
            faux_assistant_message("assistant response to compact", FauxAssistantMessageOptions::default()),
        )))
        .expect("older assistant message");
}

fn append_usage_anchor(session: &AgentSession, tokens: f64) -> pi_ai::AssistantMessage {
    let mut anchor = faux_assistant_message("anchor", Default::default());
    anchor.usage.input = tokens;
    anchor.usage.total_tokens = tokens;
    let message = pi_agent_core::agent::types::AgentMessage::Assistant(Box::new(anchor.clone()));
    let identity = session.agent().append_message(message.clone());
    let mut manager = session.session_manager();
    let id = manager.append_message(message).expect("anchor");
    manager.bind_message_identity(&id, identity).expect("anchor identity");
    anchor
}

type BeforeHook = Arc<dyn Fn(&SessionBeforeCompactEvent) -> Option<SessionBeforeCompactResult> + Send + Sync>;
type CompactionHook = Arc<dyn Fn(&CompactionEvent) -> Option<CompactionEventResult> + Send + Sync>;

async fn env_with(
    compaction: serde_json::Value,
    before: Option<BeforeHook>,
    compaction_hook: Option<CompactionHook>,
) -> Env {
    let dir = tempfile::tempdir().expect("temp dir");
    let cwd = dir.path().join("repo").display().to_string();
    std::fs::create_dir_all(&cwd).expect("cwd");
    let agent_dir = dir.path().join("agent").display().to_string();
    std::fs::create_dir_all(&agent_dir).expect("agent dir");
    let session_dir = dir.path().join("sessions").display().to_string();
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![faux_model()]),
        ..Default::default()
    });
    let runtime = model_registry(&faux).await;
    let model = faux.get_model(None).expect("model");
    let mut manager = SessionManager::create(&JsString::from(&cwd), Some(&JsString::from(&session_dir)), None)
        .expect("session manager");
    seed_compactable_session(&mut manager);

    let captured = Captured::new();
    let before_capture = Arc::clone(&captured);
    let compaction_capture = Arc::clone(&captured);
    let background_capture = Arc::clone(&captured);
    let factory = Arc::new(move |api: &pi_coding_agent::core::extensions::ExtensionApi| {
        let before_capture = Arc::clone(&before_capture);
        let compaction_capture = Arc::clone(&compaction_capture);
        let background_start_capture = Arc::clone(&background_capture);
        let background_end_capture = Arc::clone(&background_capture);
        let before = before.clone();
        let compaction_hook = compaction_hook.clone();
        let registered = (|| -> Result<(), String> {
            api.on_session_before_compact(move |event: SessionBeforeCompactEvent, _ctx| {
                let before_capture = Arc::clone(&before_capture);
                let before = before.clone();
                Box::pin(async move {
                    before_capture.lock().expect("captured").before.push(event.clone());
                    Ok(before.as_ref().and_then(|before| before(&event)))
                })
                    as pi_coding_agent::core::extensions::BoxFuture<
                        'static,
                        Result<Option<SessionBeforeCompactResult>, HandlerError>,
                    >
            })?;
            api.on_compaction(move |event: CompactionEvent, _ctx| {
                let compaction_capture = Arc::clone(&compaction_capture);
                let compaction_hook = compaction_hook.clone();
                Box::pin(async move {
                    compaction_capture
                        .lock()
                        .expect("captured")
                        .compaction
                        .push(event.clone());
                    Ok(compaction_hook.as_ref().and_then(|hook| hook(&event)))
                })
                    as pi_coding_agent::core::extensions::BoxFuture<
                        'static,
                        Result<Option<CompactionEventResult>, HandlerError>,
                    >
            })?;
            api.on_background_compaction_start(move |_event, _ctx| {
                let captured = Arc::clone(&background_start_capture);
                Box::pin(async move {
                    captured.lock().unwrap().background.push("background_compaction_start");
                    Ok(None)
                }) as pi_ai::request::BoxFuture<'static, Result<Option<()>, HandlerError>>
            })?;
            api.on_background_compaction_end(move |_event, _ctx| {
                let captured = Arc::clone(&background_end_capture);
                Box::pin(async move {
                    let gate = {
                        let mut captured = captured.lock().unwrap();
                        captured.background.push("background_compaction_end");
                        captured.background_end_gate.clone()
                    };
                    if let Some(gate) = gate {
                        gate.notified().await;
                    }
                    Ok(None)
                }) as pi_ai::request::BoxFuture<'static, Result<Option<()>, HandlerError>>
            })?;
            Ok(())
        })();
        Box::pin(async move { registered.map_err(HandlerError::new) })
            as pi_coding_agent::core::extensions::BoxFuture<'static, Result<(), HandlerError>>
    });

    let settings_manager = SettingsManager::in_memory(settings(compaction), SettingsManagerCreateOptions::default());
    let result = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(JsString::from(cwd.clone())),
        agent_dir: Some(JsString::from(agent_dir)),
        model_registry: Some(runtime),
        model: Some(model),
        thinking_level: Some(ThinkingLevel::Off),
        no_tools: Some(NoTools::All),
        session_manager: Some(manager),
        settings_manager: Some(Arc::new(Mutex::new(settings_manager))),
        extension_factories: vec![InlineExtension::Factory {
            extension_path: Some(JsString::from("<inline:compaction-test>")),
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

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    result.session.subscribe(Arc::new(move |event: &AgentSessionEvent| {
        sink.lock().expect("events").push(event.clone());
    }));
    Env {
        session: result.session,
        faux,
        captured,
        events,
        _dir: dir,
    }
}

fn compaction_entry(session: &AgentSession) -> SessionEntry {
    lock_session_manager(&session.session_manager_handle())
        .get_entries()
        .into_iter()
        .find(|entry| entry.entry_type() == "compaction")
        .expect("compaction entry")
}

#[tokio::test]
async fn extension_compact_callback_histories_match_actual_source_boundaries() {
    use pi_js_compat::js_json as lossless_json;
    let mut oracle = callbacks_oracle::TsOracle::spawn(
        "extension-compact-callbacks-oracle.ts",
        "packages/coding-agent/src/core/agent-session.ts",
    );
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    for mode in ["success", "completeThrow", "errorThrow"] {
        let response=oracle.request(&lossless_json!({"id":1,"mode":mode,"completeError":JsString::from_utf16(vec![0xd800]),"handlerError":JsString::from_utf16(vec![0xdfff])}));
        assert_eq!(response["ok"], lossless_json!(true), "{response:?}");
        expected.push(response["output"].clone());
        if mode == "errorThrow" {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "compact_callback_escape_probe", "--ignored", "--nocapture"])
                .env("PI_COMPACT_CALLBACK_ESCAPE", "1")
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let text = String::from_utf8(output.stdout).unwrap();
            let record = text
                .lines()
                .find_map(|line| line.strip_prefix("COMPACT_EVENTS="))
                .expect("child callback events");
            actual.push(pi_js_compat::js_json::parse_lossless(record).unwrap());
        } else {
            actual.push(run_compact_callback_history(mode).await);
        }
    }
    assert_eq!(lossless_json!(actual), lossless_json!(expected));
}

#[tokio::test]
#[ignore = "explicit private child probe for the uncaught callback boundary"]
async fn compact_callback_escape_probe() {
    assert!(std::env::var_os("PI_COMPACT_CALLBACK_ESCAPE").is_some());
    let events = run_compact_callback_history("errorThrow").await;
    println!("COMPACT_EVENTS={}", pi_js_compat::js_json::stringify_lossless(&events));
}

fn callback_error_snapshot(error: &pi_js_compat::callback_error::CallbackError) -> JsValue {
    use pi_js_compat::abort::AbortReason;
    let reason = error.thrown_value();
    let kind = if matches!(reason, AbortReason::Error(_) | AbortReason::DomException(_)) {
        "error"
    } else {
        "value"
    };
    pi_js_compat::js_json!({"kind":kind,"value":reason.value()})
}

async fn run_compact_callback_history(mode: &str) -> JsValue {
    use pi_coding_agent::core::extensions::types::CompactOptions;
    use pi_js_compat::{
        callback_error::CallbackError,
        js_json as lossless_json,
        node_error::{NativeErrorClass, NativeNodeError},
    };
    let mode = mode.to_owned();
    let events = Arc::new(Mutex::new(Vec::<JsValue>::new()));
    if mode == "errorThrow" {
        let captured = events.clone();
        pi_coding_agent::utils::uncaught::install_uncaught_handler();
        pi_coding_agent::utils::uncaught::set_process_crash_handler(Some(Arc::new(move |payload| {
            let error = payload
                .downcast_ref::<CallbackError>()
                .expect("original escaped callback");
            captured
                .lock()
                .unwrap()
                .push(lossless_json!(["unhandled", callback_error_snapshot(error)]));
            println!(
                "COMPACT_EVENTS={}",
                pi_js_compat::js_json::stringify_lossless(&lossless_json!(captured.lock().unwrap().clone()))
            );
            std::process::exit(0);
        })));
    }
    let producer = events.clone();
    let env = env_with(
        json!({"keepRecentTokens":1}),
        None,
        Some(Arc::new(move |event| {
            producer
                .lock()
                .unwrap()
                .push(lossless_json!(["produce", event.custom_instructions]));
            Some(CompactionEventResult {
                cancel: None,
                compaction: Some(extension_result("callback summary", &event.preparation)),
            })
        })),
    )
    .await;
    let completed = events.clone();
    let failed = events.clone();
    let complete_mode = mode.clone();
    let complete_error = CallbackError::from(
        NativeNodeError::from_constructor(NativeErrorClass::TypeError, JsString::from_utf16(vec![0xd800]))
            .with_property("code", lossless_json!("E_COMPLETE")),
    );
    let handler_error = CallbackError::from(
        NativeNodeError::from_constructor(NativeErrorClass::RangeError, JsString::from_utf16(vec![0xdfff]))
            .with_property("code", lossless_json!("E_HANDLER")),
    );
    env.session
        .extension_runner()
        .create_context()
        .compact(Some(CompactOptions {
            custom_instructions: Some("callback instructions".into()),
            on_complete: Some(Arc::new(move |result| {
                completed
                    .lock()
                    .unwrap()
                    .push(lossless_json!(["complete", result.summary]));
                if complete_mode != "success" {
                    return Err(complete_error.clone());
                }
                Ok(())
            })),
            on_error: Some(Arc::new(move |error| {
                failed
                    .lock()
                    .unwrap()
                    .push(lossless_json!(["error", callback_error_snapshot(error)]));
                if mode == "errorThrow" {
                    return Err(handler_error.clone());
                }
                Ok(())
            })),
        }))
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if env
                .events
                .lock()
                .unwrap()
                .iter()
                .any(|event| matches!(event, AgentSessionEvent::CompactionEnd(_)))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("real compaction boundary");
    let captured = events.lock().unwrap().clone();
    lossless_json!(captured)
}

#[tokio::test]
async fn manual_compaction_uses_the_compaction_hook_and_marks_from_hook() {
    let env = env_with(
        json!({"keepRecentTokens": 1}),
        None,
        Some(Arc::new(|event: &CompactionEvent| {
            Some(CompactionEventResult {
                cancel: None,
                compaction: Some(extension_result("summary from extension", &event.preparation)),
            })
        })),
    )
    .await;

    let result = env.session.compact(None).await.expect("compact");
    assert_eq!(result.summary, "summary from extension");
    assert_eq!(
        result.source,
        Some(CompactionSourceInfo {
            r#type: CompactionSourceType::Local,
            provider: "faux".to_owned(),
            model: "faux-1".to_owned(),
            endpoint: None,
        })
    );
    assert_eq!(
        result.details,
        (Some(json!({"origin": "extension"}))).map(JsValue::from)
    );

    let entry = compaction_entry(&env.session);
    let SessionEntry::Known(pi_coding_agent::core::session_manager::KnownSessionEntry::Compaction(entry)) = entry
    else {
        panic!("expected a compaction entry");
    };
    assert_eq!(entry.summary, "summary from extension");
    assert_eq!(entry.from_hook, Some(true));
    assert_eq!(
        entry.details.as_deref(),
        Some(&JsValue::from(json!({"origin": "extension"})))
    );

    let captured = env.captured.lock().expect("captured");
    assert_eq!(captured.before.len(), 1);
    assert_eq!(captured.before[0].reason, CompactionReason::Manual);
    assert!(!captured.before[0].will_retry);
    assert_eq!(captured.compaction.len(), 1);
    assert_eq!(captured.compaction[0].reason, CompactionReason::Manual);
    assert!(!captured.compaction[0].branch_entries.is_empty());
}

#[tokio::test]
async fn manual_compaction_preflight_only_cancels_and_forwards_custom_instructions() {
    let env = env_with(
        json!({"enabled": false, "keepRecentTokens": 1}),
        Some(Arc::new(|event: &SessionBeforeCompactEvent| {
            assert_eq!(
                event.custom_instructions.as_ref(),
                Some(&JsString::from("keep decisions"))
            );
            Some(
                serde_json::from_value(json!({
                    "compaction": extension_result("obsolete preflight summary", &event.preparation)
                }))
                .expect("preflight result"),
            )
        })),
        None,
    )
    .await;

    for index in 0..4 {
        env.session
            .session_manager()
            .append_message(pi_agent_core::agent::types::AgentMessage::User(pi_ai::UserMessage {
                role: pi_ai::UserRole::Tag,
                content: pi_ai::UserContent::Text(format!("newer request {index}").into()),
                input_source: None,
                provider_native: None,
                timestamp: f64::from(index + 1),
            }))
            .expect("newer user");
    }
    env.session
        .session_manager()
        .append_message(pi_agent_core::agent::types::AgentMessage::Assistant(Box::new(
            faux_assistant_message("latest answer", Default::default()),
        )))
        .expect("latest answer");
    let mut oracle =
        callbacks_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({
        "operation": "preflight-compaction", "dir": env._dir.path().join("oracle").display().to_string()
    }));
    assert_eq!(expected, json!({"ok": true, "value": {"error": DISABLED_MESSAGE}}));
    let instructions = JsString::from("keep decisions");
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        env.session.compact(Some(&instructions)),
    )
    .await
    .expect("compaction must settle without a continuation");
    let actual = match result {
        Ok(result) => json!({"summary": result.summary}),
        Err(error) => json!({"error": error.message()}),
    };
    assert_eq!(actual, expected["value"]);
    assert!(
        !env.session
            .session_manager()
            .get_entries()
            .iter()
            .any(|entry| entry.entry_type() == "compaction")
    );
}

#[tokio::test]
async fn resumed_manual_compaction_releases_turn_start_for_streaming_input() {
    use pi_coding_agent::core::agent_session::{PromptOptions, StreamingBehavior};
    let env = env_with(
        json!({"keepRecentTokens": 1}),
        None,
        Some(Arc::new(|event| {
            Some(CompactionEventResult {
                cancel: None,
                compaction: Some(extension_result("saved summary", &event.preparation)),
            })
        })),
    )
    .await;
    env.session.set_auto_retry_enabled(false);
    env.session
        .session_manager()
        .append_message(pi_agent_core::agent::types::AgentMessage::User(pi_ai::UserMessage {
            role: pi_ai::UserRole::Tag,
            content: pi_ai::UserContent::Text("unfinished request".into()),
            input_source: None,
            provider_native: None,
            timestamp: 1.0,
        }))
        .expect("unfinished request");
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (start_provider, release_provider) = (Arc::clone(&started), Arc::clone(&release));
    env.faux
        .set_responses(vec![pi_ai::providers::faux::FauxResponseStep::Factory(Arc::new(
            move |_, _, _, _| {
                let (started, release) = (Arc::clone(&start_provider), Arc::clone(&release_provider));
                Box::pin(async move {
                    started.notify_one();
                    release.notified().await;
                    Ok(faux_assistant_message("resumed answer", Default::default()))
                })
            },
        ))]);
    let session = env.session.clone();
    let compact = tokio::spawn(async move { session.compact(None).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), started.notified())
        .await
        .expect("resumed provider started");
    let input = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        env.session.prompt(
            "new input",
            PromptOptions {
                streaming_behavior: Some(StreamingBehavior::FollowUp),
                ..Default::default()
            },
        ),
    )
    .await;
    release.notify_one();
    env.session.abort().await.expect("abort resumed turn");
    tokio::time::timeout(std::time::Duration::from_secs(5), compact)
        .await
        .expect("compact settled")
        .expect("compact task")
        .expect("saved compaction");
    let mut oracle =
        callbacks_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({
        "operation": "compaction-lock", "dir": env._dir.path().join("oracle").display().to_string()
    }));
    assert_eq!(expected, json!({"ok": true, "value": true}));
    assert!(
        matches!(input, Ok(Ok(()))),
        "streaming prompt must be accepted while compact awaits resumed turn: {input:?}"
    );
}

#[tokio::test]
async fn overflow_recovery_resets_when_the_next_user_request_starts_like_frozen() {
    use pi_coding_agent::core::agent_session::PromptOptions;
    let env = env_with(
        json!({"enabled": false, "keepRecentTokens": 1}),
        None,
        Some(Arc::new(|event| {
            Some(CompactionEventResult {
                cancel: None,
                compaction: Some(extension_result("saved summary", &event.preparation)),
            })
        })),
    )
    .await;
    env.session.set_auto_retry_enabled(false);
    for index in 0..4 {
        env.session
            .session_manager()
            .append_message(pi_agent_core::agent::types::AgentMessage::User(pi_ai::UserMessage {
                role: pi_ai::UserRole::Tag,
                content: pi_ai::UserContent::Text(format!("newer request {index}").into()),
                input_source: None,
                provider_native: None,
                timestamp: f64::from(index + 1),
            }))
            .expect("newer user");
    }
    env.faux.set_responses(
        [
            "maximum context length exceeded",
            "fatal",
            "maximum context length exceeded",
            "fatal",
        ]
        .into_iter()
        .map(|message| {
            faux_assistant_message(
                "",
                FauxAssistantMessageOptions {
                    stop_reason: Some(pi_ai::StopReason::Error),
                    error_message: Some(message.into()),
                    timestamp: Some(0.0),
                    ..Default::default()
                },
            )
            .into()
        })
        .collect(),
    );
    env.session
        .prompt("first overflow request", PromptOptions::default())
        .await
        .expect("first request");
    env.session
        .prompt("second overflow request", PromptOptions::default())
        .await
        .expect("second request");
    let compactions = env.captured.lock().unwrap().compaction.len();
    let mut oracle =
        callbacks_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({
        "operation": "repeat-overflow", "dir": env._dir.path().join("oracle").display().to_string()
    }));
    assert_eq!(expected, json!({"ok": true, "value": {"compactions": 2}}));
    assert_eq!(json!({"compactions": compactions}), expected["value"]);
}

#[tokio::test]
async fn percentage_threshold_counts_messages_after_the_usage_anchor_like_frozen() {
    use pi_coding_agent::core::agent_session::PromptOptions;
    let env = env_with(
        json!({"enabled": false, "thresholdPercent": 50, "reserveTokens": 0, "keepRecentTokens": 1}),
        None,
        Some(Arc::new(|event| {
            Some(CompactionEventResult {
                cancel: None,
                compaction: Some(extension_result("saved summary", &event.preparation)),
            })
        })),
    )
    .await;
    env.session.set_auto_retry_enabled(false);
    let mut model = env.session.model();
    model.context_window = 1000.0;
    model.auto_compaction_threshold = Some(900.0);
    env.session.agent().set_model(model);
    let mut anchor = faux_assistant_message("anchor", Default::default());
    anchor.usage.input = 400.0;
    anchor.usage.total_tokens = 400.0;
    let message = pi_agent_core::agent::types::AgentMessage::Assistant(Box::new(anchor));
    let identity = env.session.agent().append_message(message.clone());
    {
        let mut manager = env.session.session_manager();
        let entry_id = manager.append_message(message).expect("anchor");
        manager
            .bind_message_identity(&entry_id, identity)
            .expect("anchor identity");
    }
    env.session
        .send_custom_message(
            &"tail".into(),
            pi_agent_core::harness::messages::CustomMessageContent::Text("x".repeat(800).into()),
            false,
            None,
            false,
            None,
        )
        .await
        .expect("trailing custom");
    env.faux.set_responses(vec![
        faux_assistant_message(
            "",
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::StopReason::Error),
                error_message: Some("fatal".into()),
                ..Default::default()
            },
        )
        .into(),
    ]);
    let before_run = Arc::new(std::sync::atomic::AtomicUsize::new(usize::MAX));
    let recorded = Arc::clone(&before_run);
    let captured = Arc::clone(&env.captured);
    env.session
        .prompt(
            "next request",
            PromptOptions {
                preflight_result: Some(Arc::new(move |accepted| {
                    if accepted {
                        recorded.store(
                            captured.lock().unwrap().compaction.len(),
                            std::sync::atomic::Ordering::SeqCst,
                        );
                    }
                })),
                ..Default::default()
            },
        )
        .await
        .expect("prompt");
    let compactions = env.captured.lock().unwrap().compaction.len();
    let mut oracle =
        callbacks_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({
        "operation": "threshold-trailing", "dir": env._dir.path().join("oracle").display().to_string()
    }));
    assert_eq!(
        expected,
        json!({"ok": true, "value": {"compactions": 1, "beforeRun": 1}})
    );
    assert_eq!(
        json!({"compactions": compactions, "beforeRun": before_run.load(std::sync::atomic::Ordering::SeqCst)}),
        expected["value"]
    );
}

#[tokio::test]
async fn next_turn_refresh_compacts_percentage_context_before_returning_snapshot_like_frozen() {
    use pi_agent_core::agent::types::{AgentContext, AgentMessage, PrepareNextTurnContext};
    let env = env_with(
        json!({"enabled": false, "thresholdPercent": 50, "reserveTokens": 0, "keepRecentTokens": 1}),
        None,
        Some(Arc::new(|event| {
            Some(CompactionEventResult {
                cancel: None,
                compaction: Some(extension_result("saved summary", &event.preparation)),
            })
        })),
    )
    .await;
    let mut model = env.session.model();
    model.context_window = 1000.0;
    model.auto_compaction_threshold = Some(900.0);
    env.session.agent().set_model(model);
    let mut anchor = faux_assistant_message("anchor", Default::default());
    anchor.usage.input = 600.0;
    anchor.usage.total_tokens = 600.0;
    let message = AgentMessage::Assistant(Box::new(anchor.clone()));
    let identity = env.session.agent().append_message(message.clone());
    {
        let mut manager = env.session.session_manager();
        let id = manager.append_message(message).expect("anchor");
        manager.bind_message_identity(&id, identity).expect("anchor identity");
    }
    env.session
        .send_custom_message(
            &"tail".into(),
            pi_agent_core::harness::messages::CustomMessageContent::Text("x".repeat(800).into()),
            false,
            None,
            false,
            None,
        )
        .await
        .expect("trailing custom");
    let hook = env
        .session
        .agent()
        .prepare_next_turn_with_context_hook()
        .expect("installed hook");
    let state = env.session.state();
    let update = hook(
        PrepareNextTurnContext {
            message: anchor,
            tool_results: Vec::new(),
            new_messages: Vec::new(),
            context: AgentContext {
                system_prompt: state.system_prompt,
                messages: state.messages,
                tools: Some(state.tools),
            },
        },
        None,
    )
    .await
    .expect("hook result")
    .expect("snapshot");
    let actual = json!({
        "compactions": env.captured.lock().unwrap().compaction.len(),
        "hasSummary": update.context.expect("context").messages.iter().any(|message| message.has_role("compactionSummary")),
    });
    let mut oracle =
        callbacks_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(
        &json!({"operation": "next-turn-threshold", "dir": env._dir.path().join("oracle").display().to_string()}),
    );
    assert_eq!(
        expected,
        json!({"ok": true, "value": {"compactions": 1, "hasSummary": true}})
    );
    assert_eq!(actual, expected["value"]);
}

#[test]
fn background_compaction_event_wire_tags_match_actual_frozen_emission() {
    use pi_coding_agent::core::extensions::types::ExtensionEvent;
    let dir = tempfile::tempdir().expect("fixture dir");
    let mut oracle =
        callbacks_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(&json!({"operation": "background-events", "dir": dir.path().display().to_string()}));
    assert_eq!(
        expected,
        json!({"ok": true, "value": ["background_compaction_start", "compaction", "background_compaction_end"]})
    );
    for name in ["background_compaction_start", "background_compaction_end"] {
        let wire = json!({"type": name});
        let event: ExtensionEvent = serde_json::from_value(wire.clone()).expect("frozen event must be representable");
        assert_eq!(event.event_type(), name);
        assert_eq!(serde_json::to_value(event).expect("event serialization"), wire);
    }
}

#[tokio::test]
async fn background_compaction_handler_registration_and_dispatch_preserve_order() {
    let env = env_with(json!({"enabled": false}), None, None).await;
    let runner = env.session.extension_runner();
    for name in ["background_compaction_start", "background_compaction_end"] {
        assert!(runner.has_handlers(name));
        let mut event = serde_json::from_value(json!({"type": name})).expect("background event");
        runner.emit(&mut event).await;
    }
    assert_eq!(
        env.captured.lock().unwrap().background,
        ["background_compaction_start", "background_compaction_end"]
    );
}

#[tokio::test]
async fn manual_compaction_preflight_cancel_reports_cancelled() {
    let env = env_with(
        json!({"keepRecentTokens": 1}),
        Some(Arc::new(|_: &SessionBeforeCompactEvent| {
            Some(SessionBeforeCompactResult { cancel: Some(true) })
        })),
        None,
    )
    .await;

    let error = env.session.compact(None).await.expect_err("cancelled");
    assert_eq!(error.message(), "Compaction cancelled");
    let events = env.events.lock().expect("events");
    assert!(events.iter().any(|event| matches!(
        event,
        AgentSessionEvent::CompactionEnd(end) if end.aborted && end.result.is_none() && end.error_message.is_none()
    )));
    assert!(
        !lock_session_manager(&env.session.session_manager_handle())
            .get_entries()
            .iter()
            .any(|entry| entry.entry_type() == "compaction")
    );
}

#[tokio::test]
async fn disabled_builtin_compaction_uses_an_extension_result() {
    let env = env_with(
        json!({"enabled": false, "keepRecentTokens": 1}),
        None,
        Some(Arc::new(|event: &CompactionEvent| {
            Some(CompactionEventResult {
                cancel: None,
                compaction: Some(extension_result(
                    "disabled local compaction summary",
                    &event.preparation,
                )),
            })
        })),
    )
    .await;

    let result = env.session.compact(None).await.expect("compact");
    assert_eq!(result.summary, "disabled local compaction summary");
}

#[tokio::test]
async fn disabled_builtin_compaction_without_extension_result_fails_explicitly() {
    let env = env_with(json!({"enabled": false, "keepRecentTokens": 1}), None, None).await;

    let error = env.session.compact(None).await.expect_err("disabled");
    assert_eq!(error.message(), DISABLED_MESSAGE);
}

#[tokio::test]
async fn local_compaction_marks_the_entry_as_not_from_a_hook() {
    let env = env_with(json!({"keepRecentTokens": 1}), None, None).await;
    env.faux.set_responses(vec![
        faux_assistant_message("local summary", FauxAssistantMessageOptions::default()).into(),
        faux_assistant_message("local summary", FauxAssistantMessageOptions::default()).into(),
    ]);

    let result = env.session.compact(None).await.expect("compact");
    assert!(
        result.summary.ends_with("local summary"),
        "unexpected summary: {:?}",
        result.summary
    );
    let SessionEntry::Known(pi_coding_agent::core::session_manager::KnownSessionEntry::Compaction(entry)) =
        compaction_entry(&env.session)
    else {
        panic!("expected a compaction entry");
    };
    assert_eq!(entry.from_hook, Some(false));
}

#[tokio::test]
async fn auto_compaction_runs_extension_handlers_when_builtin_is_disabled() {
    let env = env_with(
        json!({"enabled": false, "keepRecentTokens": 1, "reserveTokens": 99_000}),
        None,
        Some(Arc::new(|event: &CompactionEvent| {
            Some(CompactionEventResult {
                cancel: None,
                compaction: Some(extension_result("auto extension summary", &event.preparation)),
            })
        })),
    )
    .await;

    env.faux.set_responses(vec![
        faux_assistant_message("done", FauxAssistantMessageOptions::default()).into(),
    ]);

    env.session
        .prompt("go", pi_coding_agent::core::agent_session::PromptOptions::default())
        .await
        .expect("prompt");

    assert_eq!(env.captured.lock().expect("captured").compaction.len(), 1);
    let entry = compaction_entry(&env.session);
    let SessionEntry::Known(pi_coding_agent::core::session_manager::KnownSessionEntry::Compaction(entry)) = entry
    else {
        panic!("expected a compaction entry");
    };
    assert_eq!(entry.summary, "auto extension summary");
    assert_eq!(entry.from_hook, Some(true));
}

#[tokio::test]
async fn auto_compaction_reports_the_disabled_message_when_no_extension_result_exists() {
    let env = env_with(
        json!({"enabled": false, "keepRecentTokens": 1, "reserveTokens": 99_000}),
        None,
        None,
    )
    .await;

    env.faux.set_responses(vec![
        faux_assistant_message("done", FauxAssistantMessageOptions::default()).into(),
    ]);

    env.session
        .prompt("go", pi_coding_agent::core::agent_session::PromptOptions::default())
        .await
        .expect("prompt");

    let events = env.events.lock().expect("events");
    let end = events
        .iter()
        .rev()
        .find_map(|event| match event {
            AgentSessionEvent::CompactionEnd(end) if end.reason == "threshold" => Some(end),
            _ => None,
        })
        .expect("compaction_end");
    let expected = format!("Auto-compaction failed: {DISABLED_MESSAGE}");
    assert_eq!(
        end.error_message.as_ref(),
        Some(expected.as_str()).map(JsString::from).as_ref()
    );
}

#[tokio::test]
async fn background_compaction_cache_uses_the_compaction_hook() {
    let env = env_with(
        json!({"keepRecentTokens": 1, "reserveTokens": 0}),
        None,
        Some(Arc::new(|event: &CompactionEvent| {
            assert_eq!(event.reason, CompactionReason::Threshold);
            Some(CompactionEventResult {
                cancel: None,
                compaction: Some(extension_result("background extension summary", &event.preparation)),
            })
        })),
    )
    .await;

    env.faux.set_responses(vec![
        faux_assistant_message(
            "",
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::StopReason::Error),
                error_message: Some("fatal".into()),
                ..Default::default()
            },
        )
        .into(),
    ]);
    append_usage_anchor(&env.session, 80000.0);
    env.session
        .prompt("first", pi_coding_agent::core::agent_session::PromptOptions::default())
        .await
        .expect("first prompt");

    for _ in 0..100 {
        if env.captured.lock().expect("captured").background.last() == Some(&"background_compaction_end") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(env.captured.lock().expect("captured").compaction.len(), 1);

    env.faux.set_responses(vec![
        faux_assistant_message(
            "",
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::StopReason::Error),
                error_message: Some("fatal".into()),
                ..Default::default()
            },
        )
        .into(),
    ]);
    env.session
        .prompt("second", pi_coding_agent::core::agent_session::PromptOptions::default())
        .await
        .expect("second prompt");

    let captured = env.captured.lock().expect("captured");
    assert_eq!(captured.compaction.len(), 1, "the cache must not regenerate");
    assert_eq!(
        captured.before.len(),
        1,
        "ready-cache installation runs foreground preflight once"
    );
    assert_eq!(
        captured.background,
        ["background_compaction_start", "background_compaction_end"]
    );
    let SessionEntry::Known(pi_coding_agent::core::session_manager::KnownSessionEntry::Compaction(entry)) =
        compaction_entry(&env.session)
    else {
        panic!("expected a compaction entry");
    };
    assert_eq!(entry.summary, "background extension summary");
    assert_eq!(entry.from_hook, Some(true));
}

#[tokio::test]
async fn cancelled_background_compaction_still_emits_the_final_end_event() {
    let env = env_with(
        json!({"enabled": false, "reserveTokens": 0}),
        None,
        Some(Arc::new(|_| {
            Some(CompactionEventResult {
                cancel: Some(true),
                compaction: None,
            })
        })),
    )
    .await;
    append_usage_anchor(&env.session, 80000.0);
    env.faux.set_responses(vec![
        faux_assistant_message(
            "",
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::StopReason::Error),
                error_message: Some("fatal".into()),
                ..Default::default()
            },
        )
        .into(),
    ]);
    env.session.prompt("first", Default::default()).await.expect("prompt");
    for _ in 0..100 {
        if env.captured.lock().unwrap().background.last() == Some(&"background_compaction_end") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let captured = env.captured.lock().unwrap();
    assert_eq!(captured.compaction.len(), 1, "one background compaction handler");
    assert!(
        captured.before.is_empty(),
        "background generation must skip foreground preflight"
    );
    assert_eq!(
        captured.background,
        ["background_compaction_start", "background_compaction_end"],
        "preflight count={}, reasons={:?}",
        captured.before.len(),
        captured.compaction.iter().map(|event| event.reason).collect::<Vec<_>>()
    );
    assert!(
        !env.session
            .session_manager()
            .get_entries()
            .iter()
            .any(|entry| entry.entry_type() == "compaction")
    );
}

#[tokio::test]
async fn disabled_background_generation_without_extension_result_never_calls_provider_like_frozen() {
    let env = env_with(json!({"enabled": false, "reserveTokens": 0}), None, None).await;
    append_usage_anchor(&env.session, 80000.0);
    env.faux.set_responses(vec![
        faux_assistant_message(
            "",
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::StopReason::Error),
                error_message: Some("fatal".into()),
                ..Default::default()
            },
        )
        .into(),
    ]);
    env.session.prompt("first", Default::default()).await.expect("prompt");
    for _ in 0..100 {
        if env.captured.lock().unwrap().background.last() == Some(&"background_compaction_end") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let actual = json!({"events": env.captured.lock().unwrap().background,
        "providerCalls": env.faux.state().lock().unwrap().call_count.checked_sub(1).expect("foreground provider call")});
    let mut oracle =
        callbacks_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(
        &json!({"operation": "background-disabled", "dir": env._dir.path().join("oracle").display().to_string()}),
    );
    assert_eq!(
        expected,
        json!({"ok": true, "value": {"events": ["background_compaction_start", "background_compaction_end"], "providerCalls": 0}})
    );
    assert_eq!(actual, expected["value"]);
}

#[tokio::test]
async fn cancelling_background_compaction_waits_for_final_handler_settlement_like_frozen() {
    let env = env_with(
        json!({"enabled": false, "reserveTokens": 0}),
        None,
        Some(Arc::new(|_| {
            Some(CompactionEventResult {
                cancel: Some(true),
                compaction: None,
            })
        })),
    )
    .await;
    let gate = Arc::new(tokio::sync::Notify::new());
    env.captured.lock().unwrap().background_end_gate = Some(Arc::clone(&gate));
    append_usage_anchor(&env.session, 80000.0);
    env.faux.set_responses(vec![
        faux_assistant_message(
            "",
            FauxAssistantMessageOptions {
                stop_reason: Some(pi_ai::StopReason::Error),
                error_message: Some("fatal".into()),
                ..Default::default()
            },
        )
        .into(),
    ]);
    env.session.prompt("first", Default::default()).await.expect("prompt");
    for _ in 0..100 {
        if env.captured.lock().unwrap().background.last() == Some(&"background_compaction_end") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        env.captured.lock().unwrap().background.last(),
        Some(&"background_compaction_end")
    );
    let session = env.session.clone();
    let mut compact = tokio::spawn(async move { session.compact(None).await });
    let early = tokio::time::timeout(std::time::Duration::from_millis(100), &mut compact).await;
    let completed_before_end = early.is_ok();
    gate.notify_one();
    let settled = match early {
        Ok(result) => result,
        Err(_) => tokio::time::timeout(std::time::Duration::from_secs(5), compact)
            .await
            .expect("compaction settled after background release"),
    }
    .expect("manual compaction task")
    .expect_err("extension cancels manual compaction");
    assert_eq!(settled.message(), "Compaction cancelled");
    let mut oracle =
        callbacks_oracle::TsOracle::spawn("agent-session-oracle.ts", "packages/coding-agent/src/core/sdk.ts");
    let expected = oracle.request(
        &json!({"operation": "background-cancel-wait", "dir": env._dir.path().join("oracle").display().to_string()}),
    );
    assert_eq!(expected, json!({"ok": true, "value": {"completedBeforeEnd": false}}));
    assert_eq!(json!({"completedBeforeEnd": completed_before_end}), expected["value"]);
}

#[path = "support/scripted_registry.rs"]
mod scripted_registry;
