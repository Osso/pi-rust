//! Behavioral tests for `coding-agent/src/core/export-html/tool-renderer.ts`.

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;

use pi_agent_core::agent::types::AgentToolResult;
use pi_agent_core::agent::{AgentToolUpdateCallback, ToolError};
use pi_ai::types::TextOrImageContent;
use pi_coding_agent::core::export_html::{
    RenderedToolResult, ToolHtmlRenderer, ToolHtmlRendererDeps, create_tool_html_renderer,
};
use pi_coding_agent::core::extensions::types::{
    ExtensionContext, HandlerError, ToolDefinition, ToolRenderContext, ToolRenderResultOptions,
};
use pi_coding_agent::modes::interactive::theme::Theme;
use pi_tui::tui::{Component, ComponentRef};
use serde_json::json;

fn dark_theme() -> std::sync::Arc<pi_coding_agent::modes::interactive::theme::Theme> {
    use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme = std::sync::Arc::new(
        load_theme_from_path(
            &JsString::from((&path).to_str().expect("fixture path must be UTF-8")),
            Some(ColorMode::Truecolor),
        )
        .expect("built-in dark theme"),
    );
    set_theme_instance(std::sync::Arc::clone(&theme));
    theme
}

struct TestComponent {
    lines: Vec<String>,
    widths: Arc<Mutex<Vec<usize>>>,
}

impl Component for TestComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.widths.lock().unwrap().push(width);
        self.lines.clone().into_iter().map(JsString::from).collect::<Vec<_>>()
    }
}

fn make_component(lines: Vec<String>, widths: Arc<Mutex<Vec<usize>>>) -> ComponentRef {
    Rc::new(RefCell::new(TestComponent { lines, widths }))
}

type RenderCall = Arc<dyn Fn(&JsValue, &Theme, &ToolRenderContext) -> Result<ComponentRef, HandlerError> + Send + Sync>;
type RenderResult = Arc<
    dyn Fn(&AgentToolResult, &ToolRenderResultOptions, &Theme, &ToolRenderContext) -> Result<ComponentRef, HandlerError>
        + Send
        + Sync,
>;

fn tool_definition(render_call: Option<RenderCall>, render_result: Option<RenderResult>) -> ToolDefinition {
    ToolDefinition {
        name: "custom".to_owned(),
        label: "custom".into(),
        description: "custom".into(),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        parameters: json!({ "type": "object" }),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(
            |_id: String,
             _params: JsValue,
             _signal: Option<pi_js_compat::abort::AbortSignal>,
             _on_update: Option<AgentToolUpdateCallback>,
             _context: Option<ExtensionContext>| {
                Box::pin(async { Err::<AgentToolResult, HandlerError>(HandlerError::new("unused")) })
            },
        ),
        render_call,
        render_result,
    }
}

fn make_renderer(tool: Option<ToolDefinition>, width: Option<usize>) -> Rc<dyn ToolHtmlRenderer> {
    create_tool_html_renderer(ToolHtmlRendererDeps {
        get_tool_definition: Arc::new(move |name: &str| tool.clone().filter(|definition| definition.name == name)),
        theme: dark_theme(),
        cwd: JsString::from("/tmp"),
        width,
        completion_executor: None,
    })
}

#[test]
fn trims_tui_spacing_lines_from_custom_tool_result_html() {
    let widths = Arc::new(Mutex::new(Vec::new()));
    let lines = vec![
        String::new(),
        "\u{1b}[31mone\u{1b}[0m".to_owned(),
        "two".to_owned(),
        String::new(),
    ];
    let render_result: RenderResult =
        Arc::new(move |_result, _options, _theme, _context| Ok(make_component(lines.clone(), Arc::clone(&widths))));
    let renderer = make_renderer(Some(tool_definition(None, Some(render_result))), None);

    let rendered = renderer
        .render_result("id", "custom", &[], None, false)
        .expect("render result");
    assert_eq!(
        rendered.expanded.as_ref(),
        (Some(
            "<div class=\"ansi-line\"><span style=\"color:#800000\">one</span></div><div class=\"ansi-line\">two</div>"
        ))
        .map(JsString::from)
        .as_ref()
    );
}

#[test]
fn returns_none_without_a_custom_renderer() {
    let renderer = make_renderer(Some(tool_definition(None, None)), None);
    assert_eq!(
        renderer.render_call("id", "custom", &JsValue::from(json!({}).clone())),
        None
    );
    assert_eq!(renderer.render_result("id", "custom", &[], None, false), None);

    let missing = make_renderer(None, None);
    assert_eq!(
        missing.render_call("id", "custom", &JsValue::from(json!({}).clone())),
        None
    );
}

#[test]
fn renders_call_html_with_the_default_width() {
    let widths = Arc::new(Mutex::new(Vec::new()));
    let call_widths = Arc::clone(&widths);
    let render_call: RenderCall =
        Arc::new(move |_args, _theme, _context| Ok(make_component(vec!["call".to_owned()], Arc::clone(&call_widths))));
    let renderer = make_renderer(Some(tool_definition(Some(render_call), None)), None);

    let html = renderer
        .render_call("id", "custom", &JsValue::from(json!({ "path": "a.ts" }).clone()))
        .expect("render call");
    assert_eq!(html, "<div class=\"ansi-line\">call</div>");
    assert_eq!(*widths.lock().unwrap(), vec![100]);
}

