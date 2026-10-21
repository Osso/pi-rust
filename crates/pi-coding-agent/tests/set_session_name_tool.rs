//! Whole frozen session-autoname model-tool differential, using real Rust managers.
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use std::sync::{Arc, Mutex};

use pi_coding_agent::bundled_extensions::session_autoname::session_autoname_extension;
use pi_coding_agent::core::extensions::types::{
    ExtensionEvent, HandlerError, SessionStartEvent, SessionStartReason, ToolDefinition,
};
use pi_coding_agent::core::extensions::{ExtensionRunner, create_extension_runtime};
use pi_coding_agent::core::session_manager::{
    NewSessionOptions, SessionManager, SetMetadataControlDbPathOptions, SharedSessionManager, lock_session_manager,
};
use serde_json::{Value, json};

const SOURCE: &str = "packages/coding-agent/extensions/session-autoname/src/index.ts";

struct Fixture {
    runner: ExtensionRunner,
    tool: ToolDefinition,
    manager: SharedSessionManager,
    events: Arc<Mutex<Vec<Value>>>,
    setter_error: Arc<Mutex<Option<String>>>,
}

impl Fixture {
    async fn new(manager: SharedSessionManager, agent_id: Option<String>) -> Self {
        let runtime = create_extension_runtime();
        let extension = support::extension(&runtime, "/fixture", "<first-party:session-autoname>", |pi| {
            session_autoname_extension(pi).expect("register extension");
        })
        .await;
        let tool = extension
            .read()
            .tools
            .get("set_session_name")
            .expect("frozen extension registers set_session_name, not an agent-end title request")
            .definition
            .clone();
        let mut runner = ExtensionRunner::new(
            vec![extension],
            runtime,
            "/fixture".to_owned(),
            Arc::clone(&manager),
            support::model_registry().await,
            None,
        );
        let events = Arc::new(Mutex::new(Vec::new()));
        let setter_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let mut actions = support::actions();
        let target = Arc::clone(&manager);
        let recorded = Arc::clone(&events);
        let failure = Arc::clone(&setter_error);
        actions.set_session_name = Arc::new(move |name| {
            if let Some(error) = failure.lock().unwrap().as_ref() {
                return Err(HandlerError::new(error.clone()));
            }
            lock_session_manager(&target)
                .set_session_name(Some(name))
                .map_err(|error| HandlerError::new(error.to_string()))?;
            recorded
                .lock()
                .unwrap()
                .push(json!({"type":"session_info_changed", "name":name}));
            Ok(())
        });
        let mut context_actions = support::context_actions();
        context_actions.get_multi_agent_agent_id = Some(Arc::new(move || agent_id.clone()));
        runner.bind_core(actions, context_actions, None);
        Self {
            runner,
            tool,
            manager,
            events,
            setter_error,
        }
    }

    async fn execute(&self, name: &str) -> Result<Value, String> {
        (self.tool.execute)(
            "fixture-call".to_owned(),
            json!({"name":name}),
            None,
            None,
            self.runner.create_context(),
        )
        .await
        .map(|result| serde_json::to_value(result).unwrap())
        .map_err(|error| error.to_string())
    }

    fn snapshot(&self, result: Option<Value>, error: Option<String>) -> Value {
        let manager = lock_session_manager(&self.manager);
        json!({"result":result, "error":error, "name":manager.get_session_name(),
            "stored":manager.has_session_name_state(), "events":*self.events.lock().unwrap()})
    }
}

fn manager(directory: &tempfile::TempDir, case: &Value) -> SharedSessionManager {
    let cwd = directory.path().to_str().unwrap();
    let options = Some(NewSessionOptions {
        is_subagent: case["subagent"].as_bool().unwrap_or(false),
        ..Default::default()
    });
    let mut manager = if case["ephemeral"] == true {
        SessionManager::in_memory(Some(cwd), options).unwrap()
    } else {
        SessionManager::create(cwd, Some(cwd), options).unwrap()
    };
    if let Some(name) = case.get("initialName") {
        manager.set_session_name(name.as_str()).unwrap();
    }
    Arc::new(Mutex::new(manager))
}

