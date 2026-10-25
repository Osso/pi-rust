//! Differential coverage for the extension runner and registry.
//!
//! The oracle loads equivalent scripted TypeScript extensions through the real
//! `discoverAndLoadExtensions`/`ExtensionRunner`; the Rust side registers the
//! same handlers through the Rust extension API.

#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::js_json::encode_node_utf8;
use std::sync::{Arc, Mutex};

use pi_agent_core::harness::types::ThinkingLevel;
use pi_ai::types::{Model, TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::keybindings::KeybindingsManager;
use pi_coding_agent::core::model_registry::ProviderConfigInput;
use pi_coding_agent::core::model_resolver::ScopedModel;
use pi_coding_agent::core::session_control_db::RuntimeMailboxMessage;
use pi_coding_agent::core::session_manager::{SessionEntry, lock_session_manager};
use serde_json::{Map, Value, json};

macro_rules! h {
    ($ty:ty, [$($cap:ident),+], |$event:ident, $ctx:ident| $body:expr) => {{
        $(let $cap = std::sync::Arc::clone(&$cap);)+
        move |$event: $ty, $ctx: pi_coding_agent::core::extensions::ExtensionContext| {
            $(let $cap = std::sync::Arc::clone(&$cap);)+
            Box::pin(async move { $body })
                as pi_coding_agent::core::extensions::BoxFuture<
                    'static,
                    Result<Option<_>, pi_coding_agent::core::extensions::HandlerError>,
                >
        }
    }};
    ($ty:ty, |$event:ident, $ctx:ident| $body:expr) => {
        move |$event: $ty, $ctx: pi_coding_agent::core::extensions::ExtensionContext| {
            Box::pin(async move { $body })
                as pi_coding_agent::core::extensions::BoxFuture<
                    'static,
                    Result<Option<_>, pi_coding_agent::core::extensions::HandlerError>,
                >
        }
    };
}

fn temp_dir(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "pi-ext-diff-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_string_lossy().into_owned()
}

fn path_in(temp_dir: &str, name: &str) -> String {
    format!("{temp_dir}/extensions/bundle/{name}")
}

fn optional_entry(key: &str, value: Option<Value>) -> Value {
    match value {
        Some(value) => json!({ key: value }),
        None => json!({}),
    }
}

fn error_log(runner: &ExtensionRunner) -> Arc<Mutex<Vec<Value>>> {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&errors);
    runner.on_error(Arc::new(move |error| {
        sink.lock().unwrap().push(serde_json::to_value(error).unwrap());
    }));
    errors
}

fn errors_value(errors: &Arc<Mutex<Vec<Value>>>) -> Value {
    Value::Array(errors.lock().unwrap().clone())
}

fn oracle_for() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "extensions-runner-oracle.ts",
        "packages/coding-agent/src/core/extensions/runner.ts",
    )
}

async fn dispatch(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let logs = Arc::new(Mutex::new(Vec::new()));
    let l = Arc::clone(&logs);
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "a.ts"), move |api| {
        let l1 = Arc::clone(&l);
        api.on_agent_start(h!(AgentStartEvent, [l1], |_e, _c| {
            support::log(&l1, "e1:agent_start");
            Ok(None)
        }))
        .unwrap();
        let l2 = Arc::clone(&l1);
        api.on_agent_end(h!(AgentEndEvent, [l2], |e, _c| {
            support::log(&l2, format!("e1:agent_end:{}", e.messages.len()));
            Ok(None)
        }))
        .unwrap();
        let l3 = Arc::clone(&l1);
        api.on_context(h!(ContextEvent, [l3], |e, _c| {
            support::log(&l3, format!("e1:context:{}", e.messages.len()));
            let mut messages = e.messages.clone();
            messages.push(support::custom_message_with("custom", "e1", "x", 1));
            Ok(Some(ContextEventResult {
                messages: Some(messages),
            }))
        }))
        .unwrap();
        let l4 = Arc::clone(&l1);
        api.on_tool_result(h!(ToolResultEvent, [l4], |e, _c| {
            support::log(&l4, format!("e1:tool_result:{}", e.content.len()));
            let mut content = e.content.clone();
            content.push(TextOrImageContent::Text(TextContent::new("e1")));
            Ok(Some(ToolResultEventResult {
                content: Some(content),
                details: Some(JsValue::from(json!({"e1": true}))),
                is_error: None,
                usage: None,
            }))
        }))
        .unwrap();
        let l5 = Arc::clone(&l1);
        api.on_before_agent_start(h!(BeforeAgentStartEvent, [l5], |_e, ctx| {
            support::log(
                &l5,
                encode_node_utf8(&js_format!(
                    "e1:before_agent_start:{}",
                    ctx.get_system_prompt().unwrap()
                )),
            );
            Ok(Some(BeforeAgentStartEventResult {
                system_prompt: Some(js_format!("{}|e1", ctx.get_system_prompt().unwrap())),
                message: None,
            }))
        }))
        .unwrap();
        api.on_input(h!(InputEvent, |e, _c| {
            Ok(Some(InputEventResult::Transform {
                text: js_format!("{}[e1]", e.text),
                images: None,
            }))
        }))
        .unwrap();
        api.on_tool_call(h!(ToolCallEvent, |_e, _c| {
            Ok(Some(ToolCallEventResult {
                reason: Some(JsString::from("e1")),
                ..Default::default()
            }))
        }))
        .unwrap();
        api.on_resources_discover(h!(ResourcesDiscoverEvent, |_e, _c| {
            Ok(Some(ResourcesDiscoverResult {
                skill_paths: Some(vec![JsString::from("s1")]),
                prompt_paths: Some(vec![JsString::from("p1")]),
                theme_paths: None,
            }))
        }))
        .unwrap();
        let l6 = Arc::clone(&l1);
        api.on_user_bash(h!(UserBashEvent, [l6], |e, _c| {
            support::log(&l6, encode_node_utf8(&js_format!("e1:user_bash:{}", e.command)));
            Ok(Some(UserBashEventResult {
                operations: None,
                result: Some(JsValue::from(
                    json!({"output": "r1", "exitCode": 0, "cancelled": false, "truncated": false}),
                )),
            }))
        }))
        .unwrap();
        let l7 = Arc::clone(&l1);
        api.on_message_end(h!(MessageEndEvent, [l7], |e, _c| {
            support::log(
                &l7,
                encode_node_utf8(&js_format!("e1:message_end:{}", e.message.role())),
            );
            Ok(Some(MessageEndEventResult {
                message: Some(e.message.clone()),
            }))
        }))
        .unwrap();
    })
    .await;

    let l = Arc::clone(&logs);
    let b = support::extension(&runtime, temp_dir, &path_in(temp_dir, "b.ts"), move |api| {
        let l1 = Arc::clone(&l);
        api.on_agent_start(h!(AgentStartEvent, [l1], |_e, _c| {
            support::log(&l1, "e2:agent_start");
            Ok(None)
        }))
        .unwrap();
        let l2 = Arc::clone(&l);
        api.on_context(h!(ContextEvent, [l2], |e, _c| {
            support::log(&l2, format!("e2:context:{}", e.messages.len()));
            let mut messages = e.messages.clone();
            messages.push(support::custom_message_with("custom", "e2", "y", 2));
            Ok(Some(ContextEventResult {
                messages: Some(messages),
            }))
        }))
        .unwrap();
        api.on_tool_result(h!(ToolResultEvent, |e, _c| {
            let mut content = e.content.clone();
            content.push(TextOrImageContent::Text(TextContent::new("e2")));
            Ok(Some(ToolResultEventResult {
                content: Some(content),
                details: None,
                is_error: None,
                usage: None,
            }))
        }))
        .unwrap();
        api.on_before_agent_start(h!(BeforeAgentStartEvent, |_e, ctx| {
            Ok(Some(BeforeAgentStartEventResult {
                system_prompt: Some(js_format!("{}|e2", ctx.get_system_prompt().unwrap())),
                message: None,
            }))
        }))
        .unwrap();
        api.on_input(h!(InputEvent, |e, _c| {
            Ok(Some(InputEventResult::Transform {
                text: js_format!("{}[e2]", e.text),
                images: None,
            }))
        }))
        .unwrap();
        api.on_tool_call(h!(ToolCallEvent, |_e, _c| Ok(None))).unwrap();
        api.on_resources_discover(h!(ResourcesDiscoverEvent, |_e, _c| {
            Ok(Some(ResourcesDiscoverResult {
                skill_paths: None,
                prompt_paths: None,
                theme_paths: Some(vec![JsString::from("t2")]),
            }))
        }))
        .unwrap();
        api.on_message_end(h!(MessageEndEvent, |_e, _c| Ok(None))).unwrap();
    })
    .await;

    let mut runner = support::runner_for(vec![a, b], temp_dir, runtime.clone(), support::model_registry().await);
    let errors = error_log(&runner);
    support::bind(&mut runner);
    let mut steps = Vec::new();

    runner.emit(&mut ExtensionEvent::AgentStart(AgentStartEvent {})).await;
    steps.push(json!({"log": logs.lock().unwrap().clone()}));

    let context = runner.emit_context(Vec::new()).await.unwrap();
    steps.push(json!({"messages": context}));

    let tool_result = runner
        .emit_tool_result(ToolResultEvent {
            tool_name: ToolName::Custom("my_tool".to_owned()),
            tool_call_id: "call-1".to_owned(),
            input: Map::new()
                .into_iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect::<pi_js_compat::js_json::JsObject>(),
            content: vec![TextOrImageContent::Text(TextContent::new("base"))],
            details: Some(JsValue::from(json!({"initial": true}))),
            is_error: false,
            usage: None,
        })
        .await;
    steps.push(json!({"toolResult": tool_result}));

    let before_agent_start = runner
        .emit_before_agent_start(
            &JsString::from("p"),
            None,
            &JsString::from("base"),
            pi_coding_agent::core::system_prompt::BuildSystemPromptOptions {
                cwd: JsString::from(temp_dir.to_owned()),
                ..Default::default()
            },
        )
        .await;
    steps.push(json!({"beforeAgentStart": before_agent_start}));

    let input = runner
        .emit_input(&JsString::from("x"), None, InputSource::Interactive, None)
        .await;
    steps.push(json!({"input": input}));

    let mut tool_call = ToolCallEvent {
        tool_name: ToolName::Bash,
        tool_call_id: "call-2".to_owned(),
        bypass_permissions: false,
        input: Map::from_iter([("command".to_owned(), json!("pwd"))])
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    };
    let tool_call_result = runner.emit_tool_call(&mut tool_call).await.unwrap();
    steps.push(json!({"toolCall": tool_call_result}));

    let resources = runner
        .emit_resources_discover(&JsString::from(temp_dir), ResourcesDiscoverReason::Startup)
        .await;
    steps.push(json!({"resources": resources}));

    let user_bash = runner
        .emit_user_bash(&UserBashEvent {
            command: JsString::from("ls"),
            exclude_from_context: false,
            cwd: JsString::from(temp_dir.to_owned()),
        })
        .await;
    let user_bash = user_bash.map(|result| match result.result {
        Some(result) => json!({"result": result}),
        None => json!({}),
    });
    steps.push(json!({"userBash": user_bash}));

    let message_end = runner
        .emit_message_end(MessageEndEvent {
            message: support::custom_message_with("custom", "m", "c", 1),
        })
        .await;
    steps.push(json!({"messageEnd": message_end}));

    runner
        .emit(&mut ExtensionEvent::AgentEnd(AgentEndEvent {
            messages: Vec::new(),
            session_continuation: None,
        }))
        .await;
    steps.push(json!({"log": logs.lock().unwrap().clone()}));

    json!({"steps": steps, "errors": errors_value(&errors)})
}

