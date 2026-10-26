use super::*;
use crate::core::{
    auth_storage::AuthStorage,
    extensions::{ExtensionRegistry, ExtensionRunner},
    model_registry::ModelRegistry,
    session_manager::SessionManager,
};
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};

fn read_debug_for_module(module: &str) -> String {
    std::fs::read_to_string(logging::debug_path().unwrap())
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains(&format!("[{module}] ")))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn claude_bridge_ask_auth_failure_writes_the_exact_source_trace_record() {
    use std::os::unix::fs::MetadataExt;
    let (dir, _, _) = integration_tests::setup().await;
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::write(
        home.join(".claude/ask-auth-trace.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
    )
    .unwrap();
    let mut bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    bridge.logs.as_mut().unwrap().enabled = true;
    let module = bridge.logs.as_ref().unwrap().module_id.clone();
    *bridge.accounts.lock().unwrap() = Ok(Some(
        account_state::AccountManager::new(home.clone(), vec!["ask-auth-trace".into()]).unwrap(),
    ));
    let mut registry = ExtensionRegistry::new();
    registry.register_hidden(
        "<fixture:ask-auth-trace>",
        Arc::new(move |pi| {
            let result = ask_claude::register(
                pi,
                &js_json::parse_lossless(r#"{"enabled":true}"#).unwrap(),
                bridge.clone(),
            );
            Box::pin(async { result })
        }),
    );
    let loaded = registry
        .load_extensions(
            &["<fixture:ask-auth-trace>".into()],
            &dir.path().to_str().unwrap().into(),
            None,
            None,
        )
        .await
        .unwrap();
    let definition = loaded.extensions[0].read().tools["AskClaude"].definition.clone();
    let runner = lifecycle_tests::runner(dir.path(), loaded, None);
    let result = (definition.execute)(
        "ask".into(),
        js_json::parse_lossless(r#"{"prompt":"ask-auth-failure","mode":"none","isolated":true}"#).unwrap(),
        None,
        None,
        Some(runner.create_context()),
    )
    .await
    .unwrap();
    assert_eq!(
        result.details.as_ref().unwrap().get("error"),
        Some(&JsValue::Bool(true))
    );
    let text = std::fs::read_to_string(logging::agent_dir().unwrap().join("claude-bridge-auth.jsonl")).unwrap();
    let (line, entry) = text
        .lines()
        .find_map(|line| {
            let row = js_json::parse_lossless(line).unwrap();
            (stream::string(&row, "profile") == "ask-auth-trace" && stream::string(&row, "event") == "oauth_failure")
                .then_some((line, row))
        })
        .expect("AskClaude authentication failure trace");
    let file = |path: &std::path::Path| {
        let stat = std::fs::metadata(path).unwrap();
        JsValue::object([
            ("state", JsValue::String("present".into())),
            ("accessTokenPresent", JsValue::Bool(true)),
            ("refreshTokenPresent", JsValue::Bool(false)),
            ("accessFingerprint", JsValue::String("f16d05ec6b29248d".into())),
            (
                "modifiedAt",
                JsValue::Number(stat.mtime() as f64 * 1000.0 + stat.mtime_nsec() as f64 / 1_000_000.0),
            ),
            ("inode", JsValue::Number(stat.ino() as f64)),
        ])
    };
    let expected = JsValue::object([
        ("timestamp", entry.get("timestamp").unwrap().clone()),
        ("pid", JsValue::Number(std::process::id().into())),
        ("profile", JsValue::String("ask-auth-trace".into())),
        ("event", JsValue::String("oauth_failure".into())),
        (
            "failure",
            JsValue::object([
                ("kind", JsValue::String("invalid_grant".into())),
                ("httpStatus", JsValue::Number(401.0)),
            ]),
        ),
        (
            "files",
            JsValue::object([
                ("named", file(&home.join(".claude/ask-auth-trace.credentials.json"))),
                (
                    "isolated",
                    file(&home.join(".config/pi-claude-bridge/profiles/ask-auth-trace/.credentials.json")),
                ),
                (
                    "active",
                    JsValue::object([("state", JsValue::String("unassociated".into()))]),
                ),
            ]),
        ),
    ]);
    assert_eq!(line, js_json::stringify_lossless(&expected));
    let result = (definition.execute)(
        "ask-usage".into(),
        js_json::parse_lossless(r#"{"prompt":"ask-null-usage","mode":"none","isolated":true}"#).unwrap(),
        None,
        None,
        Some(runner.create_context()),
    )
    .await
    .unwrap();
    assert_ne!(
        result.details.as_ref().unwrap().get("error"),
        Some(&JsValue::Bool(true))
    );
    let debug = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
    assert!(debug.contains(&format!(
        "[{module}] askClaude: result usage: in=7 out=2 cacheRead=0 cacheWrite=0 turns=?\n"
    )));
}

#[tokio::test]
async fn claude_bridge_ask_diagnostic_error_and_circular_logs() {
    for circular in [false, true] {
        let (dir, config, _) = integration_tests::setup().await;
        let mut bridge =
            provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
        bridge.logs.as_mut().unwrap().enabled = true;
        let module = bridge.logs.as_ref().unwrap().module_id.clone();
        let mut registry = ExtensionRegistry::new();
        registry.register_hidden(
            "<fixture:ask-diag>",
            Arc::new(move |pi| {
                let result = ask_claude::register(
                    pi,
                    &js_json::parse_lossless(r#"{"enabled":true}"#).unwrap(),
                    bridge.clone(),
                );
                Box::pin(async { result })
            }),
        );
        let loaded = registry
            .load_extensions(
                &["<fixture:ask-diag>".into()],
                &dir.path().to_str().unwrap().into(),
                None,
                None,
            )
            .await
            .unwrap();
        let definition = loaded.extensions[0].read().tools["AskClaude"].definition.clone();
        let model = circular.then(|| integration_tests::model(&config));
        let runner = lifecycle_tests::runner(dir.path(), loaded, model);
        let result = (definition.execute)(
            "ask".into(),
            js_json::parse_lossless(r#"{"prompt":"error","mode":"none","isolated":true}"#).unwrap(),
            None,
            None,
            Some(runner.create_context()),
        )
        .await
        .unwrap();
        assert_eq!(
            result.details.as_ref().unwrap().get("error"),
            Some(&JsValue::Bool(true))
        );
        let log = read_debug_for_module(&module);
        if circular {
            assert!(
                log.lines()
                    .any(|line| line
                        .ends_with("askClaude: blocked circular delegation (active provider is claude-bridge)")),
                "{log}"
            );
        } else {
            let message = log
                .lines()
                .find_map(|line| line.split_once("askClaude error: ").map(|(_, message)| message))
                .expect("AskClaude error diagnostic");
            let (prefix, tail) = message.split_once("elapsed=").unwrap();
            assert_eq!(prefix, "mode=none, model=default, isolated=true, ");
            let (elapsed, error) = tail.split_once("s, error= ").unwrap();
            assert_eq!(format!("{:.1}", elapsed.parse::<f64>().unwrap()), elapsed);
            assert_eq!(error, "Error: Quota �");
        }
    }
}

#[tokio::test]
async fn claude_bridge_prompt_capture_diagnostic_logs() {
    let (dir, config, _) = integration_tests::setup().await;
    let mut bridge = provider::BridgeProvider::new(JsValue::Null, Ok(dir.path().join("claude")), "AskClaude".into());
    bridge.logs.as_mut().unwrap().enabled = true;
    bridge.captures = Arc::new(Mutex::new(prompt_capture::PromptCaptures::default()));
    bridge.captures.lock().unwrap().diagnose_logs = Some(bridge.logs.clone());
    let prompt = JsString::from_utf16(vec![0xdfff, 65, 66]);
    for (known, enabled) in [(false, true), (true, true), (true, false)] {
        bridge.logs.as_mut().unwrap().enabled = enabled;
        bridge.captures.lock().unwrap().diagnose_logs = Some(bridge.logs.clone());
        let before = read_debug_for_module(&bridge.logs.as_ref().unwrap().module_id);
        if known {
            bridge.captures.lock().unwrap().record(
                &JsString::from_utf16(vec![0xdfff, 65, 67]),
                &Default::default(),
                "before_agent_start",
            );
        }
        let context = pi_ai::types::Context {
            system_prompt: Some(prompt.clone()),
            tools: None,
            messages: Vec::new(),
        };
        let mut options = pi_ai::request::SimpleStreamOptions::default();
        options.stream.cwd = Some(dir.path().to_str().unwrap().into());
        let result = bridge
            .stream(integration_tests::model(&config), context, Some(options))
            .unwrap()
            .result()
            .await
            .unwrap();
        assert_eq!(result.stop_reason, pi_ai::types::StopReason::Error);
        let log = read_debug_for_module(&bridge.logs.as_ref().unwrap().module_id);
        let expected = if known {
            r#"prompt-capture: no match for 3-char system prompt. closest known (3-char) shares its first 2 chars and diverges at offset 2: "\udfffAB" known keys=1"#
        } else {
            "prompt-capture: no match for 3-char system prompt. no known captures to compare against. known keys=0"
        };
        if enabled {
            assert!(
                !log.contains("cli-debug:"),
                "no SDK query is constructed on capture rejection"
            );
            assert!(
                !log.contains("provider: query error,"),
                "capture rejection is not an SDK query failure"
            );
            assert!(log.lines().any(|line| line.ends_with(expected)), "{log}");
        } else {
            assert_eq!(log, before);
        }
        let diag = std::fs::read_to_string(logging::agent_dir().unwrap().join("claude-bridge-diag.log")).unwrap();
        let (line, entry) = diag
            .lines()
            .rev()
            .find_map(|line| {
                let entry = js_json::parse_lossless(line).unwrap();
                (stream::string(&entry, "moduleInstanceId") == bridge.logs.as_ref().unwrap().module_id.as_str()
                    && stream::string(&entry, "label") == "prompt_capture_unresolved")
                    .then_some((line, entry))
            })
            .unwrap();
        let ts = entry.get("ts").unwrap().clone();
        let expected = JsValue::object([
            ("ts", ts),
            (
                "moduleInstanceId",
                JsValue::String(bridge.logs.as_ref().unwrap().module_id.clone().into()),
            ),
            ("label", JsValue::String("prompt_capture_unresolved".into())),
            ("promptChars", JsValue::Number(3.0)),
            ("knownKeys", JsValue::Number(if known { 1.0 } else { 0.0 })),
            ("reentrantUserQuery", JsValue::Bool(false)),
            ("error", JsValue::String(result.error_message.unwrap())),
        ]);
        assert_eq!(line, js_json::stringify_lossless(&expected));
    }
}

#[tokio::test]
async fn claude_bridge_askclaude_shared_history_imports_once_and_resumes_native_session() {
    for existing in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("claude");
        std::fs::write(&executable, include_str!("fixtures/claude.py")).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut bridge = provider::BridgeProvider::new(JsValue::Null, Ok(executable), "AskClaude".into());
        bridge.logs.as_mut().unwrap().enabled = true;
        *bridge.sessions.lock().unwrap() = Ok(sessions::SessionStore::at(
            dir.path().join("cc"),
            dir.path().join("agent"),
        ));
        let cwd: JsString = dir.path().to_str().unwrap().into();
        let mut session = SessionManager::in_memory(Some(&cwd), None).unwrap();
        let user = js_json::from_value(
            &js_json::parse_lossless(r#"{"role":"user","content":"prior question \udfff","timestamp":1}"#).unwrap(),
        )
        .unwrap();
        session.append_message(user).unwrap();
        let assistant = js_json::from_value(&js_json::parse_lossless(r#"{"role":"assistant","api":"test","provider":"test","model":"test","content":[{"type":"text","text":"prior answer"}],"stopReason":"stop","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"timestamp":2}"#).unwrap()).unwrap();
        session.append_message(assistant).unwrap();
        if existing {
            bridge
                .sessions
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .record(session.get_session_id(), "existing-native".into(), 2, dir.path())
                .unwrap();
        }
        let mut registry = ExtensionRegistry::new();
        let factory_bridge = bridge.clone();
        registry.register_hidden(
            "<fixture:ask-history>",
            Arc::new(move |pi| {
                let result = ask_claude::register(
                    pi,
                    &js_json::parse_lossless(r#"{"enabled":true}"#).unwrap(),
                    factory_bridge.clone(),
                );
                Box::pin(async { result })
            }),
        );
        let loaded = registry
            .load_extensions(&["<fixture:ask-history>".into()], &cwd, None, None)
            .await
            .unwrap();
        assert!(loaded.errors.is_empty());
        let definition = loaded.extensions[0].read().tools["AskClaude"].definition.clone();
        let models =
            Arc::new(ModelRegistry::in_memory(AuthStorage::in_memory(Vec::<(JsString, JsValue)>::new())).unwrap());
        let runner = ExtensionRunner::new(
            loaded.extensions,
            loaded.runtime,
            cwd,
            Arc::new(Mutex::new(session)),
            models,
            None,
        );
        let prompt = if existing {
            "assert-ask-existing"
        } else {
            "assert-ask-history"
        };
        let args = JsValue::object([
            ("prompt", JsValue::String(prompt.into())),
            ("mode", JsValue::String("none".into())),
        ]);
        let result = (definition.execute)("ask".into(), args, None, None, Some(runner.create_context()))
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
        let pi_ai::types::TextOrImageContent::Text(text) = &result.content[0] else {
            panic!("answer");
        };
        assert_eq!(text.text, "shared history restored");
        let log = std::fs::read_to_string(logging::debug_path().unwrap()).unwrap();
        let module = &bridge.logs.as_ref().unwrap().module_id;
        let messages = log
            .lines()
            .filter_map(|line| line.split_once(&format!("[{module}] ")).map(|(_, message)| message))
            .collect::<Vec<_>>();
        assert!(messages.iter().any(
            |message| message.starts_with("askClaude: mode=none model=opus cliModel=")
                && message.ends_with(&format!(" skills=false promptLen={}", prompt.len()))
        ));
        assert!(messages.iter().any(|message| {
            message.starts_with("askClaude: done stopReason=stop resultSubtype=success sdkMessages=")
                && message.ends_with(" responseLen=23 toolCalls=0")
        }));
    }
}