async fn run_case(case: &Value) -> Value {
    let directory = tempfile::tempdir().unwrap();
    let manager = manager(&directory, case);
    let agent_id = case["agentId"].as_str().map(str::to_owned);
    let mut fixture = Fixture::new(Arc::clone(&manager), agent_id.clone()).await;
    let metadata = json!({"name":fixture.tool.name, "label":fixture.tool.label,
        "description":fixture.tool.description, "promptGuidelines":fixture.tool.prompt_guidelines,
        "approvalRequired":fixture.tool.approval_required, "parameters":fixture.tool.parameters});
    let mut output = Vec::new();
    for step in case["steps"].as_array().unwrap() {
        let mut result = None;
        let mut error = None;
        match step["op"].as_str().unwrap() {
            "call" => match fixture.execute(step["name"].as_str().unwrap()).await {
                Ok(value) => result = Some(value),
                Err(value) => error = Some(value),
            },
            "newSession" => {
                lock_session_manager(&manager).new_session(None).unwrap();
            }
            "external" => lock_session_manager(&manager)
                .set_session_name(step["name"].as_str())
                .unwrap(),
            "setterError" => *fixture.setter_error.lock().unwrap() = step["error"].as_str().map(str::to_owned),
            "reset" => {
                let events = Arc::clone(&fixture.events);
                fixture = Fixture::new(Arc::clone(&manager), agent_id.clone()).await;
                // A factory reload resets closure ownership, not session/event history.
                fixture.events.lock().unwrap().extend(events.lock().unwrap().clone());
            }
            "session_start" => {
                fixture
                    .runner
                    .emit(&mut ExtensionEvent::SessionStart(SessionStartEvent {
                        reason: SessionStartReason::New,
                        previous_session_file: None,
                    }))
                    .await;
            }
            other => panic!("unknown step {other}"),
        }
        output.push(fixture.snapshot(result, error));
    }
    json!({"metadata":metadata, "output":output})
}

#[tokio::test]
async fn frozen_registered_tool_matches_results_refusals_and_closure_lifetime() {
    let mut ts = oracle::TsOracle::spawn("set-session-name-tool-oracle.ts", SOURCE);
    let cases = vec![
        json!({"steps":[{"op":"call","name":"  Fix\r\n login\t flow  "},{"op":"call","name":"Fix login flow"},{"op":"call","name":"Own new title"}]}),
        json!({"steps":[{"op":"call","name":"\u{feff}\u{2000}\r\n"},{"op":"call","name":"after blank"}]}),
        json!({"initialName":"Someone else", "steps":[{"op":"call","name":"Someone else"}]}),
        json!({"initialName":null, "steps":[{"op":"call","name":"Refused clear"}]}),
        json!({"agentId":"child-1", "steps":[{"op":"call","name":"Child"},{"op":"call","name":" "}]}),
        json!({"agentId":"", "steps":[{"op":"call","name":"Empty agent ID"}]}),
        json!({"subagent":true, "steps":[{"op":"call","name":"Child"}]}),
        json!({"ephemeral":true, "steps":[{"op":"call","name":"Ephemeral"}]}),
        json!({"steps":[{"op":"call","name":"First"},{"op":"external","name":"Another writer"},{"op":"call","name":"Cannot seize"}]}),
        json!({"steps":[{"op":"call","name":"First"},{"op":"external","name":null},{"op":"call","name":"Cannot undo clear"}]}),
        json!({"steps":[{"op":"call","name":"First"},{"op":"session_start"},{"op":"call","name":"Still owns"},{"op":"reset"},{"op":"call","name":"Reload loses ownership"}]}),
        json!({"steps":[{"op":"setterError","error":"fixture write failed"},{"op":"call","name":"Failed"},{"op":"setterError"},{"op":"external","name":"Failed"},{"op":"call","name":"Not owned"}]}),
        json!({"steps":[{"op":"call","name":"## Title: Keep **formatting**\nsecond line\u{0085} retained"}]}),
        json!({"steps":[{"op":"call","name":"Owned"},{"op":"newSession"},{"op":"session_start"},{"op":"external","name":"Owned"},{"op":"call","name":"Equality allows across switch"}]}),
        json!({"steps":[{"op":"call","name":"Owned"},{"op":"newSession"},{"op":"session_start"},{"op":"external","name":"Different"},{"op":"call","name":"Cannot seize new session"}]}),
    ];
    for case in cases {
        let expected = ts.request(&case);
        assert!(expected.get("error").is_none(), "oracle failed: {expected}");
        let actual = run_case(&case).await;
        assert_eq!(actual["metadata"], expected["metadata"], "metadata: {case}");
        let output: Value = serde_json::from_str(expected["outputJson"].as_str().unwrap()).unwrap();
        assert_eq!(actual["output"], output, "scenario: {case}");
        assert_eq!(expected["titleRequests"], 0);
    }
}