async fn errors_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "a.ts"), |api| {
        api.on_context(h!(ContextEvent, |_e, _c| Err(HandlerError::new("boom"))))
            .unwrap();
        api.on_tool_call(h!(ToolCallEvent, |_e, _c| Err(HandlerError::new("call boom"))))
            .unwrap();
        api.on_message_end(h!(MessageEndEvent, |_e, _c| {
            Ok(Some(MessageEndEventResult {
                message: Some(support::custom_message_with("custom", "x", "c", 1)),
            }))
        }))
        .unwrap();
    })
    .await;

    let mut runner = support::runner_for(vec![a], temp_dir, runtime, support::model_registry().await);
    let errors = error_log(&runner);
    support::bind(&mut runner);
    let mut steps = Vec::new();

    let context = runner.emit_context(Vec::new()).await.unwrap();
    steps.push(json!({"context": context}));

    let mut tool_call = ToolCallEvent {
        tool_name: ToolName::Bash,
        tool_call_id: "call-1".to_owned(),
        bypass_permissions: false,
        input: Map::from_iter([("command".to_owned(), json!("pwd"))])
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    };
    match runner.emit_tool_call(&mut tool_call).await {
        Ok(_) => steps.push(json!({"toolCall": "no-error"})),
        Err(error) => steps.push(json!({"toolCallError": error.message})),
    }

    let message_end = runner
        .emit_message_end(MessageEndEvent {
            message: support::custom_message_with("user", "m", "c", 1),
        })
        .await;
    steps.push(optional_entry(
        "messageEnd",
        message_end.map(|message| serde_json::to_value(message).unwrap()),
    ));

    json!({"steps": steps, "errors": errors_value(&errors)})
}

async fn shortcuts_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let specs = [
        ("a.ts", "ctrl+c"),
        ("b.ts", "ctrl+p"),
        ("c.ts", "ctrl+v"),
        ("d.ts", "ctrl+shift+x"),
        ("e.ts", "ctrl+shift+x"),
    ];
    let mut extensions = Vec::new();
    for (name, shortcut) in specs {
        let shortcut = shortcut.to_owned();
        extensions.push(
            support::extension(&runtime, temp_dir, &path_in(temp_dir, name), move |api| {
                api.register_shortcut(
                    JsString::from(shortcut.clone()),
                    None,
                    Arc::new(|_ctx: ExtensionContext| {
                        Box::pin(async { Ok(()) })
                            as pi_coding_agent::core::extensions::BoxFuture<'static, Result<(), HandlerError>>
                    }),
                )
                .unwrap();
            })
            .await,
        );
    }
    let runner = support::runner_for(extensions, temp_dir, runtime, support::model_registry().await);
    let default_keybindings = KeybindingsManager::new(Vec::new(), None).get_effective_config();
    let shortcuts = runner.get_shortcuts(&default_keybindings);
    let first = json!({
        "keys": shortcuts.keys().cloned().collect::<Vec<_>>(),
        "diagnostics": runner.get_shortcut_diagnostics(),
    });

    let mut rebound = default_keybindings.clone();
    for (keybinding, keys) in &mut rebound {
        if keybinding == "app.interrupt" {
            *keys = vec![JsString::from("ctrl+x")];
        }
    }
    let rebound_shortcuts = runner.get_shortcuts(&rebound);
    let second = json!({
        "keys": rebound_shortcuts.keys().cloned().collect::<Vec<_>>(),
        "diagnostics": runner.get_shortcut_diagnostics(),
    });

    json!({"steps": [first, second], "errors": []})
}

async fn commands_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "a.ts"), |api| {
        api.register_command(
            "shared-cmd",
            Some("First command".into()),
            None,
            Arc::new(|_args: JsString, _ctx: ExtensionCommandContext| {
                Box::pin(async { Ok(()) })
                    as pi_coding_agent::core::extensions::BoxFuture<'static, Result<(), HandlerError>>
            }),
        )
        .unwrap();
    })
    .await;
    let b = support::extension(&runtime, temp_dir, &path_in(temp_dir, "b.ts"), |api| {
        api.register_command(
            "shared-cmd",
            Some("Second command".into()),
            None,
            Arc::new(|_args: JsString, _ctx: ExtensionCommandContext| {
                Box::pin(async { Ok(()) })
                    as pi_coding_agent::core::extensions::BoxFuture<'static, Result<(), HandlerError>>
            }),
        )
        .unwrap();
        api.register_command(
            "solo",
            Some("Solo".into()),
            None,
            Arc::new(|_args: JsString, _ctx: ExtensionCommandContext| {
                Box::pin(async { Ok(()) })
                    as pi_coding_agent::core::extensions::BoxFuture<'static, Result<(), HandlerError>>
            }),
        )
        .unwrap();
    })
    .await;
    let runner = support::runner_for(vec![a, b], temp_dir, runtime, support::model_registry().await);
    let commands = runner.get_registered_commands();
    let step = json!({
        "commands": commands.iter().map(|command| json!({
            "name": command.name,
            "invocationName": command.invocation_name,
            "description": command.description,
        })).collect::<Vec<_>>(),
        "diagnostics": runner.get_command_diagnostics(),
    });
    json!({
        "steps": [
            step,
            {"byInvocation": runner.get_command("shared-cmd:2").and_then(|command| command.description)},
            {"missing": runner.get_command("not-exists").is_none()},
        ],
        "errors": []
    })
}

async fn flags_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "a.ts"), |api| {
        api.register_flag(
            "shared-flag",
            Some("first".to_owned()),
            FlagType::Boolean,
            Some(FlagValue::Boolean(true)),
        )
        .unwrap();
    })
    .await;
    let b = support::extension(&runtime, temp_dir, &path_in(temp_dir, "b.ts"), |api| {
        api.register_flag(
            "shared-flag",
            Some("second".to_owned()),
            FlagType::Boolean,
            Some(FlagValue::Boolean(false)),
        )
        .unwrap();
        api.register_flag("test-flag", Some("Test flag".to_owned()), FlagType::Boolean, None)
            .unwrap();
    })
    .await;
    let runner = support::runner_for(vec![a, b], temp_dir, runtime, support::model_registry().await);
    let flags = runner
        .get_flags()
        .into_iter()
        .map(|(name, flag)| {
            json!({
                "name": name,
                "description": flag.description,
                "type": flag.flag_type,
                "default": flag.default,
            })
        })
        .collect::<Vec<_>>();
    let values = runner
        .get_flag_values()
        .into_iter()
        .map(|(name, value)| json!([name, value]))
        .collect::<Vec<_>>();
    runner.set_flag_value("--test-flag", FlagValue::Boolean(true));
    let values_after = runner
        .get_flag_values()
        .into_iter()
        .map(|(name, value)| json!([name, value]))
        .collect::<Vec<_>>();
    json!({
        "steps": [
            {"flags": flags},
            {"values": values},
            {"values": values_after},
        ],
        "errors": []
    })
}

async fn input_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "a.ts"), |api| {
        api.on_input(h!(InputEvent, |e, _c| {
            if e.text == "h" {
                Ok(Some(InputEventResult::Handled))
            } else {
                Ok(Some(InputEventResult::Transform {
                    text: js_format!("{}[1]", e.text),
                    images: None,
                }))
            }
        }))
        .unwrap();
    })
    .await;
    let b = support::extension(&runtime, temp_dir, &path_in(temp_dir, "b.ts"), |api| {
        api.on_input(h!(InputEvent, |e, _c| {
            Ok(Some(InputEventResult::Transform {
                text: js_format!("{}[2]", e.text),
                images: None,
            }))
        }))
        .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![a, b], temp_dir, runtime, support::model_registry().await);
    support::bind(&mut runner);
    let images = vec![pi_ai::types::ImageContent::new("orig", "image/png")];
    json!({
        "steps": [
            {"continue": runner.emit_input(&JsString::from("x"), None, InputSource::Interactive, None).await},
            {"transform": runner.emit_input(&JsString::from("hi"), Some(InputImages::new(images)), InputSource::Interactive, None).await},
            {"handled": runner.emit_input(&JsString::from("h"), None, InputSource::Interactive, None).await},
        ],
        "errors": []
    })
}