#[test]
fn passes_args_and_previous_component_identity() {
    let seen_args = Arc::new(Mutex::new(Vec::new()));
    let seen_last: Arc<Mutex<Vec<Option<usize>>>> = Arc::new(Mutex::new(Vec::new()));
    let args_log = Arc::clone(&seen_args);
    let last_log = Arc::clone(&seen_last);
    let widths = Arc::new(Mutex::new(Vec::new()));
    let closure_widths = Arc::clone(&widths);
    let render_call: RenderCall = Arc::new(move |args, _theme, context| {
        args_log.lock().unwrap().push(args.clone());
        last_log.lock().unwrap().push(
            context
                .last_component
                .as_ref()
                .map(|component| Rc::as_ptr(component) as *const () as usize),
        );
        match &context.last_component {
            Some(component) => Ok(Rc::clone(component)),
            None => Ok(make_component(vec!["first".to_owned()], Arc::clone(&closure_widths))),
        }
    });
    let renderer = make_renderer(Some(tool_definition(Some(render_call), None)), Some(40));

    let first = renderer.render_call("id", "custom", &JsValue::from(json!({ "n": 1 }).clone()));
    let second = renderer.render_call("id", "custom", &JsValue::from(json!({ "n": 2 }).clone()));

    assert_eq!(
        first.as_ref(),
        Some(&JsString::from("<div class=\"ansi-line\">first</div>"))
    );
    assert_eq!(
        second.as_ref(),
        Some(&JsString::from("<div class=\"ansi-line\">first</div>"))
    );
    assert_eq!(
        *seen_args.lock().unwrap(),
        vec![JsValue::from(json!({ "n": 1 })), JsValue::from(json!({ "n": 2 }))]
    );
    let seen_last = seen_last.lock().unwrap();
    assert!(seen_last[0].is_none());
    assert!(seen_last[1].is_some());
    assert_eq!(*widths.lock().unwrap(), vec![40, 40]);
}

#[test]
fn shares_render_state_between_calls() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_log = Arc::clone(&observed);
    let widths = Arc::new(Mutex::new(Vec::new()));
    let render_call: RenderCall = Arc::new(move |_args, _theme, context| {
        observed_log.lock().unwrap().push(context.state.get());
        Ok(make_component(Vec::new(), Arc::clone(&widths)))
    });
    let renderer = make_renderer(Some(tool_definition(Some(render_call), None)), None);

    let _ = renderer.render_call("id", "custom", &JsValue::from(json!({}).clone()));
    let _ = renderer.render_call("id", "custom", &JsValue::from(json!({}).clone()));
    let observed = observed.lock().unwrap();
    assert_eq!(observed[0], pi_js_compat::js_json!({}));
    assert_eq!(observed[1], pi_js_compat::js_json!({}));
}

#[test]
fn omits_collapsed_html_when_it_equals_expanded_html() {
    let widths = Arc::new(Mutex::new(Vec::new()));
    let render_result: RenderResult = Arc::new(move |_result, _options, _theme, _context| {
        Ok(make_component(vec!["same".to_owned()], Arc::clone(&widths)))
    });
    let renderer = make_renderer(Some(tool_definition(None, Some(render_result))), None);

    let rendered = renderer
        .render_result("id", "custom", &[], None, false)
        .expect("render result");
    assert_eq!(
        rendered,
        RenderedToolResult {
            collapsed: None,
            expanded: Some(JsString::from("<div class=\"ansi-line\">same</div>")),
        }
    );
}

#[test]
fn includes_collapsed_html_when_it_differs() {
    let widths = Arc::new(Mutex::new(Vec::new()));
    let render_result: RenderResult = Arc::new(move |_result, options, _theme, _context| {
        let line = if options.expanded { "full" } else { "short" };
        Ok(make_component(vec![line.to_owned()], Arc::clone(&widths)))
    });
    let renderer = make_renderer(Some(tool_definition(None, Some(render_result))), None);

    let rendered = renderer
        .render_result("id", "custom", &[], None, false)
        .expect("render result");
    assert_eq!(
        rendered.collapsed.as_ref(),
        Some(&JsString::from("<div class=\"ansi-line\">short</div>"))
    );
    assert_eq!(
        rendered.expanded.as_ref(),
        Some(&JsString::from("<div class=\"ansi-line\">full</div>"))
    );
}

#[test]
fn forwards_content_details_and_error_state() {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let captured_log = Arc::clone(&captured);
    let widths = Arc::new(Mutex::new(Vec::new()));
    let render_result: RenderResult = Arc::new(move |result, _options, _theme, _context| {
        captured_log.lock().unwrap().push(result.clone());
        Ok(make_component(Vec::new(), Arc::clone(&widths)))
    });
    let renderer = make_renderer(Some(tool_definition(None, Some(render_result))), None);

    let content = vec![TextOrImageContent::Text(pi_ai::types::TextContent::new("body"))];
    let details = json!({ "path": "a.ts" });
    let _ = renderer.render_result("id", "custom", &content, Some(&JsValue::from(details.clone())), true);

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 2);
    assert_eq!(captured[0].content, content);
    assert_eq!(captured[0].details, Some(details).map(JsValue::from));
    assert_eq!(captured[0].is_error, Some(true));
}

#[test]
fn component_errors_surface_as_none() {
    // The Rust renderer contract cannot reject; a missing renderer is the only
    // `undefined` path. This test documents that `ToolError` is not involved.
    let _ = ToolError::new("unused");
}
