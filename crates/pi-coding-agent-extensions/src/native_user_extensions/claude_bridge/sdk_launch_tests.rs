//! Offline SDK 0.3.293 process captures; see docs/porting/bridge-sdk-proof.md.
use super::{integration_tests::setup, process::ClaudeProcess, provider::BridgeProvider, *};
use pi_ai::{
    request::SimpleStreamOptions,
    types::{Context, ModelThinkingLevel, ThinkingLevel, Tool},
};
use pi_js_compat::js_json::{self, JsValue};

async fn capture(launch: &process::LaunchOptions) -> JsValue {
    std::fs::write(launch.cwd.join(".fixture-record-launch"), "").unwrap();
    let mut child = ClaudeProcess::spawn(launch).await.unwrap();
    child
        .write(&process::user_message(JsValue::String("record".into()), false))
        .await
        .unwrap();
    while let Some(frame) = child.read().await.unwrap() {
        if stream::string(&frame, "type") == "result" {
            break;
        }
    }
    child.close().await.unwrap();
    js_json::parse_lossless(std::fs::read_to_string(launch.cwd.join(".fixture-launch.json")).unwrap()).unwrap()
}

#[tokio::test]
async fn claude_bridge_sdk_launch_matches_offline_sdk_captures() {
    for (mode, golden) in [
        ("provider", include_str!("fixtures/sdk-launch-provider.json")),
        ("tools", include_str!("fixtures/sdk-launch-tools.json")),
        ("ask", include_str!("fixtures/sdk-launch-ask.json")),
        ("summary", include_str!("fixtures/sdk-launch-summary.json")),
        ("summary-empty", include_str!("fixtures/sdk-launch-summary-empty.json")),
        ("debug", include_str!("fixtures/sdk-launch-debug.json")),
        ("configured", include_str!("fixtures/sdk-launch-configured.json")),
        ("ask-read", include_str!("fixtures/sdk-launch-ask-read.json")),
        ("ask-none", include_str!("fixtures/sdk-launch-ask-none.json")),
    ] {
        let (dir, config, _) = setup().await;
        let bridge = BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
        let mut context = Context {
            system_prompt: None,
            tools: None,
            messages: Vec::new(),
        };
        if mode == "tools" {
            context.tools = Some(vec![Tool {
                name: "edit".into(),
                description: "Edit text".into(),
                parameters: serde_json::json!({"type":"object","properties":{"text":{"type":"string"}}}),
                label: None,
                execution_mode: None,
                typebox_kind: false,
            }]);
        }
        let mut launch = bridge
            .launch_options(
                &integration_tests::model(&config),
                &context,
                &SimpleStreamOptions::default(),
                dir.path().into(),
                "provider",
            )
            .unwrap();
        launch.append = Some(
            js_json::parse_lossless(r#""append \ud800""#)
                .unwrap()
                .as_string()
                .unwrap()
                .clone(),
        );
        launch.resume = Some("existing".into());
        launch.effort = Some("xhigh".into());
        launch.claude_dir = Some(dir.path().join("profile"));
        if mode.starts_with("ask") {
            launch.ask_mode = Some(
                match mode {
                    "ask-read" => "read",
                    "ask-none" => "none",
                    _ => "full",
                }
                .into(),
            );
            launch.effort = Some("max".into());
            launch.isolated = true;
        } else if mode.starts_with("summary") {
            launch.summary = true;
            launch.summary_system_prompt = Some(
                js_json::parse_lossless(r#""summary \udfff""#)
                    .unwrap()
                    .as_string()
                    .unwrap()
                    .clone(),
            );
            launch.append = None;
            launch.resume = None;
            launch.effort = None;
            launch.isolated = true;
        }
        if mode == "summary-empty" {
            launch.summary_system_prompt = None;
        }
        if mode == "configured" {
            launch.model = "-custom-model".into();
            launch.provider = js_json::parse_lossless(r#"{"autoMemoryEnabled":true,"strictMcpConfig":false}"#).unwrap();
            launch.effort = None;
            launch.resume = None;
            launch.append = None;
        }
        if mode == "debug" {
            launch.cli_debug.as_mut().unwrap().debug_file = Some(dir.path().join("debug.log"));
        }
        let mut actual = capture(&launch).await;
        if mode == "debug" {
            let argv = actual
                .as_object_mut()
                .unwrap()
                .get_mut("argv")
                .unwrap()
                .as_array_mut()
                .unwrap();
            let index = argv
                .iter()
                .position(|v| v.as_string().is_some_and(|s| s == "--debug-file"))
                .unwrap();
            assert_eq!(
                argv[index + 1].as_string().unwrap(),
                dir.path().join("debug.log").to_str().unwrap()
            );
            argv[index + 1] = JsValue::String("<debug-file>".into());
        }
        let mut expected = js_json::parse_lossless(golden).unwrap();
        expected
            .as_object_mut()
            .unwrap()
            .insert("cwd".into(), JsValue::String(dir.path().to_str().unwrap().into()));
        let init = actual.as_object_mut().unwrap().get_mut("initialize").unwrap();
        init.as_object_mut()
            .unwrap()
            .insert("request_id".into(), JsValue::String("<request-id>".into()));
        // SDK inherits all ambient variables, changes these entries, and removes NODE_OPTIONS/DEBUG.
        let mut env: serde_json::Map<String, serde_json::Value> =
            std::env::vars().map(|(k, v)| (k, v.into())).collect();
        if env.get("CLAUDE_CODE_ENTRYPOINT").is_none_or(|v| v == "") {
            env.insert("CLAUDE_CODE_ENTRYPOINT".into(), "sdk-ts".into());
        }
        if env.get("CLAUDE_AGENT_SDK_VERSION").is_none_or(|v| v == "") {
            env.insert("CLAUDE_AGENT_SDK_VERSION".into(), "0.3.293".into());
        }
        if !env
            .keys()
            .any(|key| key.eq_ignore_ascii_case("CLAUDE_CODE_SDK_READS_SESSION_STATE"))
        {
            env.insert("CLAUDE_CODE_SDK_READS_SESSION_STATE".into(), "1".into());
        }
        env.insert("ENABLE_CLAUDEAI_MCP_SERVERS".into(), "0".into());
        env.insert("DISABLE_AUTO_COMPACT".into(), "1".into());
        env.insert(
            "CLAUDE_CONFIG_DIR".into(),
            launch.claude_dir.as_ref().unwrap().to_str().unwrap().into(),
        );
        env.remove("NODE_OPTIONS");
        env.remove("DEBUG");
        if std::env::var("DEBUG_CLAUDE_AGENT_SDK")
            .is_ok_and(|v| ["1", "true", "yes", "on"].contains(&v.trim().to_lowercase().as_str()))
        {
            env.insert("DEBUG".into(), "1".into());
        }
        expected
            .as_object_mut()
            .unwrap()
            .insert("env".into(), js_json::to_value(&env).unwrap());
        // Compare the full environment separately, without including secrets in a failure diff.
        assert!(
            actual.get("env") == expected.get("env"),
            "{mode}: child environment differs from SDK"
        );
        actual.as_object_mut().unwrap().shift_remove("env");
        expected.as_object_mut().unwrap().shift_remove("env");
        assert_eq!(actual, expected, "{mode}");
    }
}

#[tokio::test]
async fn claude_bridge_sdk_askclaude_thinking_overrides_reach_child() {
    use crate::core::{
        auth_storage::AuthStorage, extensions::ExtensionRunner, model_registry::ModelRegistry,
        session_manager::SessionManager,
    };
    use std::sync::{Arc, Mutex};
    let (dir, _, loaded) = setup().await;
    std::fs::write(dir.path().join(".fixture-record-launch"), "").unwrap();
    let definition = loaded.extensions[0].read().tools["AskClaude"].definition.clone();
    let cwd = pi_js_compat::js_json::JsString::from(dir.path().to_str().unwrap());
    let models = Arc::new(
        ModelRegistry::in_memory(AuthStorage::in_memory(
            Vec::<(pi_js_compat::js_json::JsString, JsValue)>::new(),
        ))
        .unwrap(),
    );
    let session = SessionManager::in_memory(Some(&cwd), None).unwrap();
    let runner = ExtensionRunner::new(
        loaded.extensions,
        loaded.runtime,
        cwd,
        Arc::new(Mutex::new(session)),
        models,
        None,
    );
    for (thinking, expected) in [
        (None, None),
        (Some("off"), None),
        (Some("minimal"), Some("low")),
        (Some("low"), Some("low")),
        (Some("medium"), Some("medium")),
        (Some("high"), Some("high")),
        (Some("xhigh"), Some("max")),
    ] {
        let mut args = JsValue::object([
            ("prompt", JsValue::String("record".into())),
            ("mode", JsValue::String("none".into())),
            ("isolated", JsValue::Bool(true)),
            ("model", JsValue::String("claude-opus-5-5".into())),
        ]);
        if let Some(thinking) = thinking {
            args.as_object_mut()
                .unwrap()
                .insert("thinking".into(), JsValue::String(thinking.into()));
        }
        let result = (definition.execute)("ask-sdk".into(), args, None, None, Some(runner.create_context()))
            .await
            .unwrap();
        assert_ne!(
            result
                .details
                .as_ref()
                .and_then(|d| d.get("error"))
                .and_then(JsValue::as_bool),
            Some(true),
            "{:?}",
            result.content
        );
        let record =
            js_json::parse_lossless(std::fs::read_to_string(dir.path().join(".fixture-launch.json")).unwrap()).unwrap();
        let argv = record.get("argv").unwrap().as_array().unwrap();
        let effort = argv
            .iter()
            .position(|v| v.as_string().is_some_and(|s| s == "--effort"))
            .map(|i| argv[i + 1].as_string().unwrap().to_utf8().unwrap());
        assert_eq!(effort.as_deref(), expected, "{thinking:?}");
        assert_eq!(
            argv.iter()
                .any(|v| v.as_string().is_some_and(|s| s == "--thinking-display")),
            expected.is_some()
        );
    }
}

#[tokio::test]
async fn claude_bridge_sdk_runs_configured_js_cli_with_node() {
    let (dir, config, _) = setup().await;
    let script = dir.path().join("claude.mjs");
    let fixture = serde_json::to_string(dir.path().join("claude").to_str().unwrap()).unwrap();
    std::fs::write(&script, format!("import {{spawnSync}} from 'node:child_process';\nconst child=spawnSync('/usr/bin/python3', [{fixture}, ...process.argv.slice(2)], {{stdio:'inherit'}});\nprocess.exit(child.status ?? 1);\n")).unwrap();
    let settings = js_json::parse_lossless(format!(
        r#"{{"pathToClaudeCodeExecutable":{}}}"#,
        serde_json::to_string(script.to_str().unwrap()).unwrap()
    ))
    .unwrap();
    let executable = provider::executable_path(&settings, dir.path()).unwrap();
    let bridge = BridgeProvider::new(settings, Ok(executable), "AskClaude".into());
    let launch = bridge
        .launch_options(
            &integration_tests::model(&config),
            &Context {
                system_prompt: None,
                tools: None,
                messages: Vec::new(),
            },
            &SimpleStreamOptions::default(),
            dir.path().into(),
            "provider",
        )
        .unwrap();
    let actual = capture(&launch).await;
    assert_eq!(
        actual.get("cwd").unwrap().as_string().unwrap(),
        dir.path().to_str().unwrap()
    );
    let expected = js_json::to_value(&vec!["--output-format", "stream-json", "--verbose", "--input-format", "stream-json", "--tools", "WebSearch", "--permission-mode", "bypassPermissions", "--include-partial-messages", "--model", "claude-opus-5-5[1m]", "--strict-mcp-config", "--settings", r#"{"autoMemoryEnabled":false,"claudeMdExcludes":["**/CLAUDE.md","**/AGENTS.md","**/.claude/rules/**"],"includeGitInstructions":false}"#]).unwrap();
    assert_eq!(actual.get("argv").unwrap(), &expected);
}

#[tokio::test]
async fn claude_bridge_sdk_effort_honors_model_overrides_and_undefined_levels() {
    use ThinkingLevel::{High, Max, Minimal, Ultra, Xhigh};
    for (level, mapping, expected) in [
        (Xhigh, Some(Some("xhigh")), Some("xhigh")),
        (Xhigh, Some(None), None),
        (Xhigh, Some(Some("unsupported")), None),
        (Xhigh, Some(Some("\u{1f600}")), None),
        (Xhigh, None, Some("max")),
        (Minimal, None, Some("low")),
        (High, None, Some("high")),
        (Max, None, None),
        (Ultra, None, None),
    ] {
        let (dir, config, _) = setup().await;
        let bridge = BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
        let mut model = integration_tests::model(&config);
        model.thinking_level_map = mapping.map(|value| {
            [(ModelThinkingLevel::Xhigh, value.map(Into::into))]
                .into_iter()
                .collect()
        });
        let mut options = SimpleStreamOptions {
            reasoning: Some(level),
            ..Default::default()
        };
        options.stream.cwd = Some(dir.path().to_str().unwrap().into());
        std::fs::write(dir.path().join(".fixture-record-launch"), "").unwrap();
        let result = bridge
            .stream(
                model,
                Context {
                    system_prompt: None,
                    tools: None,
                    messages: vec![pi_ai::types::Message::User(pi_ai::types::UserMessage {
                        role: pi_ai::types::UserRole::Tag,
                        content: pi_ai::types::UserContent::Text("record".into()),
                        input_source: None,
                        provider_native: None,
                        timestamp: 1.0,
                    })],
                },
                Some(options),
            )
            .unwrap()
            .result()
            .await
            .unwrap();
        assert_eq!(
            result.stop_reason,
            pi_ai::types::StopReason::Stop,
            "{:?}",
            result.error_message
        );
        let actual =
            js_json::parse_lossless(std::fs::read_to_string(dir.path().join(".fixture-launch.json")).unwrap()).unwrap();
        let argv = actual.get("argv").unwrap().as_array().unwrap();
        let effort = argv
            .iter()
            .position(|v| v.as_string().is_some_and(|s| s == "--effort"))
            .map(|i| argv[i + 1].as_string().unwrap().to_utf8().unwrap());
        assert_eq!(effort.as_deref(), expected, "{level:?}");
        assert_eq!(
            argv.iter()
                .any(|v| v.as_string().is_some_and(|s| s == "--thinking-display")),
            expected.is_some()
        );
    }
}
