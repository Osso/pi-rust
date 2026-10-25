#[path = "support/oracle.rs"]
mod oracle;

use pi_agent_core::agent::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::tools::tool_definition_wrapper::{ToolOutputSpill, cap_tool_result_output};
use pi_js_compat::js_json::{self as json, JsString, JsValue, encode_node_utf8};

#[test]
fn result_cap_matches_frozen_for_json_details_and_raw_text() {
    let mut oracle = oracle::TsOracle::spawn(
        "tool-definition-wrapper-oracle.ts",
        "packages/coding-agent/src/core/tools/tool-definition-wrapper.ts",
    );
    let text = JsString::from("line\n".repeat(2500)) + JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
    for details in [
        pi_js_compat::js_json!(["first", {"raw": JsString::from_utf16(vec![0xd800])}]),
        pi_js_compat::js_json!(42),
        pi_js_compat::js_json!({"truncation": null}),
        pi_js_compat::js_json!({"keep": true}),
    ] {
        let result = AgentToolResult {
            content: vec![TextOrImageContent::Text(TextContent::new(text.clone()))],
            details: Some(details),
            is_error: None,
            terminate: None,
        };
        let expected = oracle.request(&pi_js_compat::js_json!({"id": 1, "result": result}));
        assert_eq!(expected["ok"], pi_js_compat::js_json!(true), "{expected:?}");
        let mut spill = ToolOutputSpill::default();
        let mut actual = cap_tool_result_output(result, &mut spill).expect("cap");
        let spilled = spill.full_output_path.as_ref().map(|path| {
            let bytes = std::fs::read(encode_node_utf8(path)).expect("spill bytes");
            std::fs::remove_file(encode_node_utf8(path)).expect("remove spill");
            if let TextOrImageContent::Text(content) = &mut actual.content[0] {
                content.text = content.text.replace(path, "<spill>");
            }
            actual
                .details
                .as_mut()
                .expect("details")
                .as_object_mut()
                .expect("object")
                .insert("fullOutputPath".into(), pi_js_compat::js_json!("<spill>"));
            bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        });
        assert_eq!(json::to_value(&actual).unwrap(), expected["output"]["result"]);
        assert_eq!(
            spilled.map(|value| JsValue::String(value.into())),
            expected["output"].as_object().unwrap().get("spilled").cloned()
        );
    }
}

#[path = "support/env_probe.rs"]
mod env_probe;
#[path = "support/tool_context.rs"]
mod tool_context;

#[test]
fn update_spill_failure_is_synchronous_and_catchable() {
    let temp = tempfile::tempdir().unwrap();
    let blocked = temp.path().join("not-a-directory");
    std::fs::write(&blocked, "blocked").unwrap();
    env_probe::run_probe(
        "spill_failure_probe",
        &[
            ("TMPDIR", blocked.into_os_string()),
            ("PI_OFFLINE", "1".into()),
            ("TSX_DISABLE_CACHE", "1".into()),
        ],
    );
}

#[test]
#[ignore = "child process probe"]
fn spill_failure_probe() {
    if !env_probe::in_probe() {
        return;
    }
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        use std::sync::{Arc, Mutex};
        use pi_coding_agent::core::tools::tool_definition_wrapper::{wrap_definition, create_tool_definition_from_agent_tool};
        let session = tool_context::in_memory_session("/tmp", "wrapper");
        let (_runner, context) = tool_context::TestContextBuilder::new("/tmp", session).build().await;
        let blocked = std::env::temp_dir();
        let mut oracle = oracle::TsOracle::spawn_with_environment("tool-definition-wrapper-oracle.ts", "packages/coding-agent/src/core/tools/tool-definition-wrapper.ts", &[("TMPDIR", blocked.parent().unwrap().as_os_str().to_owned())]);
        let output_log_pattern = regex::Regex::new("pi-tool-[0-9a-f]{16}\\.log").unwrap();
        for catches in [true, false] {
            let expected = oracle.request(&pi_js_compat::js_json!({"id": 2, "operation": "spill-failure", "catch": catches, "tmpdir": blocked.to_str().unwrap()}));
            assert_eq!(expected["ok"], pi_js_compat::js_json!(true), "{expected:?}");
            let trace = Arc::new(Mutex::new(Vec::<String>::new()));
            let captured = Arc::clone(&trace);
            let mut definition = create_tool_definition_from_agent_tool(pi_coding_agent::core::tools::read::create_read_tool(JsString::from("/tmp"), Default::default()));
            definition.execute = Arc::new(move |_, _, _, update, _| {
                let captured = Arc::clone(&captured);
                Box::pin(async move {
                    captured.lock().unwrap().push("before".into());
                    let outcome = update.unwrap()(AgentToolResult { content: vec![TextOrImageContent::Text(TextContent::new("line\n".repeat(2500)))], details: None, is_error: None, terminate: None });
                    if catches { return Ok(AgentToolResult { content: vec![TextOrImageContent::Text(TextContent::new("caught"))], details: None, is_error: None, terminate: None }); }
                    outcome.map_err(|error| pi_coding_agent::core::extensions::HandlerError::new(error.message))?;
                    captured.lock().unwrap().push("after".into());
                    Ok(AgentToolResult { content: vec![TextOrImageContent::Text(TextContent::new("after"))], details: None, is_error: None, terminate: None })
                })
            });
            let context = context.clone();
            let tool = wrap_definition(definition, Some(Arc::new(move || context.clone())));
            let actual = tool.execute("call".into(), pi_js_compat::js_json!({}), None, Arc::new(|_| Ok(())), None).await;
            let mut actual = match actual { Ok(result) => pi_js_compat::js_json!({"result": result}), Err(error) => {
                let message = encode_node_utf8(&error.message);
                let normalized = output_log_pattern.replace_all(&message, "pi-tool-<id>.log").into_owned();
                pi_js_compat::js_json!({"error": normalized})
            }};
            actual.as_object_mut().unwrap().insert("trace".into(), json::to_value(&*trace.lock().unwrap()).unwrap());
            assert_eq!(actual, expected["output"], "catch={catches}");
        }
    });
}

