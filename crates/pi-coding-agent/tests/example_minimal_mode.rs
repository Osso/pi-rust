#[path = "../examples/extensions/minimal_mode.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};
use std::sync::Arc;

#[tokio::test]
async fn all_seven_tools_render_and_execute_like_pinned_extension() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().to_str().unwrap();
    std::fs::write(tmp.path().join("fixture.txt"), "needle\nsecond\n").unwrap();
    let runtime = create_extension_runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register(
        "minimal-mode",
        Arc::new(|api| {
            let result = example::minimal_mode_extension(api);
            Box::pin(async move { result })
        }),
    );
    let extension = load_extension_from_factory(
        registry.get(&JsString::from("minimal-mode")).unwrap().clone(),
        &JsString::from(cwd),
        support::event_bus(),
        runtime.clone(),
        &JsString::from("minimal-mode"),
    )
    .await
    .unwrap();
    let metadata: Vec<_> = extension
        .read()
        .tools
        .values()
        .map(|tool| {
            let d = &tool.definition;
            json!({"name":d.name,"label":d.label,"description":d.description,"parameters":d.parameters})
        })
        .collect();
    let model_registry = pi_coding_agent::core::model_registry::ModelRegistry::in_memory(
        pi_coding_agent::core::auth_storage::AuthStorage::in_memory(serde_json::Map::new()),
    )
    .unwrap();
    let mut runner = support::runner_for(vec![extension.clone()], cwd, runtime, Arc::new(model_registry));
    support::bind(&mut runner);
    let theme = load_theme_from_path(
        &JsString::from(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("theme/dark.json")
                .to_str()
                .expect("fixture path must be UTF-8"),
        ),
        Some(ColorMode::Truecolor),
    )
    .unwrap();
    let mut calls = Vec::new();
    let mut actual = Vec::new();
    for name in ["read", "bash", "write", "edit", "find", "grep", "ls"] {
        for args in [
            json!({}),
            json!({"path":"fixture.txt","pattern":"needle","command":"echo hi","timeout":2.0,"offset":2.0,"limit":3.0,"content":"one\ntwo","glob":"*.txt"}),
            json!({"path":"","pattern":"","command":"","timeout":0,"offset":0,"limit":0,"content":""}),
            json!({"path":"fixture.txt","offset":2}),
            json!({"path":"fixture.txt","limit":0}),
            json!({"path":format!("{}/suffix",std::env::var("HOME").unwrap()),"offset":1,"limit":0}),
            json!({"path":"λ🦀","content":"x","glob":""}),
        ] {
            for expanded in [false, true] {
                for content in [
                    json!([]),
                    json!([{"type":"text","text":""}]),
                    json!([{"type":"text","text":"  one\ntwo\n "}]),
                    json!([{"type":"text","text":"Error: failure"},{"type":"text","text":"ignored"}]),
                ] {
                    let result = json!({"content":content});
                    let context = ToolRenderContext {
                        args: JsValue::from(args.clone()),
                        tool_call_id: "fixture".into(),
                        invalidate: Arc::new(|| {}),
                        last_component: None,
                        call_component: None,
                        state: ToolRenderState::new(),
                        cwd: cwd.into(),
                        execution_started: false,
                        args_complete: true,
                        is_partial: false,
                        expanded,
                        show_images: false,
                        is_error: false,
                    };
                    let tool = runner.get_tool_definition(name).unwrap();
                    let call = tool.render_call.as_ref().unwrap()(&JsValue::from(args.clone()), &theme, &context)
                        .unwrap()
                        .borrow_mut()
                        .render(1000);
                    let rendered = tool.render_result.as_ref().unwrap()(
                        &serde_json::from_value(result.clone()).unwrap(),
                        &ToolRenderResultOptions {
                            expanded,
                            is_partial: false,
                        },
                        &theme,
                        &context,
                    )
                    .unwrap()
                    .borrow_mut()
                    .render(1000);
                    actual.push(json!({"call":call,"result":rendered}));
                    calls.push(json!({"name":name,"args":args,"expanded":expanded,"result":result}));
                }
            }
        }
    }
    let executions = json!([
        {"name":"read","args":{"path":"fixture.txt"}},
        {"name":"write","args":{"path":"out.txt","content":"written\n"}},
        {"name":"edit","args":{"path":"out.txt","edits":[{"oldText":"written","newText":"changed"}]}},
        {"name":"find","args":{"pattern":"fixture.txt","path":"."}},
        {"name":"grep","args":{"pattern":"needle","path":"fixture.txt"}},
        {"name":"ls","args":{"path":"."}},
        {"name":"bash","args":{"command":"printf minimal-fixture"}},
        {"name":"read","args":{"path":"missing.txt"}},
        {"name":"write","args":{"path":".","content":"no"}},
        {"name":"edit","args":{"path":"out.txt","edits":[{"oldText":"absent","newText":"no"}]}},
        {"name":"find","args":{"pattern":"*.txt","path":"missing"}},
        {"name":"grep","args":{"pattern":"needle","path":"missing.txt"}},
        {"name":"ls","args":{"path":"missing"}},
        {"name":"bash","args":{"command":"exit 7"}}
    ]);
    // TS runs before Rust against identical initial fixture state.
    let mut oracle = oracle::TsOracle::spawn(
        "extension-examples-g-minimal-oracle.ts",
        "packages/coding-agent/examples/extensions/minimal-mode.ts",
    );
    let expected = oracle.request(&json!({"cwd":cwd,"calls":calls,"executions":executions}));
    std::fs::remove_file(tmp.path().join("out.txt")).unwrap();
    let mut executed = Vec::new();
    for item in executions.as_array().unwrap() {
        let tool = runner.get_tool_definition(item["name"].as_str().unwrap()).unwrap();
        executed.push(
            match (tool.execute)(
                "fixture".into(),
                JsValue::from(item["args"].clone()),
                None,
                None,
                Some(runner.create_context()),
            )
            .await
            {
                Ok(value) => json!({"value":value}),
                Err(error) => json!({"error":error.message}),
            },
        );
    }
    for (index, item) in metadata.iter().enumerate() {
        assert_eq!(*item, expected["metadata"][index], "metadata {index}");
    }
    for (index, item) in actual.iter().enumerate() {
        assert_eq!(*item, expected["calls"][index], "render {index}: {}", calls[index]);
    }
    assert_eq!(json!(executed), expected["executions"]);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("out.txt")).unwrap(),
        "changed\n"
    );
    let _: Value = expected;
}