async fn input_images_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "images.ts"), |api| {
        api.on_input(h!(InputEvent, |e, _c| {
            match e.text.to_utf8().as_deref().unwrap_or_default() {
                "same" => Ok(Some(InputEventResult::Transform {
                    text: e.text.clone(),
                    images: e.images.clone(),
                })),
                "copy" => Ok(Some(InputEventResult::Transform {
                    text: e.text.clone(),
                    images: e
                        .images
                        .as_ref()
                        .map(|images| InputImages::new(images.as_slice().to_vec())),
                })),
                _ => Ok(None),
            }
        }))
        .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![a], temp_dir, runtime, support::model_registry().await);
    support::bind(&mut runner);
    let images = vec![pi_ai::types::ImageContent::new("orig", "image/png")];
    json!({
        "steps": [
            {"same": runner.emit_input(&JsString::from("same"), Some(InputImages::new(images.clone())), InputSource::Interactive, None).await},
            {"copy": runner.emit_input(&JsString::from("copy"), Some(InputImages::new(images)), InputSource::Interactive, None).await},
        ],
        "errors": []
    })
}

async fn project_trust_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "a.ts"), |api| {
        api.on_project_trust(|_event: ProjectTrustEvent, _ctx: ProjectTrustContext| {
            Box::pin(async {
                Ok(ProjectTrustEventResult {
                    trusted: ProjectTrustEventDecision::Undecided,
                    remember: Some(true),
                })
            })
                as pi_coding_agent::core::extensions::BoxFuture<'static, Result<ProjectTrustEventResult, HandlerError>>
        })
        .unwrap();
    })
    .await;
    let b = support::extension(&runtime, temp_dir, &path_in(temp_dir, "b.ts"), |api| {
        api.on_project_trust(|_event: ProjectTrustEvent, _ctx: ProjectTrustContext| {
            Box::pin(async {
                Ok(ProjectTrustEventResult {
                    trusted: ProjectTrustEventDecision::No,
                    remember: Some(true),
                })
            })
                as pi_coding_agent::core::extensions::BoxFuture<'static, Result<ProjectTrustEventResult, HandlerError>>
        })
        .unwrap();
    })
    .await;
    let result = LoadExtensionsResult {
        extensions: vec![a, b],
        errors: Vec::new(),
        runtime,
    };
    let ctx = ProjectTrustContext {
        cwd: JsString::from(temp_dir.to_owned()),
        mode: ExtensionMode::Tui,
        has_ui: false,
        ui: Arc::new(NoOpUiContext),
    };
    let (decision, errors) = emit_project_trust_event(
        &result,
        ProjectTrustEvent {
            cwd: JsString::from(temp_dir.to_owned()),
        },
        &ctx,
    )
    .await;
    json!({
        "steps": [{
            "result": decision,
            "errors": errors,
        }],
        "errors": []
    })
}

fn provider_config() -> ProviderConfigInput {
    let model: pi_ai::types::Model = serde_json::from_value(json!({
        "id": "instant-model",
        "name": "Instant Model",
        "api": "openai-completions",
        "provider": "placeholder",
        "baseUrl": "https://provider.test/v1",
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 1.25},
        "contextWindow": 128000,
        "maxTokens": 4096,
    }))
    .unwrap();
    ProviderConfigInput {
        base_url: Some(JsString::from("https://provider.test/v1")),
        api_key: Some(JsString::from("provider-test-key")),
        api: Some("openai-completions".to_owned()),
        models: Some(vec![model.into()]),
        ..Default::default()
    }
}

fn provider_record(op: &str, name: &str, config: Option<&ProviderConfigInput>) -> Value {
    match config {
        Some(config) => json!({
            "op": op,
            "name": name,
            "baseUrl": config.base_url,
            "api": config.api,
            "apiKey": config.api_key,
            "authHeader": config.auth_header,
            "modelIds": config.models.as_ref().map(|models| models.iter().map(|model| model.id.clone()).collect::<Vec<_>>()),
        }),
        None => json!({"op": op, "name": name}),
    }
}

async fn providers_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "a.ts"), |api| {
        let config = ProviderConfigInput {
            base_url: Some(JsString::from("https://queued.test")),
            api_key: Some(JsString::from("k")),
            api: Some("openai-completions".to_owned()),
            models: Some(vec![
                serde_json::from_value(json!({
                    "id": "queued-model",
                    "name": "Queued",
                    "api": "openai-completions",
                    "provider": "placeholder",
                    "baseUrl": "https://queued.test",
                    "reasoning": false,
                    "input": ["text"],
                    "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
                    "contextWindow": 1000,
                    "maxTokens": 100,
                }))
                .unwrap(),
            ]),
            ..Default::default()
        };
        api.register_provider("queued", config).unwrap();
    })
    .await;
    let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let register_calls = Arc::clone(&calls);
    let unregister_calls = Arc::clone(&calls);
    let provider_actions = ProviderActions {
        register_provider: Some(Arc::new(move |name, config| {
            register_calls
                .lock()
                .unwrap()
                .push(provider_record("register", name, Some(config)));
            Ok(())
        })),
        unregister_provider: Some(Arc::new(move |name| {
            unregister_calls
                .lock()
                .unwrap()
                .push(provider_record("unregister", name, None));
            Ok(())
        })),
    };
    let mut runner = support::runner_for(vec![a], temp_dir, runtime.clone(), support::model_registry().await);
    runner
        .bind_core(support::actions(), support::context_actions(), Some(provider_actions))
        .unwrap();
    let mut steps = vec![
        json!({"calls": calls.lock().unwrap().clone()}),
        json!({"pending": runtime.pending_provider_registrations().len()}),
    ];
    let _ = runtime.register_provider(
        "instant-provider",
        provider_config(),
        Some(&JsString::from("<inline:test>")),
    );
    steps.push(json!({"calls": calls.lock().unwrap().clone()}));
    runtime
        .unregister_provider("instant-provider", Some(&JsString::from("<inline:test>")))
        .unwrap();
    steps.push(json!({"calls": calls.lock().unwrap().clone()}));
    json!({"steps": steps, "errors": []})
}

async fn mailbox_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "a.ts"), |api| {
        api.on_runtime_mailbox(h!(RuntimeMailboxEvent, |_e, _c| {
            Ok(Some(RuntimeMailboxEventResult { handled: true }))
        }))
        .unwrap();
    })
    .await;
    let b = support::extension(&runtime, temp_dir, &path_in(temp_dir, "b.ts"), |api| {
        api.on_runtime_mailbox(h!(RuntimeMailboxEvent, |_e, _c| Err(HandlerError::new("mailbox boom"))))
            .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![a, b], temp_dir, runtime, support::model_registry().await);
    let errors = error_log(&runner);
    support::bind(&mut runner);
    let message: RuntimeMailboxMessage = serde_json::from_value(json!({
        "id": 1,
        "recipient": {"sessionId": "r"},
        "sender": {"sessionId": "s"},
        "kind": "message",
        "body": "hi",
        "storeRef": {"messageId": "1", "sessionPath": "/tmp/s.jsonl"},
        "status": "claimed",
        "createdAt": "2026-01-01T00:00:00.000Z",
        "updatedAt": "2026-01-01T00:00:00.000Z",
    }))
    .unwrap();
    let step = match runner.emit_runtime_mailbox(RuntimeMailboxEvent { message }).await {
        Ok(result) => json!({"result": result}),
        Err(error) => json!({"error": error.message}),
    };
    json!({"steps": [step], "errors": errors_value(&errors)})
}

fn compaction_preparation() -> CompactionPreparation {
    CompactionPreparation {
        first_kept_entry_id: JsString::from("e1"),
        messages_to_summarize: Vec::new(),
        turn_prefix_messages: Vec::new(),
        is_split_turn: false,
        tokens_before: 10.0,
        previous_summary: None,
        file_ops: pi_agent_core::FileOperations::default(),
        settings: pi_coding_agent::core::compaction::CompactionSettings {
            threshold_percent: None,
            enabled: true,
            reserve_tokens: 1.0,
            keep_recent_tokens: 2.0,
        },
    }
}

async fn session_events_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let logs = Arc::new(Mutex::new(Vec::new()));
    let l = Arc::clone(&logs);
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "a.ts"), move |api| {
        let l1 = Arc::clone(&l);
        api.on_session_before_switch(h!(SessionBeforeSwitchEvent, [l1], |_e, _c| {
            support::log(&l1, "switch:a");
            Ok(None)
        }))
        .unwrap();
        api.on_session_before_switch(h!(SessionBeforeSwitchEvent, |_e, _c| {
            Ok(Some(SessionBeforeSwitchResult { cancel: Some(true) }))
        }))
        .unwrap();
        let l2 = Arc::clone(&l);
        api.on_session_before_switch(h!(SessionBeforeSwitchEvent, [l2], |_e, _c| {
            support::log(&l2, "switch:c");
            Ok(None)
        }))
        .unwrap();
        api.on_session_before_fork(h!(SessionBeforeForkEvent, |_e, _c| {
            Ok(Some(SessionBeforeForkResult {
                cancel: None,
                skip_conversation_restore: Some(true),
            }))
        }))
        .unwrap();
        api.on_session_before_compact(h!(SessionBeforeCompactEvent, |_e, _c| {
            Ok(Some(SessionBeforeCompactResult { cancel: Some(true) }))
        }))
        .unwrap();
        let l3 = Arc::clone(&l);
        api.on_session_before_compact(h!(SessionBeforeCompactEvent, [l3], |_e, _c| {
            support::log(&l3, "compact:b");
            Ok(None)
        }))
        .unwrap();
        api.on_compaction(h!(CompactionEvent, |_e, _c| {
            Ok(Some(CompactionEventResult {
                cancel: None,
                compaction: Some(CompactionResult {
                    summary: JsString::from("s"),
                    first_kept_entry_id: JsString::from("e1"),
                    tokens_before: 3.0,
                    duration_ms: None,
                    estimated_tokens_after: None,
                    kept_from_previous_context_tokens: None,
                    compacted_result_tokens: None,
                    compacted_result_bytes: None,
                    source: None,
                    usage: None,
                    provider_native: None,
                    details: None,
                }),
            }))
        }))
        .unwrap();
        let l4 = Arc::clone(&l);
        api.on_compaction(h!(CompactionEvent, [l4], |_e, _c| {
            support::log(&l4, "compaction:b");
            Ok(None)
        }))
        .unwrap();
        api.on_session_compaction_source(h!(SessionCompactionSourceEvent, |_e, _c| {
            Ok(Some(SessionCompactionSourceResult {
                source: Some(CompactionSourceInfo {
                    r#type: CompactionSourceType::Local,
                    provider: "p".to_owned(),
                    model: "m".to_owned(),
                    endpoint: None,
                }),
            }))
        }))
        .unwrap();
        api.on_session_compaction_source(h!(SessionCompactionSourceEvent, |_e, _c| {
            Ok(Some(SessionCompactionSourceResult {
                source: Some(CompactionSourceInfo {
                    r#type: CompactionSourceType::OpenAiRemote,
                    provider: "p2".to_owned(),
                    model: "m2".to_owned(),
                    endpoint: Some(JsString::from("https://e")),
                }),
            }))
        }))
        .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![a], temp_dir, runtime, support::model_registry().await);
    support::bind(&mut runner);
    let mut steps = Vec::new();

    let mut switch = ExtensionEvent::SessionBeforeSwitch(SessionBeforeSwitchEvent {
        reason: SessionSwitchReason::New,
        target_session_file: Some(JsString::from("/tmp/s.jsonl")),
    });
    steps.push(json!({"switch": runner.emit(&mut switch).await.session_before_switch()}));

    let mut fork = ExtensionEvent::SessionBeforeFork(SessionBeforeForkEvent {
        entry_id: JsString::from("e"),
        position: ForkPosition::At,
    });
    steps.push(json!({"fork": runner.emit(&mut fork).await.session_before_fork()}));

    let preparation = compaction_preparation();
    let mut before_compact = ExtensionEvent::SessionBeforeCompact(SessionBeforeCompactEvent {
        preparation: preparation.clone(),
        branch_entries: Vec::<SessionEntry>::new(),
        custom_instructions: None,
        reason: CompactionReason::Manual,
        will_retry: false,
        signal: None,
    });
    steps.push(json!({"beforeCompact": runner.emit(&mut before_compact).await.session_before_compact()}));

    let mut compaction = ExtensionEvent::Compaction(CompactionEvent {
        preparation,
        branch_entries: Vec::new(),
        custom_instructions: None,
        reason: CompactionReason::Manual,
        will_retry: false,
        signal: None,
    });
    steps.push(json!({"compaction": runner.emit(&mut compaction).await.compaction()}));

    let mut source = ExtensionEvent::SessionCompactionSource(SessionCompactionSourceEvent {
        reason: CompactionReason::Manual,
        will_retry: false,
    });
    steps.push(json!({"source": runner.emit(&mut source).await.session_compaction_source()}));

    steps.push(json!({"log": logs.lock().unwrap().clone()}));
    json!({"steps": steps, "errors": []})
}