#[tokio::test]
async fn utf16_whitespace_and_word_boundaries_match_the_executed_ts_tool() {
    let mut ts = oracle::TsOracle::spawn("set-session-name-tool-oracle.ts", SOURCE);
    let names = vec![
        "x".repeat(80),
        "x".repeat(81),
        format!("{} rest", "x".repeat(79)),
        format!("{} rest", "x".repeat(80)),
        format!("{}y rest", "🙂".repeat(39)),
        format!("{} rest", "🙂".repeat(40)),
        format!("ab {} tail", "🙂".repeat(40)),
        "a\u{0009}b\u{000b}c\u{000c}d\u{00a0}e\u{1680}f\u{200a}g\u{2028}h\u{2029}i\u{202f}j\u{205f}k\u{3000}l\u{feff}m"
            .to_owned(),
        "a\u{0085}b\u{180e}c\u{200b}d".to_owned(),
    ];
    for name in names {
        let case = json!({"steps":[{"op":"call","name":name}]});
        // Preserve literal JS line separators across the oracle's JSONL reader.
        let encoded = json!({"steps":[{"op":"call", "nameUnits":case["steps"][0]["name"].as_str().unwrap().encode_utf16().collect::<Vec<_>>()}]});
        let expected = ts.request(&encoded);
        assert!(expected.get("error").is_none(), "oracle failed for {case}: {expected}");
        let output: Value = serde_json::from_str(expected["outputJson"].as_str().unwrap()).unwrap();
        assert_eq!(run_case(&case).await["output"], output, "{case}");
    }
}

