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
    NewSessionOptions, SessionManager, SharedSessionManager, lock_session_manager,
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