async fn mutations_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "a.ts"), |api| {
        api.on_tool_call(h!(ToolCallEvent, |e, _c| {
            let mut input = e.input;
            input.insert(JsString::from("a"), JsValue::from(json!(1)));
            Ok(Some(ToolCallEventResult {
                input: Some(input),
                ..Default::default()
            }))
        }))
        .unwrap();
        api.on_tool_call(h!(ToolCallEvent, |e, _c| {
            let mut input = e.input;
            let next = input.get("a").and_then(JsValue::as_f64).unwrap_or_default() as i64 + 1;
            input.insert(JsString::from("b"), JsValue::from(json!(next)));
            Ok(Some(ToolCallEventResult {
                reason: Some(JsString::from("mut")),
                input: Some(input),
                ..Default::default()
            }))
        }))
        .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![a], temp_dir, runtime, support::model_registry().await);
    support::bind(&mut runner);
    let mut event = ToolCallEvent {
        tool_name: ToolName::Bash,
        tool_call_id: "call-mut".to_owned(),
        bypass_permissions: false,
        input: Map::new()
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    };
    let tool_call = runner.emit_tool_call(&mut event).await.unwrap();
    json!({
        "steps": [
            {"toolCall": tool_call, "input": JsValue::Object(event.input).to_serde().unwrap()},
        ],
        "errors": []
    })
}

fn test_model(id: &str) -> Model {
    serde_json::from_value(json!({
        "id": id,
        "name": id,
        "api": "openai-completions",
        "provider": "placeholder",
        "baseUrl": "https://example.test/v1",
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 10000,
        "maxTokens": 1000,
    }))
    .unwrap()
}

fn read_live_context(ctx: &ExtensionContext) -> Value {
    json!({
        "cwd": ctx.cwd().unwrap(),
        "mode": ctx.mode().unwrap(),
        "hasUI": ctx.has_ui().unwrap(),
        "model": ctx.model().unwrap().map(|model| model.id),
        "scopedModels": ctx
            .scoped_models()
            .unwrap()
            .into_iter()
            .map(|scoped| scoped.model.id)
            .collect::<Vec<_>>(),
        "thinkingLevel": ctx.thinking_level().unwrap(),
        "controlDbPath": ctx.control_db_path().unwrap(),
        "multiAgentAgentId": ctx.multi_agent_agent_id().unwrap(),
        "multiAgentParentSessionId": ctx.multi_agent_parent_session_id().unwrap(),
        "multiAgentRequiresAgentId": ctx.multi_agent_requires_agent_id().unwrap(),
    })
}

async fn context_live_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let current_model: Arc<Mutex<Option<Model>>> = Arc::new(Mutex::new(None));
    let current_scoped: Arc<Mutex<Vec<ScopedModel>>> = Arc::new(Mutex::new(Vec::new()));
    let current_thinking = Arc::new(Mutex::new(ThinkingLevel::Off));
    let control_db_path: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let agent_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let parent_session_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let requires_agent_id = Arc::new(Mutex::new(false));

    let model_getter = Arc::clone(&current_model);
    let scoped_getter = Arc::clone(&current_scoped);
    let thinking_getter = Arc::clone(&current_thinking);
    let control_getter = Arc::clone(&control_db_path);
    let agent_getter = Arc::clone(&agent_id);
    let parent_getter = Arc::clone(&parent_session_id);
    let requires_getter = Arc::clone(&requires_agent_id);
    let mut runner = ExtensionRunner::new(
        Vec::new(),
        runtime,
        temp_dir,
        support::session_manager(temp_dir),
        support::model_registry().await,
        None,
    );
    runner
        .bind_core(
            support::actions(),
            ExtensionContextActions {
                get_model: Arc::new(move || model_getter.lock().unwrap().clone()),
                get_thinking_level: Some(Arc::new(move || *thinking_getter.lock().unwrap())),
                get_scoped_models: Some(Arc::new(move || scoped_getter.lock().unwrap().clone())),
                get_control_db_path: Some(Arc::new(move || {
                    control_getter.lock().unwrap().clone().map(JsString::from)
                })),
                get_multi_agent_agent_id: Some(Arc::new(move || {
                    agent_getter.lock().unwrap().clone().map(JsString::from)
                })),
                get_multi_agent_parent_session_id: Some(Arc::new(move || {
                    parent_getter.lock().unwrap().clone().map(JsString::from)
                })),
                get_no_supervisor: None,
                get_multi_agent_requires_agent_id: Some(Arc::new(move || *requires_getter.lock().unwrap())),
                ..support::context_actions()
            },
            None,
        )
        .unwrap();

    let ctx = runner.create_context();
    let command = runner.create_command_context();
    let before = read_live_context(&ctx);
    let before_command = read_live_context(&command);

    runner.set_cwd(format!("{temp_dir}/moved"));
    *current_model.lock().unwrap() = Some(test_model("model-a"));
    *current_scoped.lock().unwrap() = vec![ScopedModel {
        model: test_model("model-a"),
        thinking_level: Some(ThinkingLevel::High),
    }];
    *current_thinking.lock().unwrap() = ThinkingLevel::High;
    *control_db_path.lock().unwrap() = Some(format!("{temp_dir}/control.sqlite"));
    *agent_id.lock().unwrap() = Some("agent-1".to_owned());
    *parent_session_id.lock().unwrap() = Some("parent-1".to_owned());
    *requires_agent_id.lock().unwrap() = true;
    runner.set_ui_context(None, ExtensionMode::Json);

    let after = read_live_context(&ctx);
    let after_command = read_live_context(&command);

    runner.invalidate(None);
    let stale = vec![
        ctx.cwd().unwrap_err().message,
        ctx.mode().unwrap_err().message,
        ctx.model().unwrap_err().message,
        command.cwd().unwrap_err().message,
        command.get_system_prompt_options().unwrap_err().message,
    ];

    json!({
        "steps": [
            {"before": before, "beforeCommand": before_command},
            {"after": after, "afterCommand": after_command},
            {"stale": stale},
        ],
        "errors": []
    })
}

