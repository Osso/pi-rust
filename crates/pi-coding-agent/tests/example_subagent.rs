use crate::shared_oracle as oracle;
#[path = "../examples/extensions/subagent/index.rs"]
mod subagent;
use crate::shared_extensions as support;
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct Ui {
    calls: Mutex<Vec<Value>>,
    answer: bool,
}
impl ExtensionUiContext for Ui {
    fn confirm<'a>(
        &'a self,
        title: &JsString,
        message: &JsString,
        _: Option<&ExtensionUiDialogOptions>,
    ) -> BoxFuture<'a, Result<bool, pi_ai::request::CallbackError>> {
        self.calls.lock().unwrap().push(json!([title, message]));
        let answer = self.answer;
        Box::pin(async move { Ok(answer) })
    }
}
fn fixture(root: &std::path::Path) -> (PathBuf, PathBuf) {
    let agent = root.join("agent");
    std::fs::create_dir_all(agent.join("agents")).unwrap();
    for (name, model) in [("scout", ""), ("fixed", "model: fixture/override\n")] {
        std::fs::write(
            agent.join("agents").join(format!("{name}.md")),
            format!(
                "---\nname: {name}\ndescription: Fixture {name}\ntools: read,bash\n{model}---\nPrivate system prompt\n"
            ),
        )
        .unwrap();
    }
    let local = root
        .join(pi_coding_agent::config::config_dir_name().to_utf8().unwrap())
        .join("agents");
    std::fs::create_dir_all(&local).unwrap();
    std::fs::write(
        local.join("local.md"),
        "---\nname: local\ndescription: Local fixture\n---\nLocal prompt",
    )
    .unwrap();
    let script = root.join("child.py");
    std::fs::write(&script, include_str!("fixtures/subagent_child.py")).unwrap();
    (agent, script)
}
async fn runner(
    root: &std::path::Path,
    agent: PathBuf,
    program: PathBuf,
    prefix: Vec<String>,
    ui: Arc<Ui>,
) -> ExtensionRunner {
    let runtime = support::runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register(
        "<example:subagent>",
        Arc::new(move |api| {
            let result = subagent::subagent_extension(
                api,
                agent.clone(),
                subagent::Invocation {
                    program: program.clone(),
                    prefix: prefix.clone().into_iter().map(JsString::from).collect::<Vec<_>>(),
                },
            );
            Box::pin(async move { result })
        }),
    );
    let extension = load_extension_from_factory(
        registry.get(&JsString::from("<example:subagent>")).unwrap().clone(),
        &JsString::from(root.to_str().unwrap()),
        support::event_bus(),
        runtime.clone(),
        &JsString::from("<example:subagent>"),
    )
    .await
    .unwrap();
    let mut runner = support::runner_for(
        vec![extension],
        root.to_str().unwrap(),
        runtime,
        support::model_registry().await,
    );
    let mut context = support::context_actions();
    context.get_model = Arc::new(|| {
        Some(serde_json::from_value(json!({"id":"inherited","provider":"fixture","api":"faux","baseUrl":"http://invalid","name":"fixture","reasoning":true,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":1000,"maxTokens":100})).unwrap())
    });
    context.get_thinking_level = Some(Arc::new(|| pi_agent_core::harness::ThinkingLevel::High));
    runner.bind_core(support::actions(), context, None).unwrap();
    runner.set_ui_context(Some(ui), ExtensionMode::Tui);
    runner
}
#[tokio::test]
async fn tool_description_matches_the_pinned_extension_literal() {
    let temp = tempfile::tempdir().unwrap();
    let (agent, script) = fixture(temp.path());
    let runner = runner(
        temp.path(),
        agent.clone(),
        PathBuf::from("python3"),
        Vec::new(),
        Arc::new(Ui::default()),
    )
    .await;
    let tool = runner.get_tool_definition("subagent").unwrap();
    let mut ts = oracle::TsOracle::spawn_with_environment(
        "example-subagent-oracle.ts",
        "packages/coding-agent/examples/extensions/subagent/index.ts",
        &[
            ("PI_CODING_AGENT_DIR", agent.into_os_string()),
            ("SUBAGENT_FIXTURE", script.into_os_string()),
        ],
    );
    let expected = ts.request(&json!({"cwd":temp.path(),"description":true}));
    assert_eq!(
        tool.description,
        JsString::from(expected["description"].as_str().unwrap())
    );
}

#[tokio::test]
async fn modes_dispatch_and_streaming_match_pinned_real_extension_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let (agent, script) = fixture(temp.path());
    let ui = Arc::new(Ui {
        answer: true,
        ..Default::default()
    });
    let runner = runner(
        temp.path(),
        agent.clone(),
        PathBuf::from("python3"),
        vec![script.to_str().unwrap().into()],
        ui.clone(),
    )
    .await;
    let tool = runner.get_tool_definition("subagent").unwrap();
    assert_eq!(tool.label, "Subagent");
    // The companion exported helper remains usable by native embedders.
    let discovered = subagent::agents::discover_agents(temp.path(), &JsString::from("user"), &agent).unwrap();
    assert_eq!(
        subagent::agents::format_agent_list(&discovered.agents, 1)["remaining"],
        pi_js_compat::js_json!(1)
    );
    let mut ts = oracle::TsOracle::spawn_with_environment(
        "example-subagent-oracle.ts",
        "packages/coding-agent/examples/extensions/subagent/index.ts",
        &[
            ("PI_CODING_AGENT_DIR", agent.into_os_string()),
            ("SUBAGENT_FIXTURE", script.into_os_string()),
        ],
    );
    for params in [
        json!({}),
        json!({"agent":"scout","task":"a","tasks":[{"agent":"scout","task":"b"}]}),
        json!({"agent":"missing","task":"x"}),
        json!({"agent":"scout","task":"hello"}),
        json!({"agent":"scout","task":"ZERO"}),
        json!({"agent":"fixed","task":"fixed-model"}),
        json!({"agent":"scout","task":"FAIL"}),
        json!({"chain":[{"agent":"scout","task":"DOLLARS"},{"agent":"scout","task":"pre {previous} middle {previous} post"}]}),
        json!({"chain":[{"agent":"scout","task":"one"},{"agent":"scout","task":"two {previous}"}]}),
        json!({"chain":[{"agent":"scout","task":"FAIL"},{"agent":"scout","task":"not run"}]}),
        json!({"tasks":[{"agent":"scout","task":"alpha"},{"agent":"missing","task":"beta"}]}),
        json!({"tasks":(0..9).map(|i|json!({"agent":"scout","task":i.to_string()})).collect::<Vec<_>>()}),
        json!({"agentScope":"both","agent":"local","task":"trusted"}),
    ] {
        let updates = Arc::new(Mutex::new(Vec::new()));
        let capture = updates.clone();
        ui.calls.lock().unwrap().clear();
        let result = (tool.execute)(
            "call".into(),
            JsValue::from(params.clone()),
            None,
            Some(Arc::new(move |update| {
                capture.lock().unwrap().push(serde_json::to_value(update).unwrap());
                Ok(())
            })),
            Some(runner.create_context()),
        )
        .await
        .unwrap();
        let expected = ts.request(&json!({"cwd":temp.path(),"params":params,"confirm":true}));
        assert_eq!(serde_json::to_value(&result).unwrap(), expected["result"], "{params}");
        assert_eq!(
            *ui.calls.lock().unwrap(),
            expected["confirmations"].as_array().unwrap().clone()
        );
        // Clone in the oracle at callback time, matching approved native isolation.
        if params.get("tasks").is_none() {
            assert_eq!(
                serde_json::to_value(&*updates.lock().unwrap()).unwrap(),
                expected["updates"],
                "streaming {params}"
            );
        }
        // Parallel completion order is nondeterministic; compare final results and counts.
        assert_eq!(
            updates.lock().unwrap().len(),
            expected["updateCount"].as_u64().unwrap() as usize
        );
    }
    let journal = std::fs::read_to_string(temp.path().join("journal.jsonl")).unwrap();
    let records = journal
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    let hello_invocations: Vec<_> = records.iter().filter(|r| r["task"] == "Task: hello").collect();
    assert_eq!(
        hello_invocations.len(),
        2,
        "native and frozen child invocations must both be recorded"
    );
    assert_eq!(
        json!({"model":hello_invocations[0]["model"],"thinking":hello_invocations[0]["thinking"]}),
        json!({"model":hello_invocations[1]["model"],"thinking":hello_invocations[1]["thinking"]}),
        "DRIFT: native inherited invocation flags differ from the frozen child invocation"
    );
    let inherited = records.iter().find(|r| r["task"] == "Task: hello").unwrap();
    assert_eq!(inherited["model"], Value::Null);
    assert_eq!(inherited["thinking"], Value::Null);
    assert_eq!(inherited["tools"], "read,bash");
    assert_eq!(inherited["prompt"], "Private system prompt");
    assert!(!std::path::Path::new(inherited["promptPath"].as_str().unwrap()).exists());
    let fixed = records.iter().find(|r| r["task"] == "Task: fixed-model").unwrap();
    assert_eq!(fixed["model"], "fixture/override");
    assert!(fixed["thinking"].is_null());
}
#[tokio::test]
async fn project_refusal_spawn_error_and_abort_use_private_children() {
    let temp = tempfile::tempdir().unwrap();
    let (agent, script) = fixture(temp.path());
    let ui = Arc::new(Ui::default());
    let runner = runner(
        temp.path(),
        agent.clone(),
        PathBuf::from("python3"),
        vec![script.to_str().unwrap().into()],
        ui,
    )
    .await;
    let tool = runner.get_tool_definition("subagent").unwrap();
    let result = (tool.execute)(
        "c".into(),
        JsValue::from(json!({"agentScope":"project","agent":"local","task":"x"})),
        None,
        None,
        Some(runner.create_context()),
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap()["content"][0]["text"],
        "Canceled: project-local agents not approved."
    );
    assert!(!temp.path().join("journal.jsonl").exists());
    let controller_token = pi_js_compat::abort::AbortController::new();
    let token = controller_token.signal();
    let controller_trigger = controller_token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        controller_trigger.abort(None);
    });
    let error = (tool.execute)(
        "c".into(),
        JsValue::from(json!({"agent":"scout","task":"SLOW"})),
        Some(token),
        None,
        Some(runner.create_context()),
    )
    .await
    .unwrap_err();
    assert_eq!(error.message, "Subagent was aborted");
    let runner = crate::runner(
        temp.path(),
        agent,
        temp.path().join("missing-program"),
        vec![],
        Arc::new(Ui::default()),
    )
    .await;
    let result = (runner.get_tool_definition("subagent").unwrap().execute)(
        "c".into(),
        JsValue::from(json!({"agent":"scout","task":"x"})),
        None,
        None,
        Some(runner.create_context()),
    )
    .await
    .unwrap();
    assert_eq!(result.is_error, Some(true));
}

#[tokio::test]
async fn call_and_result_renderers_match_single_chain_parallel_and_tools() {
    check_renderers(false, false).await;
}
#[tokio::test]
async fn running_parallel_renderer_matches_pinned() {
    check_renderers(true, false).await;
}
#[tokio::test]
async fn usage_decimal_ties_match_pinned() {
    check_renderers(false, true).await;
}
async fn check_renderers(running: bool, ties: bool) {
    let temp = tempfile::tempdir().unwrap();
    let (agent, script) = fixture(temp.path());
    let runner = runner(
        temp.path(),
        agent.clone(),
        PathBuf::from("python3"),
        vec![script.to_str().unwrap().into()],
        Arc::new(Ui::default()),
    )
    .await;
    let tool = runner.get_tool_definition("subagent").unwrap();
    let mut ts = oracle::TsOracle::spawn_with_environment(
        "example-subagent-oracle.ts",
        "packages/coding-agent/examples/extensions/subagent/index.ts",
        &[
            ("PI_CODING_AGENT_DIR", agent.into_os_string()),
            ("SUBAGENT_FIXTURE", script.into_os_string()),
        ],
    );
    let mut content = vec![];
    for (name, args) in [
        ("bash", json!({"command":"echo concrete"})),
        ("read", json!({"path":"/tmp/input","offset":3,"limit":2})),
        ("write", json!({"path":"/tmp/out","content":"a\nb"})),
        ("edit", json!({"path":"/tmp/out"})),
        ("ls", json!({"path":"/tmp"})),
        ("grep", json!({"pattern":"needle","path":"/tmp"})),
        ("find", json!({"pattern":"*.rs","path":"/tmp"})),
        ("other", json!({"x":1})),
    ] {
        content.push(json!({"type":"toolCall","name":name,"arguments":args}));
    }
    content.push(json!({"type":"text","text":"final\nsecond\nthird\nfourth"}));
    let mut row = json!({"agent":"scout","agentSource":"user","task":"the task","exitCode":0,"messages":[{"role":"assistant","content":content}],"usage":{"input":1234,"output":12000,"cacheRead":3,"cacheWrite":1,"cost":0.125,"turns":2,"contextTokens":20000},"model":"fixture/model","step":1});
    if ties {
        row["usage"]["input"] = json!(1250);
        row["usage"]["output"] = json!(1250000);
        row["usage"]["cost"] = json!(0.03125);
    }
    let mut renders = vec![
        json!({"args":{}}),
        json!({"args":{"agent":"scout","task":"long ".repeat(20)}}),
        json!({"args":{"chain":(0..4).map(|_|json!({"agent":"scout","task":"before {previous}"})).collect::<Vec<_>>()}}),
        json!({"args":{"tasks":(0..4).map(|_|json!({"agent":"scout","task":"x"})).collect::<Vec<_>>()}}),
    ];
    for mode in ["single", "chain", "parallel"] {
        for expanded in [false, true] {
            renders.push(json!({"result":{"content":[{"type":"text","text":"fallback"}],"details":{"mode":mode,"results":[row.clone()]}},"expanded":expanded}));
        }
    }
    if running {
        let mut partial = row.clone();
        partial["exitCode"] = json!(-1);
        partial["messages"] = json!([]);
        renders.push(json!({"result":{"content":[],"details":{"mode":"parallel","results":[partial]}},"expanded":true,"partial":true}));
    }
    let mut error = row.clone();
    error["exitCode"] = json!(2);
    error["stopReason"] = json!("error");
    error["errorMessage"] = json!("failed");
    renders.push(json!({"result":{"content":[],"details":{"mode":"parallel","results":[error,row]}},"expanded":true}));
    renders.push(json!({"result":{"content":[{"type":"text","text":"no details"}]},"expanded":false}));
    renders.push(json!({"args":{"agent":"scout","task":format!("{}🚀", "a".repeat(59))}}));
    let expected: JsValue = ts.request(&pi_js_compat::js_json!({"cwd":temp.path(),"renders":renders}));
    let theme = pi_coding_agent::modes::interactive::theme::Theme::new(
        &[
            "toolTitle",
            "accent",
            "muted",
            "dim",
            "toolOutput",
            "warning",
            "error",
            "success",
        ]
        .into_iter()
        .map(|s| {
            (
                s.into(),
                pi_coding_agent::modes::interactive::theme::ColorValue::Text("#888888".into()),
            )
        })
        .collect::<Vec<_>>(),
        &[],
        pi_coding_agent::modes::interactive::theme::ColorMode::Truecolor,
        None,
        None,
    )
    .unwrap();
    for (i, r) in renders.iter().enumerate() {
        let context = ToolRenderContext {
            args: JsValue::from(r["args"].clone()),
            tool_call_id: "c".into(),
            invalidate: Arc::new(|| {}),
            last_component: None,
            call_component: None,
            state: Default::default(),
            cwd: JsString::from(temp.path().display().to_string()),
            execution_started: true,
            args_complete: true,
            is_partial: r["partial"].as_bool().unwrap_or(false),
            expanded: r["expanded"].as_bool().unwrap_or(false),
            show_images: false,
            is_error: false,
        };
        let component = if let Some(value) = r.get("result") {
            (tool.render_result.as_ref().unwrap())(
                &serde_json::from_value(value.clone()).unwrap(),
                &ToolRenderResultOptions {
                    expanded: context.expanded,
                    is_partial: r["partial"].as_bool().unwrap_or(false),
                },
                &theme,
                &context,
            )
            .unwrap()
        } else {
            (tool.render_call.as_ref().unwrap())(&JsValue::from(r["args"].clone()), &theme, &context).unwrap()
        };
        let actual = component
            .borrow_mut()
            .render(100)
            .iter()
            .map(pi_coding_agent::utils::ansi::strip_ansi)
            .collect::<Vec<_>>();
        assert_eq!(pi_js_compat::js_json!(actual), expected["renders"][i], "render {i}");
    }
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
