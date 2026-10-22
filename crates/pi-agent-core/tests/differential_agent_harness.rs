#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;
use pi_agent_core::harness::session::*;
use pi_agent_core::harness::*;
use pi_agent_core::{AgentError, AgentMessage};
use pi_ai::models::{CreateModelsOptions, MutableModels, create_models};
use pi_ai::providers::faux::*;
use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue, from_value, to_value},
};
use std::sync::{Arc, Mutex};

fn clean(value: JsValue) -> JsValue {
    match value {
        JsValue::Array(values) => JsValue::Array(values.into_iter().map(clean).collect()),
        JsValue::Object(values) => JsValue::Object(
            values
                .into_iter()
                .filter(|(key, _)| key != "timestamp" && key != "responseId")
                .map(|(key, value)| (key, clean(value)))
                .collect(),
        ),
        value => value,
    }
}
fn native<T: serde::Serialize>(value: &T) -> JsValue {
    clean(to_value(value).unwrap())
}
fn event_value(event: &HarnessEvent) -> JsValue {
    match event {
        HarnessEvent::QueueUpdate {
            steer,
            follow_up,
            next_turn,
        } => json!({"type":event.kind(),"steer":steer.len(),"followUp":follow_up.len(),"nextTurn":next_turn.len()}),
        HarnessEvent::SavePoint { had_pending_mutations } => {
            json!({"type":event.kind(),"hadPendingMutations":had_pending_mutations})
        }
        HarnessEvent::Settled { next_turn_count } => json!({"type":event.kind(),"nextTurnCount":next_turn_count}),
        _ => json!({"type":event.kind()}),
    }
}
fn error_value(error: AgentHarnessError) -> JsValue {
    let code = match error.code {
        AgentHarnessErrorCode::Busy => "busy",
        AgentHarnessErrorCode::InvalidState => "invalid_state",
        AgentHarnessErrorCode::InvalidArgument => "invalid_argument",
        AgentHarnessErrorCode::Session => "session",
        AgentHarnessErrorCode::Compaction => "compaction",
        AgentHarnessErrorCode::BranchSummary => "branch_summary",
        AgentHarnessErrorCode::Hook => "hook",
        AgentHarnessErrorCode::Auth => "auth",
        AgentHarnessErrorCode::Unknown => "unknown",
    };
    json!({"error":{"code":code,"message":error.message}})
}
fn user(text: impl Into<JsString>) -> AgentMessage {
    from_value(&json!({"role":"user","content":text.into(),"timestamp":1700000000000.0})).unwrap()
}
fn string(value: &JsValue) -> JsString {
    value.as_string().unwrap().clone()
}
fn optional<T: serde::Serialize>(object: &mut JsValue, key: &str, value: &Option<T>) {
    if let Some(value) = value {
        object[key] = native(value);
    }
}
fn stream_options(value: &JsValue) -> AgentHarnessStreamOptions {
    AgentHarnessStreamOptions {
        timeout_ms: value.get("timeoutMs").and_then(JsValue::as_f64),
        max_retries: value.get("maxRetries").and_then(JsValue::as_f64),
        max_retry_delay_ms: value.get("maxRetryDelayMs").and_then(JsValue::as_f64),
        headers: value.get("headers").and_then(JsValue::as_object).map(|headers| {
            headers
                .iter()
                .map(|(key, value)| (key.clone(), string(value)))
                .collect()
        }),
        metadata: value.get("metadata").and_then(JsValue::as_object).cloned(),
        ..Default::default()
    }
}
fn options_value(options: &AgentHarnessStreamOptions) -> JsValue {
    let mut result = json!({});
    optional(&mut result, "timeoutMs", &options.timeout_ms);
    optional(&mut result, "maxRetries", &options.max_retries);
    optional(&mut result, "maxRetryDelayMs", &options.max_retry_delay_ms);
    if let Some(headers) = &options.headers {
        result["headers"] = JsValue::Object(
            headers
                .iter()
                .map(|(key, value)| (key.clone(), JsValue::String(value.clone())))
                .collect(),
        );
    }
    if let Some(metadata) = &options.metadata {
        result["metadata"] = JsValue::Object(metadata.clone());
    }
    result
}
async fn run(request: &JsValue) -> JsValue {
    let scenario = request["scenario"].as_string().unwrap().to_utf8().unwrap();
    let faux = faux_provider(RegisterFauxProviderOptions {
        api: Some("fixture-api".into()),
        provider: Some("fixture".into()),
        tokens_per_second: Some(1000000000.0),
        token_size: Some(FauxTokenSize {
            min: Some(100000.0),
            max: Some(100000.0),
        }),
        ..Default::default()
    });
    let model = faux.get_model(None).unwrap();
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(faux.provider.clone());
    let requests = Arc::new(Mutex::new(Vec::new()));
    faux.set_responses(
        (0..8)
            .map(|index| {
                let requests = requests.clone();
                let model = model.clone();
                FauxResponseStep::Factory(Arc::new(move |context, options, _, _| {
                    let requests = requests.clone();
                    let model = model.clone();
                    Box::pin(async move {
                        let options = options.unwrap();
                        let mut captured = json!({});
                        optional(&mut captured, "timeoutMs", &options.stream.request.timeout_ms);
                        optional(&mut captured, "maxRetries", &options.stream.request.max_retries);
                        optional(
                            &mut captured,
                            "maxRetryDelayMs",
                            &options.stream.request.max_retry_delay_ms,
                        );
                        optional(&mut captured, "sessionId", &options.stream.session_id);
                        optional(&mut captured, "reasoning", &options.reasoning);
                        optional(&mut captured, "transport", &options.stream.transport);
                        optional(&mut captured, "cacheRetention", &options.stream.cache_retention);
                        if let Some(headers) = options.stream.request.headers {
                            captured["headers"] = JsValue::Object(
                                headers
                                    .into_iter()
                                    .filter_map(|(key, value)| value.map(|value| (key.into(), JsValue::String(value))))
                                    .collect(),
                            );
                        }
                        if let Some(metadata) = options.stream.metadata {
                            captured["metadata"] = JsValue::Object(metadata);
                        }
                        let mut entry =
                            json!({"model":model.id,"messages":native(&context.messages),"options":captured});
                        if let Some(prompt) = context.system_prompt {
                            entry["systemPrompt"] = JsValue::String(prompt);
                        }
                        requests.lock().unwrap().push(entry);
                        Ok(faux_assistant_message(
                            format!("reply-{}", index + 1),
                            Default::default(),
                        ))
                    })
                }))
            })
            .collect(),
    );
    let entries = request
        .get("entries")
        .and_then(JsValue::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| SessionTreeEntry::from_value(entry.clone()).unwrap())
                .collect()
        })
        .unwrap_or_default();
    let session = Session::new(Arc::new(
        InMemorySessionStorage::with_metadata(
            SessionMetadata {
                id: "session-1".into(),
                created_at: "now".into(),
            },
            entries,
        )
        .unwrap(),
    ));
    let env = Arc::new(NodeExecutionEnv::new(NodeExecutionEnvOptions {
        cwd: "/tmp".into(),
        ..Default::default()
    }));
    let mut options = AgentHarnessOptions::new(env, session.clone(), Arc::new(models), model.clone());
    if let Some(value) = request.get("streamOptions") {
        options.stream_options = stream_options(value);
    }
    if request
        .get("mode")
        .is_some_and(|mode| mode.as_string().is_some_and(|mode| mode == "all"))
    {
        options.steering_mode = QueueMode::All;
        options.follow_up_mode = QueueMode::All;
    }
    options.resources = AgentHarnessResources {
        skills: Some(vec![Skill {
            name: "skill".into(),
            description: "desc".into(),
            content: JsString::from_utf16("body".encode_utf16().chain([0xd800]).collect()),
            file_path: "/skills/SKILL.md".into(),
            disable_model_invocation: None,
        }]),
        prompt_templates: Some(vec![PromptTemplate {
            name: "template".into(),
            description: None,
            content: "$1 $@".into(),
        }]),
    };
    let harness = AgentHarness::new(options).unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let results = Arc::new(Mutex::new(Vec::new()));
    let activated = Arc::new(Mutex::new(false));
    let reject_drain = Arc::new(Mutex::new(false));
    let observe = events.clone();
    let listener_harness = harness.clone();
    let flag = activated.clone();
    let reject = reject_drain.clone();
    let kind = scenario.clone();
    let unsubscribe = harness.subscribe(Arc::new(move |event,_| {
        let harness = listener_harness.clone(); let flag = flag.clone(); let kind = kind.clone(); let reject = reject.clone(); let model = model.clone();
        observe.lock().unwrap().push(event_value(&event));
        Box::pin(async move {
            let is_turn_end = matches!(&event,HarnessEvent::Agent(pi_agent_core::AgentEvent::TurnEnd {..}));
            let is_assistant_start = matches!(&event,HarnessEvent::Agent(pi_agent_core::AgentEvent::MessageStart {message}) if message.as_assistant().is_some());
            let once = ((kind == "writes" || kind == "turn-listener-error") && is_turn_end) || (kind == "queues" && is_assistant_start);
            let first = if once { let mut flag = flag.lock().unwrap(); let first = !*flag; *flag = true; first } else { false };
            if first && kind == "writes" { harness.append_message(user(JsString::from_utf16("deferred".encode_utf16().chain([0xd800]).collect()))).await?; let mut model = model; model.name = "renamed".into(); harness.set_model(model).await?; harness.set_thinking_level(ThinkingLevel::High).await?; harness.set_active_tools(vec![]).await?; }
            if first && kind == "queues" { harness.steer("steer-1".into(),None).await?; harness.steer("steer-2".into(),None).await?; harness.follow_up("follow-1".into(),None).await?; harness.follow_up("follow-2".into(),None).await?; }
            if kind == "rollback" && matches!(&event,HarnessEvent::QueueUpdate {..}) && std::mem::take(&mut *reject.lock().unwrap()) { return Err(AgentError::new(JsString::from_utf16("queue".encode_utf16().chain([0xd800]).collect())).into()); }
            if kind == "failure-report" && matches!(&event,HarnessEvent::Agent(pi_agent_core::AgentEvent::MessageEnd {message}) if message.as_assistant().is_some()) { return Err(AgentError::new(JsString::from_utf16("report".encode_utf16().chain([0xd800]).collect())).into()); }
            if first && kind == "turn-listener-error" { harness.set_thinking_level(ThinkingLevel::High).await?; return Err(AgentError::new(JsString::from_utf16("turn".encode_utf16().chain([0xd800]).collect())).into()); }
            Ok(())
        })
    }));
    let mut removals = Vec::new();
    if scenario == "request-error" || scenario == "failure-report" {
        removals.push(harness.on(
            HarnessHookKind::BeforeProviderRequest,
            Arc::new(|_| {
                Box::pin(async {
                    Err(
                        AgentError::new(JsString::from_utf16("request".encode_utf16().chain([0xd800]).collect()))
                            .into(),
                    )
                })
            }),
        ));
    }
    if scenario == "start-error" {
        removals.push(harness.on(
            HarnessHookKind::BeforeAgentStart,
            Arc::new(|_| {
                Box::pin(async {
                    Err(AgentError::new(JsString::from_utf16("start".encode_utf16().chain([0xd800]).collect())).into())
                })
            }),
        ));
    }
    if scenario == "context" {
        removals.push(harness.on(
            HarnessHookKind::BeforeAgentStart,
            Arc::new(|_| {
                Box::pin(async {
                    Ok(Some(HarnessHookResult::BeforeAgentStart {
                        messages: Some(vec![user(JsString::from_utf16(
                            "injected".encode_utf16().chain([0xd800]).collect(),
                        ))]),
                        system_prompt: Some(JsString::from_utf16(
                            "override".encode_utf16().chain([0xd800]).collect(),
                        )),
                    }))
                })
            }),
        ));
        removals.push(harness.on(
            HarnessHookKind::Context,
            Arc::new(|event| {
                Box::pin(async move {
                    let HarnessEvent::Context { mut messages } = event else {
                        panic!("context")
                    };
                    messages.push(user(JsString::from_utf16(
                        "context".encode_utf16().chain([0xdc00]).collect(),
                    )));
                    Ok(Some(HarnessHookResult::Context(messages)))
                })
            }),
        ));
    }
    if scenario == "patches" {
        removals.push(harness.on(
            HarnessHookKind::BeforeProviderRequest,
            Arc::new(|_| {
                Box::pin(async {
                    Ok(Some(HarnessHookResult::BeforeProviderRequest(
                        AgentHarnessStreamOptionsPatch {
                            timeout_ms: OptionalPatch::Clear,
                            headers: OptionalPatch::Set(
                                [
                                    ("remove".into(), None),
                                    ("one".into(), Some(JsString::from_utf16(vec![0x61, 0xd800]))),
                                ]
                                .into(),
                            ),
                            metadata: OptionalPatch::Set(
                                [
                                    ("remove".into(), None),
                                    ("one".into(), Some(JsValue::String(JsString::from_utf16(vec![0xdc00])))),
                                ]
                                .into(),
                            ),
                            ..Default::default()
                        },
                    )))
                })
            }),
        ));
        let recorded = results.clone();
        removals.push(harness.on(
            HarnessHookKind::BeforeProviderRequest,
            Arc::new(move |event| {
                let HarnessEvent::BeforeProviderRequest { stream_options, .. } = event else {
                    panic!("request")
                };
                recorded.lock().unwrap().push(options_value(&stream_options));
                Box::pin(async {
                    Ok(Some(HarnessHookResult::BeforeProviderRequest(
                        AgentHarnessStreamOptionsPatch {
                            headers: OptionalPatch::Set([("two".into(), Some("b".into()))].into()),
                            metadata: OptionalPatch::Set(
                                [(
                                    "two".into(),
                                    Some(JsValue::Array(vec![
                                        JsValue::Null,
                                        JsValue::String(JsString::from_utf16(vec![0xd800])),
                                    ])),
                                )]
                                .into(),
                            ),
                            ..Default::default()
                        },
                    )))
                })
            }),
        ));
    }
    if scenario == "live-hooks" {
        let recorded = results.clone();
        let instance = harness.clone();
        let late_results = results.clone();
        let late: HarnessHook = Arc::new(move |_| {
            late_results.lock().unwrap().push(json!("late"));
            Box::pin(async {
                Ok(Some(HarnessHookResult::BeforeProviderRequest(
                    AgentHarnessStreamOptionsPatch {
                        timeout_ms: OptionalPatch::Set(42.0),
                        ..Default::default()
                    },
                )))
            })
        });
        removals.push(harness.on(
            HarnessHookKind::BeforeProviderRequest,
            Arc::new(move |_| {
                let _remove = instance.on(HarnessHookKind::BeforeProviderRequest, late.clone());
                recorded.lock().unwrap().push(json!("first"));
                Box::pin(async { Ok(None) })
            }),
        ));
    }
    let default =
        vec![json!({"op":"prompt","text":JsString::from_utf16("hello".encode_utf16().chain([0xd800]).collect())})];
    for operation in request
        .get("operations")
        .and_then(JsValue::as_array)
        .unwrap_or(&default)
    {
        let result: Result<JsValue,AgentHarnessError> = match operation["op"].as_string().unwrap().to_utf8().unwrap().as_str() {
            "next" => harness.next_turn(string(&operation["text"]),None).await.map(|_| JsValue::Null),
            "prompt" => { if scenario == "rollback" {*reject_drain.lock().unwrap() = true;} harness.prompt(string(&operation["text"]),None).await.map(|value| native(&value)) },
            "skill" => harness.skill(string(&operation["name"]),operation.get("instructions").map(string)).await.map(|value| native(&value)),
            "template" => harness.prompt_from_template(string(&operation["name"]),operation.get("args").and_then(JsValue::as_array).map(|args| args.iter().map(string).collect()).unwrap_or_default()).await.map(|value| native(&value)),
            "steer" => harness.steer(string(&operation["text"]),None).await.map(|_| JsValue::Null),
            "follow" => harness.follow_up(string(&operation["text"]),None).await.map(|_| JsValue::Null),
            "abort" => harness.abort().await.map(|value| json!({"clearedSteer":native(&value.cleared_steer),"clearedFollowUp":native(&value.cleared_follow_up)})),
            "compact" => harness.compact(None).await.map(|_| panic!("empty compaction fixture must fail")),
            "navigate" => harness.navigate_tree(string(&operation["id"]),None).await.map(|value| json!({"cancelled":value.cancelled})),
            _ => panic!("unsupported fixture operation"),
        };
        results.lock().unwrap().push(match result {
            Ok(value) => json!({"ok":value}),
            Err(error) => error_value(error),
        });
    }
    unsubscribe();
    for remove in removals {
        remove();
    }
    let entries: Vec<_> = session
        .get_entries()
        .await
        .unwrap()
        .into_iter()
        .map(|entry| {
            let mut object = entry.object().clone();
            for key in ["id", "parentId", "timestamp"] {
                object.shift_remove(key);
            }
            clean(JsValue::Object(object))
        })
        .collect();
    json!({"results":results.lock().unwrap().clone(),"events":events.lock().unwrap().clone(),"requests":requests.lock().unwrap().clone(),"entries":entries,"thinking":harness.get_thinking_level().as_str()})
}