async fn provider_request_scenario(temp_dir: &str) -> Value {
    let runtime = support::runtime();
    let a = support::extension(&runtime, temp_dir, &path_in(temp_dir, "a.ts"), |api| {
        api.on_before_provider_request(h!(BeforeProviderRequestEvent, |e, _c| {
            let mut payload = e.payload.clone();
            payload["a"] = JsValue::from(json!(1));
            Ok(Some(payload))
        }))
        .unwrap();
        // The oracle mutates payload.b in place without returning; native handlers must return the
        // replacement (approved return-only deviation, compatibility.md).
        api.on_before_provider_request(h!(BeforeProviderRequestEvent, |e, _c| {
            let mut payload = e.payload;
            payload["b"] = JsValue::from(json!(2));
            Ok(Some(payload))
        }))
        .unwrap();
        api.on_before_provider_request(h!(BeforeProviderRequestEvent, |_e, _c| Ok(Some(JsValue::Null))))
            .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![a], temp_dir, runtime, support::model_registry().await);
    support::bind(&mut runner);
    let payload = runner
        .emit_before_provider_request(JsValue::from(json!({"base": true})))
        .await;
    json!({"steps": [{"payload": payload}, {"log": []}], "errors": []})
}

fn compare(name: &str, oracle: &mut oracle::TsOracle, actual: Value) {
    let expected = oracle.request(&json!({"id": name, "operation": "run", "scenario": name}));
    assert_eq!(expected["ok"], true, "{name}: {expected}");
    assert_eq!(actual, expected["output"], "scenario {name}");
}

#[tokio::test]
async fn extensions_match_the_pinned_typescript_runner() {
    let mut oracle = oracle_for();

    let dir = temp_dir("dispatch");
    let actual = support::normalize(&dispatch(&dir).await, &dir);
    compare("dispatch", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("errors");
    let actual = support::normalize(&errors_scenario(&dir).await, &dir);
    compare("errors", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("shortcuts");
    let actual = support::normalize(&shortcuts_scenario(&dir).await, &dir);
    compare("shortcuts", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("commands");
    let actual = support::normalize(&commands_scenario(&dir).await, &dir);
    compare("commands", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("flags");
    let actual = support::normalize(&flags_scenario(&dir).await, &dir);
    compare("flags", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("input");
    let actual = support::normalize(&input_scenario(&dir).await, &dir);
    compare("input", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("input_images");
    let actual = support::normalize(&input_images_scenario(&dir).await, &dir);
    compare("input_images", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("project_trust");
    let actual = support::normalize(&project_trust_scenario(&dir).await, &dir);
    compare("project_trust", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("providers");
    let actual = support::normalize(&providers_scenario(&dir).await, &dir);
    compare("providers", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("mailbox");
    let actual = support::normalize(&mailbox_scenario(&dir).await, &dir);
    compare("mailbox", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("session_events");
    let actual = support::normalize(&session_events_scenario(&dir).await, &dir);
    compare("session_events", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("mutations");
    let actual = support::normalize(&mutations_scenario(&dir).await, &dir);
    compare("mutations", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("context_live");
    let actual = support::normalize(&context_live_scenario(&dir).await, &dir);
    compare("context_live", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();

    let dir = temp_dir("provider_request");
    let actual = support::normalize(&provider_request_scenario(&dir).await, &dir);
    compare("provider_request", &mut oracle, actual);
    std::fs::remove_dir_all(&dir).ok();
}

// ===== Behavioral runner cases =====

use pi_coding_agent::core::extensions::loader::{command_handler, shortcut_handler};

fn tool_definition(name: &str, description: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.to_owned(),
        label: name.into(),
        description: description.into(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: json!({"type": "object"}),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(|_id, _params, _signal, _on_update, _ctx| {
            Box::pin(async {
                Ok(pi_agent_core::AgentToolResult {
                    content: Vec::new(),
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        render_call: None,
        render_result: None,
    }
}

#[tokio::test]
async fn keeps_the_first_registered_tool_and_definition() {
    let runtime = support::runtime();
    let a = support::extension(&runtime, "/tmp", "/tmp/a.ts", |api| {
        api.register_tool(tool_definition("shared", "first")).unwrap();
        api.register_tool(tool_definition("only_a", "a")).unwrap();
    })
    .await;
    let b = support::extension(&runtime, "/tmp", "/tmp/b.ts", |api| {
        api.register_tool(tool_definition("shared", "second")).unwrap();
    })
    .await;
    let runner = support::runner_for(vec![a, b], "/tmp", runtime, support::model_registry().await);

    let tools = runner.get_all_registered_tools();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].definition.description, "first");
    assert_eq!(tools[1].definition.name, "only_a");
    assert_eq!(runner.get_tool_definition("shared").unwrap().description, "first");
    assert!(runner.get_tool_definition("missing").is_none());
}

#[tokio::test]
async fn collects_renderers_and_transformers_in_load_order() {
    let runtime = support::runtime();
    let a = support::extension(&runtime, "/tmp", "/tmp/a.ts", |api| {
        api.register_markdown_transformer(Arc::new(|markdown, _| format!("{markdown}a")))
            .unwrap();
        api.register_message_renderer("my-type", Arc::new(|_, _, _| None))
            .unwrap();
        api.register_entry_renderer("my-entry", Arc::new(|_, _, _| None))
            .unwrap();
    })
    .await;
    let b = support::extension(&runtime, "/tmp", "/tmp/b.ts", |api| {
        api.register_markdown_transformer(Arc::new(|markdown, _| format!("{markdown}b")))
            .unwrap();
    })
    .await;
    let runner = support::runner_for(vec![a, b], "/tmp", runtime, support::model_registry().await);

    assert_eq!(runner.get_markdown_transformers().len(), 2);
    assert!(runner.get_message_renderer("my-type").is_some());
    assert!(runner.get_message_renderer("missing").is_none());
    assert!(runner.get_entry_renderer("my-entry").is_some());
    assert!(runner.get_entry_renderer("missing").is_none());
}

#[tokio::test]
async fn command_context_passes_fork_options_through() {
    let runtime = support::runtime();
    let runner = support::runner_for(Vec::new(), "/tmp", runtime, support::model_registry().await);
    let calls = Arc::new(Mutex::new(Vec::<(JsString, Option<ForkOptions>)>::new()));
    let fork_calls = Arc::clone(&calls);
    let mut runner = runner;
    runner.bind_command_context(Some(ExtensionCommandContextActions {
        show_approval_selector: Arc::new(|| Ok(())),
        show_sandbox_selector: Arc::new(|| Ok(())),
        set_sandbox_profile: Arc::new(|_, _| Ok(())),
        wait_for_idle: Arc::new(|| Box::pin(async {})),
        new_session: Arc::new(|_| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        fork: Arc::new(move |entry_id, options| {
            fork_calls.lock().unwrap().push((entry_id, options.clone()));
            Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })
        }),
        navigate_tree: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        switch_session: Arc::new(|_, _| Box::pin(async { Ok(SessionTransitionResult { cancelled: false }) })),
        reload: Arc::new(|| Box::pin(async { Ok(()) })),
        restart: Arc::new(|_| Box::pin(async { Ok(()) })),
    }));

    let context = runner.create_command_context();
    context.fork(JsString::from("entry-1"), None).await.unwrap();
    let options = ForkOptions {
        position: Some(ForkPosition::At),
        with_session: None,
    };
    context.fork(JsString::from("entry-2"), Some(options)).await.unwrap();

    let calls = calls.lock().unwrap();
    assert_eq!(calls[0].0, "entry-1");
    assert!(calls[0].1.is_none());
    assert_eq!(calls[1].0, "entry-2");
    assert_eq!(calls[1].1.as_ref().unwrap().position, Some(ForkPosition::At));
}

#[tokio::test]
async fn stale_contexts_reject_actions() {
    let runtime = support::runtime();
    let runner = support::runner_for(Vec::new(), "/tmp", runtime, support::model_registry().await);
    runner.invalidate(Some("stale ctx"));
    assert_eq!(runner.get_active_tools().unwrap_err().message, "stale ctx");
    let error = runner.create_command_context().new_session(None).await.unwrap_err();
    assert_eq!(error.message, "stale ctx");
}

const STALE_CONTEXT_MESSAGE: &str = "This extension ctx is stale after session replacement or reload. Do not use a captured pi or command ctx after ctx.newSession(), ctx.fork(), ctx.switchSession(), or ctx.reload(). For newSession, fork, and switchSession, move post-replacement work into withSession and use the ctx passed to withSession. For reload, do not use the old ctx after await ctx.reload().";

#[tokio::test]
async fn context_members_read_live_runner_state() {
    let runtime = support::runtime();
    let model: Arc<Mutex<Option<Model>>> = Arc::new(Mutex::new(None));
    let getter = Arc::clone(&model);
    let mut runner = ExtensionRunner::new(
        Vec::new(),
        runtime,
        "/tmp",
        support::session_manager("/tmp"),
        support::model_registry().await,
        None,
    );
    let ctx = runner.create_context();
    let command = runner.create_command_context();
    assert_eq!(ctx.cwd().unwrap(), "/tmp");
    assert_eq!(ctx.mode().unwrap(), ExtensionMode::Print);
    assert!(!ctx.has_ui().unwrap());
    assert!(ctx.model().unwrap().is_none());

    runner
        .bind_core(
            support::actions(),
            ExtensionContextActions {
                get_model: Arc::new(move || getter.lock().unwrap().clone()),
                ..support::context_actions()
            },
            None,
        )
        .unwrap();
    *model.lock().unwrap() = Some(test_model("model-a"));
    runner.set_cwd("/tmp/moved");
    runner.set_ui_context(None, ExtensionMode::Json);

    assert_eq!(ctx.cwd().unwrap(), "/tmp/moved");
    assert_eq!(ctx.mode().unwrap(), ExtensionMode::Json);
    assert!(!ctx.has_ui().unwrap());
    assert_eq!(ctx.model().unwrap().unwrap().id, "model-a");
    assert_eq!(command.cwd().unwrap(), "/tmp/moved");
    assert_eq!(command.model().unwrap().unwrap().id, "model-a");
}

#[tokio::test]
async fn context_set_thinking_level_invokes_the_session_setter() {
    let runtime = support::runtime();
    let current = Arc::new(Mutex::new(ThinkingLevel::Off));
    let getter = Arc::clone(&current);
    let setter_target = Arc::clone(&current);
    let mut runner = ExtensionRunner::new(
        Vec::new(),
        runtime,
        "/tmp",
        support::session_manager("/tmp"),
        support::model_registry().await,
        None,
    );
    runner
        .bind_core(
            support::actions(),
            ExtensionContextActions {
                get_thinking_level: Some(Arc::new(move || *getter.lock().unwrap())),
                set_thinking_level: Some(Arc::new(move |level| {
                    *setter_target.lock().unwrap() = level;
                    Ok(())
                })),
                ..support::context_actions()
            },
            None,
        )
        .unwrap();
    let ctx = runner.create_context();

    ctx.set_thinking_level(ThinkingLevel::Ultra).unwrap();

    assert_eq!(ctx.get_thinking_level().unwrap(), ThinkingLevel::Ultra);
    runner.invalidate(None);
    assert_eq!(
        ctx.set_thinking_level(ThinkingLevel::Low).unwrap_err().message,
        STALE_CONTEXT_MESSAGE
    );
}

#[tokio::test]
async fn stale_context_reads_throw_the_typescript_error_text() {
    let runtime = support::runtime();
    let runner = support::runner_for(Vec::new(), "/tmp", runtime, support::model_registry().await);
    let ctx = runner.create_context();
    let command = runner.create_command_context();
    runner.invalidate(None);

    for error in [
        ctx.cwd().unwrap_err(),
        ctx.mode().unwrap_err(),
        ctx.model().unwrap_err(),
        ctx.is_idle().unwrap_err(),
        ctx.get_system_prompt().unwrap_err(),
        command.cwd().unwrap_err(),
        command.get_system_prompt_options().unwrap_err(),
    ] {
        assert_eq!(error.message, STALE_CONTEXT_MESSAGE);
    }
}

#[tokio::test]
async fn gates_and_reviewers_patch_tool_input() {
    let runtime = support::runtime();
    let a = support::extension(&runtime, "/tmp", "/tmp/a.ts", |api| {
        api.register_tool_gate(Arc::new(|event: ToolCallEvent, _ctx: ExtensionContext| {
            Box::pin(async move {
                let mut input = event.input.clone();
                input.insert(JsString::from("gate"), JsValue::from(json!(true)));
                Ok(Some(ToolCallEventResult {
                    input: Some(input),
                    ..Default::default()
                }))
            }) as BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>>
        }))
        .unwrap();
        api.register_approval_reviewer(Arc::new(|_event: ToolCallEvent, _ctx: ExtensionContext| {
            Box::pin(async {
                Ok(Some(ApprovalReviewerResult::Allow {
                    updated_input: Some(
                        Map::from_iter([("approved".to_owned(), json!(true))])
                            .into_iter()
                            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                            .collect::<pi_js_compat::js_json::JsObject>(),
                    ),
                }))
            }) as BoxFuture<'static, Result<Option<ApprovalReviewerResult>, HandlerError>>
        }))
        .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![a], "/tmp", runtime, support::model_registry().await);
    support::bind(&mut runner);

    let mut event = ToolCallEvent {
        tool_name: ToolName::Bash,
        tool_call_id: "call".to_owned(),
        bypass_permissions: false,
        input: Map::from_iter([("command".to_owned(), json!("pwd"))])
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    };
    runner.emit_tool_gates(&mut event).await.unwrap();
    assert_eq!(event.input.get("gate"), Some(&JsValue::Bool(true)));

    runner.emit_approval_reviewers(&mut event).await.unwrap();
    assert_eq!(event.input.get("approved"), Some(&JsValue::Bool(true)));
    assert!(event.input.get("command").is_none());
}

#[tokio::test]
async fn wrapper_does_not_inject_old_added_tool_names() {
    let runtime = support::runtime();
    let active_tools = Arc::new(Mutex::new(vec!["existing".to_owned()]));
    let tool_active = Arc::clone(&active_tools);
    let mut definition = tool_definition("ext", "ext");
    definition.execute = Arc::new(move |_id, _params, _signal, _on_update, _ctx| {
        let active = Arc::clone(&tool_active);
        Box::pin(async move {
            active.lock().unwrap().push("added".to_owned());
            Ok(pi_agent_core::AgentToolResult {
                content: Vec::new(),
                details: None,
                is_error: None,
                terminate: None,
            })
        })
    });
    let a = support::extension(&runtime, "/tmp", "/tmp/a.ts", move |api| {
        api.register_tool(definition).unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![a], "/tmp", runtime, support::model_registry().await);
    let mut actions = support::actions();
    let get_active = Arc::clone(&active_tools);
    actions.get_active_tools = Arc::new(move || Ok(get_active.lock().unwrap().clone()));
    runner.bind_core(actions, support::context_actions(), None).unwrap();

    let registered = runner.get_all_registered_tools();
    let tools = wrap_registered_tools(registered, &runner);
    let result = tools[0]
        .execute(
            "call".to_owned(),
            JsValue::from(json!({})),
            None,
            Arc::new(|_| Ok(())),
            None,
        )
        .await
        .unwrap();
    assert_eq!(serde_json::to_value(result).unwrap().get("addedToolNames"), None);
}

#[tokio::test]
async fn session_shutdown_helper_reports_handler_presence() {
    let runtime = support::runtime();
    let a = support::extension(&runtime, "/tmp", "/tmp/a.ts", |api| {
        api.on_session_shutdown(|_event: SessionShutdownEvent, _ctx: ExtensionContext| {
            Box::pin(async { Ok(None) }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
        })
        .unwrap();
    })
    .await;
    let runner = support::runner_for(vec![a], "/tmp", runtime, support::model_registry().await);
    assert!(runner.has_handlers("session_shutdown"));
    assert!(
        emit_session_shutdown_event(
            &runner,
            SessionShutdownEvent {
                reason: SessionShutdownReason::Quit,
                target_session_file: None,
            }
        )
        .await
    );
    assert!(
        !emit_session_shutdown_event(
            &support::runner_for(Vec::new(), "/tmp", support::runtime(), support::model_registry().await),
            SessionShutdownEvent {
                reason: SessionShutdownReason::Quit,
                target_session_file: None,
            }
        )
        .await
    );
}

#[tokio::test]
async fn handler_count_is_the_number_of_events_with_handlers() {
    let runtime = support::runtime();
    let empty = support::extension(&runtime, "/tmp", "/tmp/empty.ts", |_api| {}).await;
    let a = support::extension(&runtime, "/tmp", "/tmp/a.ts", |api| {
        api.on_tool_call(h!(ToolCallEvent, |_e, _c| Ok(None))).unwrap();
        api.on_tool_call(h!(ToolCallEvent, |_e, _c| Ok(None))).unwrap();
        api.on_input(h!(InputEvent, |_e, _c| Ok(None))).unwrap();
    })
    .await;
    {
        let empty = empty.read();
        assert_eq!((empty.handlers.len(), empty.handlers.is_empty()), (0, true));
    }
    {
        let a = a.read();
        assert_eq!((a.handlers.len(), a.handlers.is_empty()), (2, false));
    }
}

#[tokio::test]
async fn cancelled_context_signal_aborts_dispatch() {
    let runtime = support::runtime();
    let a = support::extension(&runtime, "/tmp", "/tmp/a.ts", |api| {
        api.on_context(|_event: ContextEvent, _ctx: ExtensionContext| {
            Box::pin(async { Ok(None) }) as BoxFuture<'static, Result<Option<ContextEventResult>, HandlerError>>
        })
        .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![a], "/tmp", runtime, support::model_registry().await);
    let controller_token = pi_js_compat::abort::AbortController::new();
    let token = controller_token.signal();
    controller_token.abort(None);
    let mut actions = support::context_actions();
    let signal = token.clone();
    actions.get_signal = Arc::new(move || Some(signal.clone()));
    runner.bind_core(support::actions(), actions, None).unwrap();

    let error = runner.emit_context(Vec::new()).await.unwrap_err();
    assert_eq!(error.message, "Agent run aborted");
}

#[tokio::test]
async fn reports_invalid_queued_provider_registration() {
    let runtime = support::runtime();
    let a = support::extension(&runtime, "/tmp", "/tmp/a.ts", |api| {
        api.register_provider(
            "broken-provider",
            ProviderConfig {
                stream_simple: Some(Arc::new(|_model, _context, _options| panic!("should not run"))),
                ..Default::default()
            },
        )
        .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![a], "/tmp", runtime, support::model_registry().await);
    let errors = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&errors);
    runner.on_error(Arc::new(move |error| sink.lock().unwrap().push(error.clone())));
    runner
        .bind_core(support::actions(), support::context_actions(), None)
        .unwrap();
    let errors = errors.lock().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].event, "register_provider");
    assert_eq!(
        errors[0].error,
        "Provider broken-provider: \"api\" is required when registering streamSimple."
    );
}

#[tokio::test]
async fn project_trust_collects_handler_errors_and_keeps_scanning() {
    let runtime = support::runtime();
    let a = support::extension(&runtime, "/tmp", "/tmp/a.ts", |api| {
        api.on_project_trust(|_event: ProjectTrustEvent, _ctx: ProjectTrustContext| {
            Box::pin(async { Err(HandlerError::new("trust boom")) })
                as BoxFuture<'static, Result<ProjectTrustEventResult, HandlerError>>
        })
        .unwrap();
    })
    .await;
    let b = support::extension(&runtime, "/tmp", "/tmp/b.ts", |api| {
        api.on_project_trust(|_event: ProjectTrustEvent, _ctx: ProjectTrustContext| {
            Box::pin(async {
                Ok(ProjectTrustEventResult {
                    trusted: ProjectTrustEventDecision::Yes,
                    remember: None,
                })
            }) as BoxFuture<'static, Result<ProjectTrustEventResult, HandlerError>>
        })
        .unwrap();
    })
    .await;
    let result = LoadExtensionsResult {
        extensions: vec![a, b],
        errors: Vec::new(),
        runtime,
    };
    let ctx = ProjectTrustContext {
        cwd: JsString::from("/tmp"),
        mode: ExtensionMode::Print,
        has_ui: false,
        ui: Arc::new(NoOpUiContext),
    };
    let (decision, errors) = emit_project_trust_event(
        &result,
        ProjectTrustEvent {
            cwd: JsString::from("/tmp"),
        },
        &ctx,
    )
    .await;
    assert_eq!(decision.unwrap().trusted, ProjectTrustEventDecision::Yes);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].error, "trust boom");
}

#[tokio::test]
async fn tool_result_usage_is_not_forwarded() {
    let runtime = support::runtime();
    let a = support::extension(&runtime, "/tmp", "/tmp/a.ts", |api| {
        api.on_tool_result(|_event: ToolResultEvent, _ctx: ExtensionContext| {
            Box::pin(async {
                Ok(Some(ToolResultEventResult {
                    is_error: Some(true),
                    ..Default::default()
                }))
            }) as BoxFuture<'static, Result<Option<ToolResultEventResult>, HandlerError>>
        })
        .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![a], "/tmp", runtime, support::model_registry().await);
    support::bind(&mut runner);
    let result = runner
        .emit_tool_result(ToolResultEvent {
            tool_name: ToolName::Custom("x".to_owned()),
            tool_call_id: "c".to_owned(),
            input: Map::new()
                .into_iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect::<pi_js_compat::js_json::JsObject>(),
            content: Vec::new(),
            details: None,
            is_error: false,
            usage: None,
        })
        .await
        .unwrap();
    assert_eq!(result.is_error, Some(true));
    assert!(result.usage.is_none());
}

// ===== Discovery cases =====

use std::fs;
use std::path::{Path, PathBuf};

fn discovery_temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pi-ext-discovery-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn factory() -> ExtensionFactory {
    Arc::new(|_api| Box::pin(async { Ok(()) }))
}

#[test]
fn discovers_direct_files_and_index_entries() {
    let dir = discovery_temp_dir("direct");
    let extensions = dir.join(".pi").join("extensions");
    write(&extensions.join("foo.ts"), "export default function() {}");
    write(&extensions.join("bar.js"), "export default function() {}");
    write(
        &extensions.join("my-extension").join("index.ts"),
        "export default function() {}",
    );
    write(
        &extensions.join("my-package").join("index.js"),
        "export default function() {}",
    );

    let paths = discover_extension_paths(
        &[],
        &JsString::from(dir.to_str().unwrap()),
        &JsString::from(dir.to_str().unwrap()),
    );
    let names: Vec<String> = paths
        .iter()
        .map(|path| {
            Path::new(&encode_node_utf8(&path))
                .strip_prefix(&dir)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        vec![
            ".pi/extensions/bar.js",
            ".pi/extensions/foo.ts",
            ".pi/extensions/my-extension/index.ts",
            ".pi/extensions/my-package/index.js",
        ]
    );
    assert_eq!(names.len(), 4);
    fs::remove_dir_all(dir).ok();
}

#[test]
fn package_manifest_entries_keep_declared_order() {
    let dir = discovery_temp_dir("manifest");
    let package = dir.join(".pi").join("extensions").join("bundle");
    write(
        &package.join("package.json"),
        r#"{"pi":{"extensions":["second.ts","first.ts","missing.ts"]}}"#,
    );
    write(&package.join("first.ts"), "export default function() {}");
    write(&package.join("second.ts"), "export default function() {}");

    let paths = discover_extension_paths(
        &[],
        &JsString::from(dir.to_str().unwrap()),
        &JsString::from(dir.to_str().unwrap()),
    );
    let names: Vec<String> = paths
        .iter()
        .map(|path| {
            Path::new(&encode_node_utf8(&path))
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(names, vec!["second.ts", "first.ts"]);
    fs::remove_dir_all(dir).ok();
}

#[test]
fn configured_directories_use_manifest_or_index() {
    let dir = discovery_temp_dir("configured");
    let configured = dir.join("external");
    write(&configured.join("index.js"), "export default function() {}");
    let other = dir.join("other");
    write(&other.join("a.ts"), "export default function() {}");

    let paths = discover_extension_paths(
        &[
            JsString::from(configured.to_string_lossy().into_owned()),
            JsString::from(other.to_string_lossy().into_owned()),
        ],
        &JsString::from(dir.to_str().unwrap()),
        &JsString::from(dir.to_str().unwrap()),
    );
    let names: Vec<String> = paths
        .iter()
        .map(|path| {
            Path::new(&encode_node_utf8(&path))
                .strip_prefix(&dir)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(names, vec!["external/index.js", "other/a.ts"]);
    fs::remove_dir_all(dir).ok();
}

#[test]
fn disabled_extension_names_filter_paths_and_inline_ids() {
    let disabled = vec![
        JsString::from(" foo.ts "),
        JsString::from("<first-party:goal>"),
        JsString::default(),
    ];
    assert!(is_extension_path_disabled(
        &disabled,
        &JsString::from("/tmp/foo.ts"),
        &JsString::from("/tmp/foo.ts")
    ));
    assert!(is_extension_path_disabled(
        &disabled,
        &JsString::from("foo.ts"),
        &JsString::from("/tmp/foo.ts")
    ));
    assert!(!is_extension_path_disabled(
        &disabled,
        &JsString::from("/tmp/bar.ts"),
        &JsString::from("/tmp/bar.ts")
    ));
    assert!(is_extension_factory_disabled(
        &disabled,
        &JsString::from("<first-party:goal>")
    ));
    assert!(!is_extension_factory_disabled(
        &disabled,
        &JsString::from("<first-party:loop>")
    ));
    assert_eq!(
        inline_extension_name(&JsString::from("<first-party:goal>")),
        Some("goal".to_owned()).map(JsString::from)
    );
    assert_eq!(
        inline_extension_name(&JsString::from("<inline:1>")),
        Some("inline:1".to_owned()).map(JsString::from)
    );
    assert_eq!(inline_extension_name(&JsString::from("/tmp/a.ts")), None);
}

#[tokio::test]
async fn registry_loads_enabled_factories_in_registration_order() {
    let dir = discovery_temp_dir("registry");
    let extensions = dir.join(".pi").join("extensions");
    write(&extensions.join("a.ts"), "export default function() {}");
    write(&extensions.join("b.ts"), "export default function() {}");
    let paths = discover_extension_paths(
        &[],
        &JsString::from(dir.to_str().unwrap()),
        &JsString::from(dir.to_str().unwrap()),
    );
    assert_eq!(paths.len(), 2);

    let mut registry = ExtensionRegistry::new();
    for path in &paths {
        registry.register(path.clone(), factory());
    }
    let result = registry
        .discover_and_load_extensions(
            &[],
            &JsString::from(dir.to_str().unwrap()),
            &JsString::from(dir.to_str().unwrap()),
            None,
            &[JsString::from(
                Path::new(&encode_node_utf8(&paths[1]))
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            )],
        )
        .await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.extensions.len(), 1);
    assert_eq!(
        Path::new(&encode_node_utf8(&result.extensions[0].read().path))
            .file_name()
            .unwrap()
            .to_string_lossy(),
        "a.ts"
    );
    fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn registry_reports_missing_and_failing_factories() {
    let dir = discovery_temp_dir("errors");
    let mut registry = ExtensionRegistry::new();
    let missing = format!("{}/missing.ts", dir.to_string_lossy());
    let result = registry
        .load_extensions(
            std::slice::from_ref(&JsString::from(&missing)),
            &JsString::from(dir.to_str().unwrap()),
            None,
            None,
        )
        .await;
    assert_eq!(result.extensions.len(), 0);
    assert_eq!(
        result.errors[0].error,
        format!("Extension does not export a valid factory function: {missing}")
    );

    let failing_path = dir.join("failing.ts").to_string_lossy().into_owned();
    registry.register(
        failing_path.clone(),
        Arc::new(|_api| Box::pin(async { Err(HandlerError::new("boom")) })),
    );
    let result = registry
        .load_extensions(
            &[JsString::from(failing_path)],
            &JsString::from(dir.to_str().unwrap()),
            None,
            None,
        )
        .await;
    assert_eq!(result.errors[0].error, "Failed to load extension: boom");
    fs::remove_dir_all(dir).ok();
}

#[tokio::test]
async fn registered_extensions_honor_hidden_and_disabled_ids() {
    let mut registry = ExtensionRegistry::new();
    registry.register("<first-party:a>", factory());
    registry.register_hidden("<first-party:b>", factory());
    let runtime = support::runtime();
    let (extensions, errors) = registry
        .load_registered(
            &[JsString::from("<first-party:b>")],
            &JsString::from("/tmp"),
            support::event_bus(),
            runtime,
        )
        .await;
    assert!(errors.is_empty());
    assert_eq!(extensions.len(), 1);
    assert_eq!(extensions[0].read().path, "<first-party:a>");
    assert_eq!(extensions[0].read().hidden, Some(false));
    assert_eq!(extensions[0].read().source_info.source, "first-party");
}

#[tokio::test]
async fn context_session_manager_is_a_live_readonly_view() {
    let temp = tempfile::tempdir().expect("temp dir");
    let cwd = temp.path().display().to_string();
    let session_dir = temp.path().join("sessions").display().to_string();
    let control_db = temp.path().join("control.db").display().to_string();
    let manager = Arc::new(Mutex::new(
        pi_coding_agent::core::session_manager::SessionManager::create(
            &JsString::from(&cwd),
            Some(&JsString::from(&session_dir)),
            None,
        )
        .expect("session manager"),
    ));
    lock_session_manager(&manager)
        .set_metadata_control_db_path(Some(&JsString::from(&control_db)), Default::default())
        .expect("metadata control db");
    let runner = support::runner_for_session(
        &cwd,
        support::runtime(),
        Arc::clone(&manager),
        support::model_registry().await,
    );
    let context = runner.create_context();
    let extension_handle = context.session_manager().expect("session manager");
    assert_eq!(
        extension_handle.get_session_id(),
        lock_session_manager(&manager).get_session_id().clone()
    );
    extension_handle
        .set_session_goal_json(&JsString::from("{\"objective\":\"shared\"}"))
        .expect("extension goal write");
    assert_eq!(
        lock_session_manager(&manager)
            .get_session_goal_json()
            .expect("session goal read"),
        (Some("{\"objective\":\"shared\"}".to_owned())).map(JsString::from)
    );

    lock_session_manager(&manager)
        .append_message(pi_agent_core::agent::AgentMessage::User(pi_ai::types::UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: pi_ai::types::UserContent::Text(JsString::from("appended by session")),
            input_source: None,
            provider_native: None,
            timestamp: 1_700_000_000_000.0,
        }))
        .expect("session append");
    let entries = extension_handle.get_entries();
    assert_eq!(
        entries.len(),
        1,
        "the appended message is visible through the extension handle"
    );
    assert!(matches!(
        entries[0],
        SessionEntry::Known(pi_coding_agent::core::session_manager::KnownSessionEntry::Message(_))
    ));
}

// ============================================================================
// wrapToolDefinition (tool-definition-wrapper.ts)
// ============================================================================

fn text_definition(
    execute: pi_coding_agent::core::extensions::ToolExecute,
) -> pi_coding_agent::core::extensions::ToolDefinition {
    pi_coding_agent::core::extensions::ToolDefinition {
        name: "wrapped".to_owned(),
        label: "Wrapped".into(),
        description: "Wrapped tool".into(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: json!({"type": "object"}),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute,
        render_call: None,
        render_result: None,
    }
}

fn wrapped_text_result(text: String) -> pi_agent_core::agent::AgentToolResult {
    pi_agent_core::agent::AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: None,
        is_error: None,
        terminate: None,
    }
}

#[tokio::test]
async fn wrapped_definitions_inject_execution_identity_and_cap_output() {
    use pi_agent_core::harness::AgentToolExecutionContext;
    use pi_coding_agent::core::extensions::wrapper::wrap_definition_tool;

    let observed_started_at = Arc::new(Mutex::new(None::<f64>));
    let observed_agent_id = Arc::new(Mutex::new(None::<Option<String>>));
    let started_at_hook = Arc::clone(&observed_started_at);
    let agent_id_hook = Arc::clone(&observed_agent_id);
    let definition = text_definition(Arc::new(move |_id, _params, _signal, on_update, context| {
        let started_at_hook = Arc::clone(&started_at_hook);
        let agent_id_hook = Arc::clone(&agent_id_hook);
        Box::pin(async move {
            *started_at_hook.lock().unwrap() = context.as_ref().unwrap().tool_execution_started_at;
            *agent_id_hook.lock().unwrap() = Some(context.as_ref().unwrap().tool_execution_agent_id.clone());
            if let Some(on_update) = on_update {
                on_update(wrapped_text_result("partial".to_owned()))?;
            }
            Ok(wrapped_text_result("x".repeat(50_000)))
        })
    }));
    let mut runner = support::runner_for(Vec::new(), "/tmp", support::runtime(), support::model_registry().await);
    support::bind(&mut runner);
    let tool = wrap_definition_tool(definition, &runner);

    let updates = Arc::new(Mutex::new(Vec::new()));
    let update_hook = Arc::clone(&updates);
    let result = tool
        .execute(
            "call-1".to_owned(),
            JsValue::from(json!({})),
            None,
            Arc::new(move |result| {
                update_hook.lock().unwrap().push(result);
                Ok(())
            }),
            Some(AgentToolExecutionContext {
                agent_id: Some("known-agent".to_owned()),
                started_at: 1234.0,
            }),
        )
        .await
        .expect("wrapped execution");

    assert_eq!(*observed_started_at.lock().unwrap(), Some(1234.0));
    assert_eq!(
        observed_agent_id.lock().unwrap().clone(),
        Some(Some("known-agent".to_owned()))
    );
    assert_eq!(updates.lock().unwrap().len(), 1);
    let details = result.details.expect("capped details");
    let full_output_path = encode_node_utf8(details["fullOutputPath"].as_string().expect("spill path"));
    assert!(
        std::fs::read_to_string(&full_output_path)
            .expect("spill file")
            .starts_with('x')
    );
    let _ = std::fs::remove_file(full_output_path);
}

#[tokio::test]
async fn wrapped_definitions_forward_prepare_arguments_and_prepare_execution() {
    use pi_coding_agent::core::extensions::wrapper::wrap_definition_tool;

    let observed_context_started_at = Arc::new(Mutex::new(None::<f64>));
    let hook = Arc::clone(&observed_context_started_at);
    let mut definition = text_definition(Arc::new(|_id, _params, _signal, _on_update, _context| {
        Box::pin(async { Ok(wrapped_text_result("done".to_owned())) })
    }));
    definition.prepare_arguments = Some(Arc::new(|args: &JsValue| {
        let mut args = args.clone();
        if let JsValue::Object(object) = &mut args {
            object.insert("prepared".into(), JsValue::Bool(true));
        }
        Ok(args)
    }));
    definition.prepare_execution = Some(Arc::new(move |_id, _signal, context| {
        let hook = Arc::clone(&hook);
        Box::pin(async move {
            *hook.lock().unwrap() = context.as_ref().unwrap().tool_execution_started_at;
            Ok(Some(pi_ai::types::ToolCallExecution {
                agent_id: "prepared-agent".to_owned(),
            }))
        })
    }));
    let mut runner = support::runner_for(Vec::new(), "/tmp", support::runtime(), support::model_registry().await);
    support::bind(&mut runner);
    let tool = wrap_definition_tool(definition, &runner);

    assert_eq!(
        tool.prepare_arguments(JsValue::from(json!({"value": 1})))
            .expect("prepared arguments"),
        JsValue::from(json!({"value": 1, "prepared": true}))
    );
    let prepared = tool.prepare_execution().expect("prepare hook")("call-1".to_owned(), None)
        .await
        .expect("prepare execution")
        .expect("execution metadata");
    assert_eq!(prepared.agent_id, "prepared-agent");
    assert_eq!(*observed_context_started_at.lock().unwrap(), None);
}

#[tokio::test]
async fn wrapped_definitions_propagate_prepare_execution_errors() {
    use pi_coding_agent::core::extensions::wrapper::wrap_definition_tool;

    let mut definition = text_definition(Arc::new(|_id, _params, _signal, _on_update, _context| {
        Box::pin(async { Ok(wrapped_text_result("done".to_owned())) })
    }));
    definition.prepare_execution = Some(Arc::new(|_id, _signal, _context| {
        Box::pin(async { Err(HandlerError::new("prepare failed")) })
    }));
    let mut runner = support::runner_for(Vec::new(), "/tmp", support::runtime(), support::model_registry().await);
    support::bind(&mut runner);
    let tool = wrap_definition_tool(definition, &runner);

    let error = tool.prepare_execution().expect("prepare hook")("call-1".to_owned(), None)
        .await
        .expect_err("prepare execution must fail");
    assert_eq!(error.message, "prepare failed");
}

#[tokio::test]
async fn runtime_registration_from_command_is_visible_to_runner() {
    let runtime = support::runtime();
    let extension = support::extension(&runtime, "/tmp", "<runtime-registration>", |api| {
        let live = api.clone();
        api.register_command(
            "install",
            None,
            None,
            command_handler(move |_, _| {
                let live = live.clone();
                Box::pin(async move {
                    live.register_tool(tool_definition("late", "registered after load"))?;
                    live.register_command(
                        "late-command",
                        None,
                        None,
                        command_handler(|_, ctx| {
                            Box::pin(async move {
                                ctx.ui()?.notify(&JsString::from("late command ran"), None)?;
                                Ok(())
                            })
                        }),
                    )?;
                    live.register_flag("late-flag", None, FlagType::Boolean, Some(FlagValue::Boolean(true)))?;
                    live.register_shortcut("ctrl+9".into(), None, shortcut_handler(|_| Box::pin(async { Ok(()) })))?;
                    live.register_message_renderer("late", Arc::new(|_, _, _| None))?;
                    live.register_entry_renderer("late", Arc::new(|_, _, _| None))?;
                    live.register_markdown_transformer(Arc::new(|text, _| format!("{text}!")))?;
                    live.register_tool_gate(Arc::new(|mut event: ToolCallEvent, _: ExtensionContext| {
                        event.input.insert("late_gate".into(), JsValue::from(json!(true)));
                        Box::pin(async move {
                            Ok(Some(ToolCallEventResult {
                                input: Some(event.input),
                                ..Default::default()
                            }))
                        })
                            as BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>>
                    }))?;
                    live.register_approval_reviewer(Arc::new(|_: ToolCallEvent, _: ExtensionContext| {
                        Box::pin(async {
                            Ok(Some(ApprovalReviewerResult::Allow {
                                updated_input: Some(
                                    Map::from_iter([("late_review".into(), json!(true))])
                                        .into_iter()
                                        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                                        .collect::<pi_js_compat::js_json::JsObject>(),
                                ),
                            }))
                        })
                            as BoxFuture<'static, Result<Option<ApprovalReviewerResult>, HandlerError>>
                    }))?;
                    live.on_agent_start(h!(AgentStartEvent, |_event, ctx| {
                        ctx.ui()?.notify(&JsString::from("late handler ran"), None)?;
                        Ok(None)
                    }))?;
                    Ok(())
                })
            }),
        )
        .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![extension], "/tmp", runtime, support::model_registry().await);
    support::bind(&mut runner);
    let command = runner.get_prompt_command("install").unwrap();
    (command.handler)(JsString::default(), runner.create_command_context())
        .await
        .unwrap();
    assert_eq!(runner.get_all_registered_tools()[0].definition.name, "late");
    assert_eq!(
        runner.get_tool_definition("late").unwrap().description,
        "registered after load"
    );
    let command = runner.get_prompt_command("late-command").unwrap();
    (command.handler)(JsString::default(), runner.create_command_context())
        .await
        .unwrap();
    assert!(runner.get_flags().contains_key("late-flag"));
    assert!(runner.get_shortcuts(&Default::default()).contains_key("ctrl+9"));
    assert!(runner.get_message_renderer("late").is_some());
    assert!(runner.get_entry_renderer("late").is_some());
    assert_eq!(runner.get_markdown_transformers().len(), 1);
    assert!(runner.has_handlers("agent_start"));
    runner.emit(&mut ExtensionEvent::AgentStart(AgentStartEvent {})).await;
    assert!(runner.has_tool_gates());
    assert!(runner.has_approval_reviewers());
    let mut event = ToolCallEvent {
        tool_name: ToolName::Bash,
        tool_call_id: "late".into(),
        bypass_permissions: false,
        input: Map::new()
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    };
    runner.emit_tool_gates(&mut event).await.unwrap();
    assert_eq!(event.input["late_gate"], JsValue::Bool(true));
    runner.emit_approval_reviewers(&mut event).await.unwrap();
    assert_eq!(event.input["late_review"], JsValue::Bool(true));
}

#[tokio::test]
async fn handlers_appended_during_dispatch_run_in_the_same_event() {
    let runtime = support::runtime();
    let observed = Arc::new(Mutex::new(Vec::<String>::new()));
    let output = observed.clone();
    let extension = support::extension(&runtime, "/tmp", "<append-handler>", move |api| {
        let live = api.clone();
        api.on_agent_start(move |_: AgentStartEvent, _: ExtensionContext| {
            let live = live.clone();
            let output = output.clone();
            Box::pin(async move {
                output.lock().unwrap().push("first".into());
                live.on_agent_start(move |_: AgentStartEvent, _: ExtensionContext| {
                    let output = output.clone();
                    Box::pin(async move {
                        output.lock().unwrap().push("appended".into());
                        Ok(None)
                    }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
                })?;
                Ok(None)
            }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
        })
        .unwrap();
    })
    .await;
    let mut runner = support::runner_for(vec![extension], "/tmp", runtime, support::model_registry().await);
    support::bind(&mut runner);
    runner.emit(&mut ExtensionEvent::AgentStart(AgentStartEvent {})).await;
    assert_eq!(*observed.lock().unwrap(), ["first", "appended"]);
}
