use crate::shared_oracle as oracle;
pub use crate::shared_render_owner as render_owner;
use pi_agent_core::agent::AgentToolResult;
use pi_coding_agent::core::extensions::types::{ToolRenderContext, ToolRenderResultOptions, ToolRenderState};
use pi_coding_agent::core::tools::bash::render_result;
use pi_coding_agent::modes::interactive::theme::{ColorMode, load_theme_from_path, set_theme_instance};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

// Frozen bash.ts:812-822: one interval per state, clear on final/error, restart
// on later partial render; ticks belong to the renderer owner, not a UI poller.
#[test]
fn bash_renderer_timer_starts_stops_and_restarts_on_owner_loop() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("theme/dark.json");
    let theme =
        Arc::new(load_theme_from_path(&JsString::from(path.to_str().unwrap()), Some(ColorMode::Truecolor)).unwrap());
    set_theme_instance(theme.clone());
    let mut source = oracle::TsOracle::spawn(
        "bash-render-timer-oracle.ts",
        "packages/coding-agent/src/core/tools/bash.ts",
    );
    let expected = source.request(&json!({"id":28053}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(expected["output"], json!([1, 1, 2, 2, 2]));
    let state = ToolRenderState::new();
    state.update(|value| *value = json!({"startedAt":0,"endedAt":1250}));
    let owner = render_owner::RenderOwner::new();
    owner.bind(&state);
    let ticks = Arc::new(AtomicUsize::new(0));
    let counter = ticks.clone();
    let thread = std::thread::current().id();
    let mut context = ToolRenderContext {
        args: json!({}),
        tool_call_id: "bash-timer".into(),
        invalidate: Arc::new(move || {
            assert_eq!(std::thread::current().id(), thread, "tick executes on owner thread");
            counter.fetch_add(1, Ordering::SeqCst);
        }),
        last_component: None,
        call_component: None,
        state,
        cwd: "/repo".into(),
        execution_started: false,
        args_complete: true,
        is_partial: true,
        expanded: true,
        show_images: false,
        is_error: false,
    };
    let result = AgentToolResult {
        content: vec![],
        details: None,
        is_error: None,
        terminate: None,
    };
    let render = |context: &ToolRenderContext, partial| {
        render_result(
            &result,
            &ToolRenderResultOptions {
                expanded: true,
                is_partial: partial,
            },
            &theme,
            context,
        )
        .unwrap();
    };
    let mut counts = Vec::new();
    render(&context, true);
    render(&context, true);
    owner.yield_for(Duration::from_millis(1250));
    counts.push(ticks.load(Ordering::SeqCst));
    render(&context, false);
    owner.yield_for(Duration::from_millis(1250));
    counts.push(ticks.load(Ordering::SeqCst));
    render(&context, true);
    owner.yield_for(Duration::from_millis(1250));
    counts.push(ticks.load(Ordering::SeqCst));
    context.is_error = true;
    render(&context, true);
    owner.yield_for(Duration::from_millis(1250));
    counts.push(ticks.load(Ordering::SeqCst));
    context.is_error = false;
    render(&context, true);
    std::thread::sleep(Duration::from_millis(1250));
    render(&context, false);
    owner.yield_for(Duration::from_millis(1250));
    counts.push(ticks.load(Ordering::SeqCst));
    assert_eq!(json!(counts), expected["output"]);
}