#[test]
fn timer_update_failure_uses_the_uncaught_process_boundary() {
    use std::process::Command;
    let lock: serde_json::Value = serde_json::from_str(include_str!("../../../docs/porting/source-lock.json")).unwrap();
    let source = std::path::Path::new(lock["worktree"].as_str().unwrap());
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let expected = Command::new(source.join("node_modules/.bin/tsx"))
        .arg("--tsconfig")
        .arg(source.join("tsconfig.json"))
        .arg(repo.join("oracle/bash-update-error-process-oracle.mts"))
        .arg(source.join("packages/coding-agent/src/core/tools/bash.ts"))
        .output()
        .unwrap();
    assert_eq!(
        expected.status.code(),
        Some(1),
        "{}",
        String::from_utf8(expected.stderr.clone()).unwrap()
    );
    let expected_stderr = String::from_utf8(expected.stderr.clone()).unwrap();
    assert!(
        expected_stderr.lines().any(|line| line == "Error: timer update failed"),
        "{expected_stderr}"
    );
    let actual = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "timer_update_failure_probe", "--nocapture"])
        .env("PI_Q5_TIMER_PROBE", "1")
        .output()
        .unwrap();
    assert_eq!(actual.status.code(), expected.status.code());
    for output in [expected, actual] {
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.lines().any(|line| line == "Error: timer update failed"),
            "{stderr}"
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stdout.contains("caught by execute"), "{stdout}");
        assert!(!stdout.contains("completed"), "{stdout}");
    }
}

#[tokio::test]
#[ignore = "fatal process probe"]
async fn timer_update_failure_probe() {
    if std::env::var_os("PI_Q5_TIMER_PROBE").is_none() {
        return;
    }
    use pi_agent_core::agent::ToolError;
    use pi_coding_agent::core::tools::bash::{
        BashExecOptions, BashExecResult, BashOperations, BashToolOptions, create_bash_tool,
    };
    use std::sync::Arc;
    struct ScriptedBash;
    impl BashOperations for ScriptedBash {
        fn exec<'a>(
            &'a self,
            _: &'a JsString,
            _: &'a JsString,
            options: BashExecOptions,
        ) -> pi_ai::request::BoxFuture<'a, Result<BashExecResult, ToolError>> {
            Box::pin(async move {
                (options.on_data)(b"first\n").map_err(|error| pi_agent_core::agent::ToolError::new(error.message))?;
                (options.on_data)(b"second\n").map_err(|error| pi_agent_core::agent::ToolError::new(error.message))?;
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                Ok(BashExecResult {
                    exit_code: Some(0.0),
                    detached: None,
                })
            })
        }
    }
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    let tool = create_bash_tool(
        JsString::from("/tmp"),
        BashToolOptions {
            operations: Some(Arc::new(ScriptedBash)),
            ..Default::default()
        },
    );
    let result = tool
        .execute(
            "call".into(),
            pi_js_compat::js_json!({"command": "fixture"}),
            None,
            Arc::new(|result| {
                if result
                    .content
                    .iter()
                    .any(|entry| matches!(entry, TextOrImageContent::Text(text) if text.text.contains("second")))
                {
                    return Err(ToolError::new("timer update failed"));
                }
                Ok(())
            }),
            None,
        )
        .await;
    println!(
        "{}",
        if result.is_err() {
            "caught by execute"
        } else {
            "completed"
        }
    );
}