#[tokio::test]
async fn split_surrogate_is_proven_exactly_and_not_silently_converted() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_str().unwrap();
    let mut ts = oracle::TsOracle::spawn("set-session-name-tool-oracle.ts", SOURCE);
    let raw = format!("{}🙂", "x".repeat(79));
    let expected = ts.request(&json!({"op":"toolPersist", "cwd":root, "sessionDir":root,
        "db":directory.path().join("control.sqlite"), "toolName":raw}));
    assert!(expected.get("error").is_none(), "{expected}");
    println!("exact frozen surrogate fixture: {expected}");
    let helper_json = format!("\"{}\\ud83d\"", "x".repeat(79));
    assert_eq!(expected["managerNameJson"], helper_json);
    assert!(
        expected["resultJson"]
            .as_str()
            .unwrap()
            .contains(&format!("\"name\":{helper_json}"))
    );
    assert_eq!(
        expected["storedNameJson"],
        serde_json::to_string(&format!("{}�", "x".repeat(79))).unwrap()
    );
    let fixture = Fixture::new(manager(&directory, &json!({})), None).await;
    let error = fixture.execute(&raw).await.unwrap_err();
    assert_eq!(
        error,
        "set_session_name UTF-16 result is not representable by the current extension API; parity pending approval"
    );
    assert!(!lock_session_manager(&fixture.manager).has_session_name_state());
    assert!(fixture.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn public_ts_manager_and_rust_tool_share_metadata_in_both_directions() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_str().unwrap();
    let db = directory.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("set-session-name-tool-oracle.ts", SOURCE);
    let created = ts.request(&json!({"op":"create", "cwd":root, "sessionDir":root, "db":db}));
    assert!(created.get("error").is_none(), "{created}");
    let session_path = created["path"].as_str().unwrap();
    let mut opened = SessionManager::open(session_path, Some(root), None).unwrap();
    opened
        .set_metadata_control_db_path(Some(&db), SetMetadataControlDbPathOptions::default())
        .unwrap();
    let fixture = Fixture::new(Arc::new(Mutex::new(opened)), None).await;
    fixture.execute("TS session Rust tool").await.unwrap();
    let read = ts.request(&json!({"op":"read", "path":session_path, "sessionDir":root, "db":db}));
    assert_eq!(read["name"], "TS session Rust tool", "{read}");
    assert_eq!(
        read["state"],
        json!({"name":"TS session Rust tool","hasStoredValue":true})
    );

    let created =
        ts.request(&json!({"op":"create", "cwd":root, "sessionDir":root, "db":db, "name":"TS independent name"}));
    let path = created["path"].as_str().unwrap();
    let mut opened = SessionManager::open(path, Some(root), None).unwrap();
    opened
        .set_metadata_control_db_path(Some(&db), SetMetadataControlDbPathOptions::default())
        .unwrap();
    let fixture = Fixture::new(Arc::new(Mutex::new(opened)), None).await;
    let refusal = fixture.execute("Cannot seize TS name").await.unwrap();
    assert_eq!(refusal["isError"], true);
    assert_eq!(
        lock_session_manager(&fixture.manager).get_session_name().as_deref(),
        Some("TS independent name")
    );

    let rust_directory = directory.path().join("rust");
    let rust_root = rust_directory.to_str().unwrap();
    let mut rust_manager = SessionManager::create(root, Some(rust_root), None).unwrap();
    rust_manager
        .set_metadata_control_db_path(Some(&db), SetMetadataControlDbPathOptions::default())
        .unwrap();
    rust_manager.persist_for_recovery().unwrap();
    let rust_path = rust_manager.get_session_file().unwrap().to_owned();
    let fixture = Fixture::new(Arc::new(Mutex::new(rust_manager)), None).await;
    fixture.execute("Rust created Rust tool").await.unwrap();
    let read = ts.request(&json!({"op":"read", "path":rust_path, "sessionDir":rust_root, "db":db}));
    assert_eq!(read["name"], "Rust created Rust tool", "{read}");
    assert_eq!(read["stored"], true);
    assert_eq!(
        read["state"],
        json!({"name":"Rust created Rust tool","hasStoredValue":true})
    );
}

