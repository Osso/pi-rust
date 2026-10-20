//! Batch H extends the shared real-runtime/oracle fixture pattern with renderer IO.
use std::sync::Arc;

use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, Theme};
use serde_json::{Value, json};
#[path = "oracle.rs"]
mod oracle;
#[path = "extensions.rs"]
mod support;

pub async fn compare(factory: ExtensionFactory, request: Value) -> Value {
    let cwd = request["cwd"].as_str().unwrap();
    let runtime = support::runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register("<example:built-in-tool-renderer>", factory);
    let extension = load_extension_from_factory(
        registry.get("<example:built-in-tool-renderer>").unwrap().clone(),
        cwd,
        support::event_bus(),
        runtime.clone(),
        "<example:built-in-tool-renderer>",
    )
    .await
    .unwrap();
    assert_eq!(extension.tools.len(), 4, "example must replace all four built-ins");
    let model_registry = support::model_registry().await;
    let runner = support::runner_for(vec![extension.clone()], cwd, runtime, model_registry);
    let theme = Theme::new(
        &request["colors"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(role, color)| (role.clone(), ColorValue::Text(color.as_str().unwrap().into())))
            .collect::<Vec<_>>(),
        &[],
        ColorMode::Truecolor,
        None,
        None,
    )
    .unwrap();
    let tools = extension
        .tools
        .values()
        .map(|tool| {
            let d = &tool.definition;
            json!({"name":d.name,"label":d.label,"description":d.description,"parameters":d.parameters,
            "renderShell":d.render_shell.map(|shell| match shell { RenderShell::SelfRender => "self", RenderShell::Default => "default" }),"promptSnippet":d.prompt_snippet,"promptGuidelines":d.prompt_guidelines,
            "approvalRequired":d.approval_required,"approvalKind":d.approval_kind})
        })
        .collect::<Vec<_>>();
    let mut renders = Vec::new();
    for item in request["renders"].as_array().unwrap() {
        let tool = &extension.tools.get(item["tool"].as_str().unwrap()).unwrap().definition;
        let options = ToolRenderResultOptions {
            expanded: item["expanded"].as_bool().unwrap_or(false),
            is_partial: item["isPartial"].as_bool().unwrap_or(false),
        };
        let context = ToolRenderContext {
            args: item["args"].clone(),
            tool_call_id: "fixture-call".into(),
            invalidate: Arc::new(|| {}),
            last_component: None,
            call_component: None,
            state: Default::default(),
            cwd: cwd.into(),
            execution_started: true,
            args_complete: true,
            is_partial: options.is_partial,
            expanded: options.expanded,
            show_images: false,
            is_error: item["isError"].as_bool().unwrap_or(false),
        };
        let component = if let Some(result) = item.get("result") {
            let result = serde_json::from_value(result.clone()).unwrap();
            (tool.render_result.as_ref().unwrap())(&result, &options, &theme, &context).unwrap()
        } else {
            (tool.render_call.as_ref().unwrap())(&item["args"], &theme, &context).unwrap()
        };
        renders.push(
            request["widths"]
                .as_array()
                .unwrap()
                .iter()
                .map(|width| component.borrow_mut().render(width.as_u64().unwrap() as usize))
                .collect::<Vec<_>>(),
        );
    }
    for (name, text) in request["initial"].as_object().unwrap() {
        std::fs::write(std::path::Path::new(cwd).join(name), text.as_str().unwrap()).unwrap();
    }
    let mut executions = Vec::new();
    for item in request["executions"].as_array().unwrap() {
        let tool = &extension.tools.get(item["tool"].as_str().unwrap()).unwrap().definition;
        executions.push(
            match (tool.execute)(
                "fixture-call".into(),
                item["params"].clone(),
                None,
                None,
                runner.create_context(),
            )
            .await
            {
                Ok(result) => json!({"value":result}),
                Err(error) => json!({"error":error.message}),
            },
        );
    }
    let files = request["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| {
            let name = name.as_str().unwrap();
            (
                name.to_owned(),
                Value::String(std::fs::read_to_string(std::path::Path::new(cwd).join(name)).unwrap()),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let actual = json!({"commands":extension.commands.keys().collect::<Vec<_>>(),"tools":tools,"renders":renders,"executions":executions,"files":files});
    let mut oracle = oracle::TsOracle::spawn(
        "extension-examples-h-oracle.ts",
        "packages/coding-agent/examples/extensions/built-in-tool-renderer.ts",
    );
    let expected = oracle.request(&request);
    for key in ["commands", "tools", "executions", "files"] {
        assert_eq!(actual[key], expected[key], "complete {key}");
    }
    for (index, item) in request["renders"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            actual["renders"][index], expected["renders"][index],
            "render case {index}: {item}"
        );
    }
    assert_eq!(actual, expected, "complete renderer/runtime transcript");
    actual
}
