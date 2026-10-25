#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_coding_agent::bundled_extensions::pyrun::{PyrunExtensionOptions, install_pyrun_extension};
use pi_coding_agent::core::extensions::ExtensionRunner;
use pi_coding_agent::core::extensions::types::RuntimeMailboxEvent;
use pi_js_compat::{js_json as json, process_env};

#[tokio::test]
async fn pyrun_registration_checks_command_without_parsing_args_and_routes_bridge() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_str().unwrap();
    let mut oracle = oracle::TsOracle::spawn(
        "pyrun-registration-oracle.ts",
        "packages/coding-agent/extensions/pyrun/src/index.ts",
    );
    for available in [false, true] {
        process_env::set(
            "PI_PYRUN_RUNNER_COMMAND",
            if available {
                "/bin/true"
            } else {
                "/missing-pyrun-runner"
            },
        );
        process_env::set(
            "PI_PYRUN_RUNNER_ARGS",
            "invalid JSON deliberately deferred until evaluation",
        );
        let runtime = support::runtime();
        let extension = support::extension(&runtime, cwd, "pyrun", |api| {
            install_pyrun_extension(api, PyrunExtensionOptions::default()).unwrap()
        })
        .await;
        let mut runner = ExtensionRunner::new(
            vec![extension],
            runtime,
            cwd,
            support::session_manager(cwd),
            support::model_registry().await,
            None,
        );
        runner
            .bind_core(support::actions(), support::context_actions(), None)
            .unwrap();
        let definition = runner.get_tool_definition("pyrun_eval");
        let actual = definition.map(|tool|json!({"name":tool.name,"description":tool.description,"promptSnippet":tool.prompt_snippet,"promptGuidelines":tool.prompt_guidelines}));
        let expected =
            oracle.request(&json!({"id":1,"command":if available { "/bin/true" } else { "/missing-pyrun-runner" }}));
        assert_eq!(expected["ok"], json!(true), "{expected:?}");
        assert_eq!(json!(actual), expected["output"]);
        if available {
            let body = json!({"protocol":"pyrun-bridge-v1","command":"request","requestId":"request-1","method":"models.scoped","params":null,"toolCallId":"call-1","replyTo":{"sessionId":"session","agentId":"job"},"identity":{"jobId":"job","owner":{"sessionId":"session","agentId":null},"processIdentity":{"pid":999999999,"startTimeTicks":7},"outputLabel":"Pyrun output"}});
            let message = json::from_value(&json!({"id":1,"status":"pending","updatedAt":"2026-01-01T00:00:00.000Z","body":json::stringify_lossless(&body),"createdAt":"2026-01-01T00:00:00.000Z","kind":"system","sender":{"sessionId":"session","agentId":"job"},"recipient":{"sessionId":"session","agentId":null},"storeRef":{"sessionPath":"/session.jsonl","messageId":"message"}})).unwrap();
            let error = runner
                .emit_runtime_mailbox(RuntimeMailboxEvent { message })
                .await
                .expect_err("bridge must reject a missing control database, not silently return unhandled");
            assert_eq!(error.message, "Detached Pyrun bridge requires the control database");
        }
    }
    process_env::set("PI_PYRUN_RUNNER_COMMAND", "/bin/true");
    process_env::set("PI_PYRUN_RUNNER_ARGS", "[]");
    let runtime = support::runtime();
    let extension = support::extension(&runtime, cwd, "pyrun-no-session", |api| {
        install_pyrun_extension(
            api,
            PyrunExtensionOptions {
                background_jobs_store: Some(std::sync::Arc::new(
                    pi_coding_agent::core::multi_agent_store::MultiAgentStore::new(Default::default()),
                )),
                detach_registry: Some(std::sync::Arc::new(
                    pi_coding_agent::core::tool_detach_registry::ToolDetachRegistry::new(),
                )),
                ..Default::default()
            },
        )
        .unwrap();
    })
    .await;
    let mut runner = ExtensionRunner::new(
        vec![extension],
        runtime,
        cwd,
        support::session_manager(cwd),
        support::model_registry().await,
        None,
    );
    runner
        .bind_core(support::actions(), support::context_actions(), None)
        .unwrap();
    let tool = runner.get_tool_definition("pyrun_eval").unwrap();
    let error = (tool.execute)(
        "call-1".into(),
        json!({"code":"1+1"}),
        None,
        None,
        Some(runner.create_context()),
    )
    .await
    .unwrap_err();
    assert!(
        error.message.starts_with("Pyrun runner exited"),
        "non-persisted stores must stay foreground: {:?}",
        error.message
    );
    process_env::clear();
}