#[tokio::test]
async fn real_extension_api_persists_publishes_events_and_never_requests_a_title() {
    use pi_ai::providers::faux::{
        FauxAssistantMessageOptions, FauxModelDefinition, RegisterFauxProviderOptions, faux_assistant_message,
        faux_provider,
    };
    use pi_coding_agent::core::agent_session::ExtensionBindings;
    use pi_coding_agent::core::auth_storage::AuthStorage;
    use pi_coding_agent::core::extensions::types::{AgentEndEvent, ExtensionMode};
    use pi_coding_agent::core::model_runtime::{CreateModelRuntimeOptions, ModelRuntime};
    use pi_coding_agent::core::resource_loader::InlineExtension;
    use pi_coding_agent::core::sdk::{CreateAgentSessionOptions, NoTools, create_agent_session};
    use pi_coding_agent::core::settings_manager::{SettingsManager, SettingsManagerCreateOptions};

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_str().unwrap();
    let db = directory.path().join("control.sqlite").display().to_string();
    let mut manager = SessionManager::create(root, Some(root), None).unwrap();
    manager
        .set_metadata_control_db_path(Some(&db), SetMetadataControlDbPathOptions::default())
        .unwrap();
    manager.persist_for_recovery().unwrap();
    let path = manager.get_session_file().unwrap().to_owned();
    let faux = faux_provider(RegisterFauxProviderOptions {
        provider: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "naming-fixture".to_owned(),
            name: None,
            reasoning: None,
            input: None,
            cost: None,
            context_window: Some(100_000.0),
            max_tokens: Some(4096.0),
        }]),
        ..Default::default()
    });
    faux.set_responses(vec![
        faux_assistant_message("Unrequested title", FauxAssistantMessageOptions::default()).into(),
    ]);
    let runtime = ModelRuntime::create(CreateModelRuntimeOptions {
        credentials: Some(Arc::new(AuthStorage::in_memory(
            serde_json::from_value(json!({"faux":{"type":"api_key", "key":"fixture-only"}})).unwrap(),
        ))),
        models_path: Some(None),
        refresh_on_create: Some(false),
        ..Default::default()
    })
    .await
    .unwrap();
    runtime.register_native_provider(Arc::clone(&faux.provider));
    let events = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&events);
    let factory = Arc::new(move |api: &pi_coding_agent::core::extensions::ExtensionApi| {
        let recorded = Arc::clone(&recorded);
        let result = session_autoname_extension(api).and_then(|()| {
            api.on_session_info_changed(
                move |event: pi_coding_agent::core::extensions::types::SessionInfoChangedEvent, _ctx| {
                    let recorded = Arc::clone(&recorded);
                    Box::pin(async move {
                        recorded.lock().unwrap().push(event.name);
                        Ok(None)
                    }) as pi_ai::request::BoxFuture<'static, _>
                },
            )
            .map_err(HandlerError::new)
        });
        Box::pin(async move { result }) as pi_ai::request::BoxFuture<'static, _>
    });
    let created = create_agent_session(CreateAgentSessionOptions {
        cwd: Some(root.to_owned()),
        agent_dir: Some(directory.path().join("agent").display().to_string()),
        model_runtime: Some(runtime),
        model: Some(faux.get_model(None).unwrap()),
        no_tools: Some(NoTools::All),
        session_manager: Some(manager),
        settings_manager: Some(Arc::new(Mutex::new(SettingsManager::in_memory(
            Default::default(),
            SettingsManagerCreateOptions::default(),
        )))),
        extension_factories: vec![InlineExtension::Factory {
            extension_path: Some("<inline:session-autoname>".to_owned()),
            factory,
        }],
        ..Default::default()
    })
    .await
    .unwrap();
    assert!(created.extensions_result.errors.is_empty());
    let session = created.session;
    session
        .bind_extensions(ExtensionBindings {
            mode: Some(ExtensionMode::Tui),
            control_db_path: Some(db.clone()),
            ..Default::default()
        })
        .await;
    let user: pi_agent_core::AgentMessage =
        serde_json::from_value(json!({"role":"user", "content":"Fix login flow", "timestamp":1})).unwrap();
    {
        let shared = session.session_manager_handle();
        let mut manager = lock_session_manager(&shared);
        manager.append_message(user.clone()).unwrap();
        manager
            .append_message(pi_agent_core::AgentMessage::Assistant(Box::new(
                faux_assistant_message("I will fix it", FauxAssistantMessageOptions::default()),
            )))
            .unwrap();
    }
    session
        .extension_runner()
        .emit(&mut ExtensionEvent::AgentEnd(AgentEndEvent {
            messages: vec![user],
            session_continuation: None,
        }))
        .await;
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    assert_eq!(
        faux.state().lock().unwrap().call_count,
        0,
        "a completed first exchange must not kick off a title request"
    );
    assert_eq!(
        lock_session_manager(&session.session_manager_handle()).get_session_name(),
        None
    );
    let tool = session
        .extension_runner()
        .get_tool_definition("set_session_name")
        .expect("tool exposed through session registry");
    for name in ["First title", "Renamed title"] {
        let result = (tool.execute)(
            "fixture".to_owned(),
            json!({"name":name}),
            None,
            None,
            session.extension_runner().create_context(),
        )
        .await
        .unwrap();
        assert_eq!(result.details, Some(json!({"name":name})));
    }
    session
        .extension_runner()
        .emit(&mut ExtensionEvent::AgentEnd(AgentEndEvent {
            messages: vec![],
            session_continuation: None,
        }))
        .await;
    tokio::task::yield_now().await;
    assert_eq!(
        faux.state().lock().unwrap().call_count,
        0,
        "naming must not issue a separate provider request"
    );
    assert_eq!(
        *events.lock().unwrap(),
        vec![Some("First title".to_owned()), Some("Renamed title".to_owned())]
    );
    let mut ts = oracle::TsOracle::spawn("set-session-name-tool-oracle.ts", SOURCE);
    let read = ts.request(&json!({"op":"read", "path":path, "sessionDir":root, "db":db}));
    assert_eq!(read["name"], "Renamed title", "{read}");
}