#[tokio::test]
async fn working_harness_matches_frozen_turn_and_queue_matrix() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-harness-oracle.ts",
        "packages/agent-core/src/harness/agent-harness.ts",
    );
    let mut cases = vec![
        "basic",
        "request-error",
        "failure-report",
        "start-error",
        "context",
        "writes",
        "turn-listener-error",
        "queues",
        "live-hooks",
    ]
    .into_iter()
    .map(|scenario| json!({"scenario":scenario}))
    .collect::<Vec<_>>();
    cases.push(json!({"scenario":"queues","mode":"all"}));
    cases.push(json!({"scenario":"rollback","operations":[{"op":"next","text":"queued"},{"op":"prompt","text":"first"},{"op":"abort"}]}));
    cases.push(json!({"scenario":"basic","operations":[{"op":"skill","name":"skill","instructions":"more"},{"op":"template","name":"template","args":["one","two"]},{"op":"skill","name":"missing"},{"op":"template","name":"missing"},{"op":"steer","text":"idle"},{"op":"follow","text":"idle"}]}));
    cases.push(json!({"scenario":"basic","operations":[{"op":"compact"},{"op":"navigate","id":"missing"}]}));
    cases.push(json!({"scenario":"patches","streamOptions":{"headers":{"keep":"base","remove":"old"},"metadata":{"keep":true,"remove":null},"timeoutMs":1000,"maxRetries":2,"maxRetryDelayMs":3000}}));
    for case in cases {
        let expected: JsValue = oracle.request(&case);
        assert!(
            expected.get("fixtureError").is_none(),
            "oracle fixture failed: {expected:?}"
        );
        let actual = run(&case).await;
        assert!(
            actual == expected,
            "case {}\nactual {}\nexpected {}",
            json::stringify_lossless(&case),
            json::stringify_lossless(&actual),
            json::stringify_lossless(&expected)
        );
    }
}
