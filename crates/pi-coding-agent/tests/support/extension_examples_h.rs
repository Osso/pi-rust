//! Batch H extends the shared real-runtime/oracle fixture pattern with renderer IO.
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::sync::Arc;

use crate::shared_extensions as fixture;
use crate::shared_oracle as oracle;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{ColorMode, ColorValue, Theme};
use pi_js_compat::js_json as json;
use serde_json::Value;

pub async fn compare(factory: ExtensionFactory, request: Value) -> JsValue {
    let cwd = request["cwd"].as_str().unwrap();
    let runtime = fixture::runtime();
    let mut registry = ExtensionRegistry::new();
    registry.register("<example:built-in-tool-renderer>", factory);
    let extension = load_extension_from_factory(
        registry
            .get(&JsString::from("<example:built-in-tool-renderer>"))
            .unwrap()
            .clone(),
        &JsString::from(cwd),
        fixture::event_bus(),
        runtime.clone(),
        &JsString::from("<example:built-in-tool-renderer>"),
    )
    .await
    .unwrap();
    assert_eq!(
        extension.read().tools.len(),
        4,
        "example must replace all four built-ins"
    );
    let model_registry = fixture::model_registry().await;
    let runner = fixture::runner_for(vec![extension.clone()], cwd, runtime, model_registry);
    let theme = Theme::new(
        &request["colors"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(role, color)| (JsString::from(role), ColorValue::Text(color.as_str().unwrap().into())))
            .collect::<Vec<_>>(),
        &[],
        ColorMode::Truecolor,
        None,
        None,
    )
    .unwrap();
    let tools = extension
        .read()
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
        let tool = runner.get_tool_definition(item["tool"].as_str().unwrap()).unwrap();
        let options = ToolRenderResultOptions {
            expanded: item["expanded"].as_bool().unwrap_or(false),
            is_partial: item["isPartial"].as_bool().unwrap_or(false),
        };
        let context = ToolRenderContext {
            args: JsValue::from(item["args"].clone()),
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
            (tool.render_call.as_ref().unwrap())(&JsValue::from(item["args"].clone()), &theme, &context).unwrap()
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
        let tool = runner.get_tool_definition(item["tool"].as_str().unwrap()).unwrap();
        executions.push(
            match (tool.execute)(
                "fixture-call".into(),
                JsValue::from(item["params"].clone()),
                None,
                None,
                Some(runner.create_context()),
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
    let actual = json!({"commands":extension.read().commands.keys().collect::<Vec<_>>(),"tools":tools,"renders":renders,"executions":executions,"files":files});
    let mut oracle = oracle::TsOracle::spawn(
        "extension-examples-h-oracle.ts",
        "packages/coding-agent/examples/extensions/built-in-tool-renderer.ts",
    );
    let expected = oracle.request(&JsValue::from(request.clone()));
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
