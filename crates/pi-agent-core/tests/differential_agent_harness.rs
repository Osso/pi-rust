use crate::shared_oracle as oracle;
use pi_agent_core::harness::session::*;
use pi_agent_core::harness::*;
use pi_agent_core::{
    AgentError, AgentMessage, AgentTool, AgentToolExecutionContext, AgentToolResult, AgentToolUpdateCallback, ToolError,
};
use pi_ai::request::BoxFuture;
use pi_ai::types::TextOrImageContent;
use pi_js_compat::abort::AbortSignal;

struct FixtureTool {
    name: String,
    schema: serde_json::Value,
}
impl AgentTool for FixtureTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn label(&self) -> pi_js_compat::js_json::JsString {
        self.name.clone().into()
    }
    fn description(&self) -> pi_js_compat::js_json::JsString {
        "fixture".into()
    }
    fn parameters(&self) -> &serde_json::Value {
        &self.schema
    }
    fn execute(
        &self,
        _id: String,
        args: JsValue,
        _signal: Option<AbortSignal>,
        _update: AgentToolUpdateCallback,
        _execution: Option<AgentToolExecutionContext>,
    ) -> BoxFuture<'static, Result<AgentToolResult, ToolError>> {
        Box::pin(async move {
            Ok(AgentToolResult {
                content: vec![TextOrImageContent::Text(pi_ai::types::TextContent::new(string(
                    &args["value"],
                )))],
                details: Some(json!({"native":JsString::from_utf16(vec![0xd800])})),
                is_error: None,
                terminate: None,
            })
        })
    }
}
fn tool(name: &str) -> Arc<dyn AgentTool> {
    Arc::new(FixtureTool {
        name: name.into(),
        schema: serde_json::json!({"type":"object","properties":{"value":{"type":"string"}},"required":["value"]}),
    })
}
fn entry_value(entry: &SessionTreeEntry) -> JsValue {
    let mut object = entry.object().clone();
    for key in ["id", "parentId", "timestamp"] {
        object.shift_remove(key);
    }
    clean(JsValue::Object(object))
}
fn compact_value(value: &HarnessCompactResult) -> JsValue {
    let mut result = json!({"summary":value.summary,"firstKeptEntryId":value.first_kept_entry_id,"tokensBefore":value.tokens_before});
    if let Some(details) = &value.details {
        result["details"] = details.clone();
    }
    optional(&mut result, "durationMs", &value.duration_ms);
    result
}
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
fn prompt_options(operation: &JsValue) -> Option<AgentHarnessPromptOptions> {
    operation.get("images").map(|images| AgentHarnessPromptOptions {
        images: Some(from_value(images).unwrap()),
    })
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
                let scenario = scenario.clone();
                FauxResponseStep::Factory(Arc::new(move |context, options, _, _| {
                    let requests = requests.clone();
                    let model = model.clone();
                    let scenario = scenario.clone();
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
                        if scenario.starts_with("payload") {
                            let payload = options.stream.request.on_payload.unwrap()(
                                json!({"native":JsString::from_utf16(vec![0xd800])}),
                                model.clone(),
                            )
                            .await?;
                            let mut result = json!({});
                            if let Some(payload) = payload {
                                result["payloadResult"] = payload;
                            }
                            requests.lock().unwrap().push(result);
                        }
                        if scenario.starts_with("tool") && index == 0 {
                            return Ok(faux_assistant_message(
                                faux_tool_call("lookup", [("value".into(), json!("raw"))].into(), Some("call".into())),
                                Default::default(),
                            ));
                        }
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
    if scenario.starts_with("tool") {
        options.tools = vec![tool("lookup")];
    }
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
    if scenario.starts_with("payload") {
        let fail = scenario == "payload-error";
        removals.push(harness.on(
            HarnessHookKind::BeforeProviderPayload,
            Arc::new(move |_| {
                Box::pin(async move {
                    if fail {
                        return Err(AgentError::new(JsString::from_utf16(
                            "payload".encode_utf16().chain([0xd800]).collect(),
                        ))
                        .into());
                    }
                    Ok(Some(HarnessHookResult::BeforeProviderPayload(None)))
                })
            }),
        ));
        let recorded = results.clone();
        removals.push(harness.on(
            HarnessHookKind::BeforeProviderPayload,
            Arc::new(move |event| {
                let HarnessEvent::BeforeProviderPayload { payload, .. } = event else {
                    panic!("payload")
                };
                recorded
                    .lock()
                    .unwrap()
                    .push(json!({"payloadStage":2,"present":payload.is_some()}));
                Box::pin(async {
                    Ok(Some(HarnessHookResult::BeforeProviderPayload(Some(
                        json!({"finish":JsString::from_utf16(vec![0xdc00])}),
                    ))))
                })
            }),
        ));
    }
    if let Some(value) = request.get("treeHook") {
        let value = value.clone();
        removals.push(harness.on(
            HarnessHookKind::SessionBeforeTree,
            Arc::new(move |_| {
                let value = value.clone();
                Box::pin(async move {
                    Ok(Some(HarnessHookResult::SessionBeforeTree(SessionBeforeTreeResult {
                        cancel: value.get("cancel").and_then(JsValue::as_bool).unwrap_or(false),
                        summary: value
                            .get("summary")
                            .map(|summary| (string(&summary["summary"]), summary.get("details").cloned())),
                        custom_instructions: value.get("customInstructions").map(string),
                        replace_instructions: value.get("replaceInstructions").and_then(JsValue::as_bool),
                        label: value.get("label").map(string),
                    })))
                })
            }),
        ));
    }
    if let Some(value) = request.get("compactHook") {
        let value = value.clone();
        removals.push(harness.on(
            HarnessHookKind::SessionBeforeCompact,
            Arc::new(move |_| {
                let value = value.clone();
                Box::pin(async move {
                    Ok(Some(HarnessHookResult::SessionBeforeCompact {
                        cancel: value.get("cancel").and_then(JsValue::as_bool).unwrap_or(false),
                        compaction: value.get("compaction").map(|value| HarnessCompactResult {
                            summary: string(&value["summary"]),
                            first_kept_entry_id: string(&value["firstKeptEntryId"]),
                            tokens_before: value["tokensBefore"].as_f64().unwrap(),
                            duration_ms: value.get("durationMs").and_then(JsValue::as_f64),
                            details: value.get("details").cloned(),
                        }),
                    }))
                })
            }),
        ));
    }
    if scenario.starts_with("tool") {
        let recorded = results.clone();
        let scenario = scenario.clone();
        removals.push(harness.on(
            HarnessHookKind::ToolCall,
            Arc::new(move |event| {
                let HarnessEvent::ToolCall { input, .. } = event else {
                    panic!("call")
                };
                recorded.lock().unwrap().push(json!({"hook":"call","input":input}));
                let block = scenario == "tool-block";
                Box::pin(async move {
                    Ok(if block {
                        Some(HarnessHookResult::ToolCall(BeforeToolCallResult {
                            block: Some(true),
                            reason: Some(JsString::from_utf16("blocked".encode_utf16().chain([0xd800]).collect())),
                        }))
                    } else {
                        None
                    })
                })
            }),
        ));
        let recorded = results.clone();
        let terminate = request["scenario"].as_string().unwrap() == "tool-terminate";
        removals.push(harness.on(
            HarnessHookKind::ToolResult,
            Arc::new(move |event| {
                let HarnessEvent::ToolResult {
                    input,
                    content,
                    details,
                    is_error,
                    ..
                } = event
                else {
                    panic!("result")
                };
                let mut value = json!({"hook":"result","input":input,"content":native(&content),"isError":is_error});
                if let Some(details) = details {
                    value["details"] = details;
                }
                recorded.lock().unwrap().push(value);
                Box::pin(async move {
                    Ok(if terminate {
                        Some(HarnessHookResult::ToolResult(pi_agent_core::AfterToolCallResult {
                            content: Some(vec![TextOrImageContent::Text(pi_ai::types::TextContent::new(
                                JsString::from_utf16("patched".encode_utf16().chain([0xdc00]).collect()),
                            ))]),
                            details: Some(JsValue::Null),
                            is_error: Some(true),
                            terminate: Some(true),
                        }))
                    } else {
                        None
                    })
                })
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
            "next" => harness.next_turn(string(&operation["text"]),prompt_options(operation)).await.map(|_| JsValue::Null),
            "prompt" => { if scenario == "rollback" {*reject_drain.lock().unwrap() = true;} harness.prompt(string(&operation["text"]),prompt_options(operation)).await.map(|value| native(&value)) },
            "skill" => harness.skill(string(&operation["name"]),operation.get("instructions").map(string)).await.map(|value| native(&value)),
            "template" => harness.prompt_from_template(string(&operation["name"]),operation.get("args").and_then(JsValue::as_array).map(|args| args.iter().map(string).collect()).unwrap_or_default()).await.map(|value| native(&value)),
            "steer" => harness.steer(string(&operation["text"]),None).await.map(|_| JsValue::Null),
            "follow" => harness.follow_up(string(&operation["text"]),None).await.map(|_| JsValue::Null),
            "abort" => harness.abort().await.map(|value| json!({"clearedSteer":native(&value.cleared_steer),"clearedFollowUp":native(&value.cleared_follow_up)})),
            "compact" => harness.compact(operation.get("instructions").map(string)).await.map(|value| compact_value(&value)),
            "navigate" => {
                let options = operation.get("options").map(|value| NavigateTreeOptions {summarize:value.get("summarize").and_then(JsValue::as_bool).unwrap_or(false),custom_instructions:value.get("customInstructions").map(string),replace_instructions:value.get("replaceInstructions").and_then(JsValue::as_bool).unwrap_or(false),label:value.get("label").map(string)});
                harness.navigate_tree(string(&operation["id"]),options).await.map(|value| {let mut result = json!({"cancelled":value.cancelled}); if let Some(text) = value.editor_text {result["editorText"] = JsValue::String(text);}
                if let Some(entry) = value.summary_entry {result["summaryEntry"] = entry_value(&entry);} result})
            },
            "active" => harness.set_active_tools(operation["names"].as_array().unwrap().iter().map(|name| string(name).to_utf8().unwrap()).collect()).await.map(|_| JsValue::Null),
            "tools" => harness.set_tools(operation["names"].as_array().unwrap().iter().map(|name| tool(&string(name).to_utf8().unwrap())).collect(),operation.get("active").and_then(JsValue::as_array).map(|names| names.iter().map(|name| string(name).to_utf8().unwrap()).collect())).await.map(|_| JsValue::Null),
            "settings" => {
                let mut model = harness.get_model(); model.name = "changed".into(); harness.set_model(model).await.unwrap(); harness.set_thinking_level(ThinkingLevel::Low).await.unwrap(); harness.set_resources(AgentHarnessResources {skills:None,prompt_templates:Some(vec![PromptTemplate {name:"new".into(),description:None,content:JsString::from_utf16("native".encode_utf16().chain([0xdc00]).collect())}])}).await.unwrap(); harness.set_steering_mode(QueueMode::All).await; harness.set_follow_up_mode(QueueMode::All).await;
                Ok(json!({"modelName":harness.get_model().name,"tools":harness.get_tools().iter().map(|tool| tool.name()).collect::<Vec<_>>(),"active":harness.get_active_tools().iter().map(|tool| tool.name()).collect::<Vec<_>>(),"resources":{"promptTemplates":[{"name":"new","content":harness.get_resources().prompt_templates.unwrap()[0].content.clone()}]},"streamOptions":options_value(&harness.get_stream_options()),"steering":harness.get_steering_mode().as_str(),"followUp":harness.get_follow_up_mode().as_str()}))
            },
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
        "payload",
        "payload-error",
    ]
    .into_iter()
    .map(|scenario| json!({"scenario":scenario}))
    .collect::<Vec<_>>();
    cases.push(json!({"scenario":"queues","mode":"all"}));
    let image = json!({"type":"image","data":"AAAA","mimeType":"image/png"});
    cases.push(json!({"scenario":"basic","operations":[{"op":"next","text":"queued-image","images":[image]},{"op":"prompt","text":"image-prompt","images":[image]},{"op":"prompt","text":"empty-images","images":[]}]}));
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

#[tokio::test]
async fn working_harness_matches_frozen_navigation_and_settings() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-harness-oracle.ts",
        "packages/agent-core/src/harness/agent-harness.ts",
    );
    let assistant = json!({"role":"assistant","content":[{"type":"text","text":"answer"}],"api":"fixture-api","provider":"fixture","model":"faux-1","stopReason":"stop","timestamp":0,"usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}}});
    let mut entries = vec![];
    for (id, parent, message) in [
        ("u", None, to_value(&user("first")).unwrap()),
        ("a", Some("u"), assistant.clone()),
        ("v", Some("a"), to_value(&user("second")).unwrap()),
        ("b", Some("v"), assistant),
    ] {
        entries.push(json!({"type":"message","id":id,"parentId":parent,"timestamp":"now","message":message}));
    }
    let mut cases = vec![];
    for target in ["b", "a", "v", "u", "missing"] {
        cases.push(json!({"scenario":"basic","entries":entries,"operations":[{"op":"navigate","id":target}]}));
    }
    for tree_hook in [
        json!({"cancel":true}),
        json!({"summary":{"summary":"provided","details":[null,JsString::from_utf16(vec![0xd800])]},"label":"ignored"}),
        json!({"summary":{"summary":""}}),
    ] {
        cases.push(json!({"scenario":"basic","entries":entries,"treeHook":tree_hook,"operations":[{"op":"navigate","id":"a","options":{"label":"also ignored"}}]}));
    }
    cases.push(json!({"scenario":"basic","entries":entries,"treeHook":{"customInstructions":"custom","replaceInstructions":true},"operations":[{"op":"navigate","id":"a","options":{"summarize":true,"customInstructions":"old"}}]}));
    let mut custom = entries.clone();
    custom.push(json!({"type":"custom_message","id":"custom","parentId":"u","timestamp":"now","customType":"fixture","content":[{"type":"text","text":JsString::from_utf16(vec![0xd800])},{"type":"text","text":"tail"}],"display":true}));
    cases.push(json!({"scenario":"basic","entries":custom,"operations":[{"op":"navigate","id":"a"},{"op":"navigate","id":"custom"}]}));
    for compact_hook in [
        json!({"cancel":true}),
        json!({"compaction":{"summary":JsString::from_utf16(vec![0xd800]),"firstKeptEntryId":"v","tokensBefore":12.5,"durationMs":3.25,"details":[null,JsString::from_utf16(vec![0xdc00])]}}),
    ] {
        cases.push(json!({"scenario":"basic","entries":entries,"compactHook":compact_hook,"operations":[{"op":"compact","instructions":"custom"}]}));
    }
    cases.push(json!({"scenario":"basic","operations":[{"op":"tools","names":["x","x"]},{"op":"active","names":["missing","missing"]},{"op":"active","names":["missing"]},{"op":"tools","names":["x","y"],"active":["x"]},{"op":"tools","names":["z"]},{"op":"tools","names":["z"],"active":["z"]},{"op":"settings"}]}));
    for case in cases {
        let expected: JsValue = oracle.request(&case);
        assert!(
            expected.get("fixtureError").is_none(),
            "oracle fixture failed: {}",
            json::stringify_lossless(&expected)
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

#[tokio::test]
async fn working_harness_matches_frozen_tool_hooks() {
    let mut oracle = oracle::TsOracle::spawn(
        "agent-harness-oracle.ts",
        "packages/agent-core/src/harness/agent-harness.ts",
    );
    for scenario in ["tool", "tool-block", "tool-terminate"] {
        let case = json!({"scenario":scenario});
        let expected: JsValue = oracle.request(&case);
        assert!(
            expected.get("fixtureError").is_none(),
            "oracle fixture failed: {}",
            json::stringify_lossless(&expected)
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
