//! Port of `packages/coding-agent/src/core/export-html/tool-renderer.ts`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use pi_ai::types::TextOrImageContent;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_tui::tui::ComponentRef;

use super::ansi_to_html::{ansi_lines_to_html, strip_sgr_sequences};
use super::index::{RenderedToolResult, ToolHtmlRenderer};
use pi_agent_core::agent::types::AgentToolResult;

use crate::core::extensions::types::{ToolDefinition, ToolRenderContext, ToolRenderResultOptions};
use crate::modes::interactive::theme::Theme;

/// Looks up an extension tool definition by name.
pub type ToolDefinitionLookup = Arc<dyn Fn(&str) -> Option<ToolDefinition> + Send + Sync>;

/// Dependencies for [`create_tool_html_renderer`].
pub struct ToolHtmlRendererDeps {
    /// Function to look up a tool definition by name.
    pub get_tool_definition: ToolDefinitionLookup,
    pub theme: Arc<Theme>,
    pub cwd: JsString,
    /// Terminal width for rendering (default: 100).
    pub width: Option<usize>,
    /// Bind an owner when a reusable renderer is observed after awaiting previews.
    /// Production export snapshots synchronously and supplies None.
    pub completion_executor: Option<crate::core::extensions::types::ToolRenderCompletionExecutor>,
}

struct RendererState {
    rendered_call_components: HashMap<String, ComponentRef>,
    rendered_result_components: HashMap<String, ComponentRef>,
    rendered_states: HashMap<String, crate::core::extensions::types::ToolRenderState>,
    rendered_args: HashMap<String, JsValue>,
}

/// Tool HTML renderer backed by extension TUI renderers.
pub struct ExtensionToolHtmlRenderer {
    get_tool_definition: ToolDefinitionLookup,
    theme: Arc<Theme>,
    cwd: JsString,
    width: usize,
    completion_executor: Option<crate::core::extensions::types::ToolRenderCompletionExecutor>,
    state: RefCell<RendererState>,
}

impl ExtensionToolHtmlRenderer {
    fn state_for(&self, tool_call_id: &str) -> crate::core::extensions::types::ToolRenderState {
        self.state
            .borrow_mut()
            .rendered_states
            .entry(tool_call_id.to_owned())
            .or_insert_with(|| {
                let state = crate::core::extensions::types::ToolRenderState::new();
                state.set_completion_executor(self.completion_executor.clone());
                state
            })
            .clone()
    }

    fn render_context(
        &self,
        tool_call_id: &str,
        last_component: Option<ComponentRef>,
        expanded: bool,
        is_partial: bool,
        is_error: bool,
    ) -> ToolRenderContext {
        let args = self
            .state
            .borrow()
            .rendered_args
            .get(tool_call_id)
            .cloned()
            .unwrap_or(JsValue::Null);
        let state = self.state_for(tool_call_id);
        let call_component = self.state.borrow().rendered_call_components.get(tool_call_id).cloned();
        ToolRenderContext {
            args,
            tool_call_id: tool_call_id.to_owned(),
            invalidate: Arc::new(|| {}),
            last_component,
            call_component,
            state,
            cwd: self.cwd.clone(),
            execution_started: true,
            args_complete: true,
            is_partial,
            expanded,
            show_images: false,
            is_error,
        }
    }
}

fn is_blank_rendered_line(line: &JsString) -> bool {
    strip_sgr_sequences(line).trim().is_empty()
}

fn trim_rendered_result_lines(lines: Vec<JsString>) -> Vec<JsString> {
    let mut start = 0;
    let mut end = lines.len();
    while start < end && is_blank_rendered_line(&lines[start]) {
        start += 1;
    }
    while end > start && is_blank_rendered_line(&lines[end - 1]) {
        end -= 1;
    }
    lines[start..end].to_vec()
}

fn render_component(component: &ComponentRef, width: usize) -> Vec<JsString> {
    component.borrow_mut().render(width)
}

impl ToolHtmlRenderer for ExtensionToolHtmlRenderer {
    fn render_call(&self, tool_call_id: &str, tool_name: &str, args: &JsValue) -> Option<JsString> {
        self.state
            .borrow_mut()
            .rendered_args
            .insert(tool_call_id.to_owned(), args.clone());
        let tool_definition = (self.get_tool_definition)(tool_name)?;
        let render_call = tool_definition.render_call?;

        let last_component = self.state.borrow().rendered_call_components.get(tool_call_id).cloned();
        let context = self.render_context(tool_call_id, last_component, false, true, false);
        let component = match render_call(args, self.theme.as_ref(), &context) {
            Ok(component) => component,
            Err(_) => return None,
        };
        self.state
            .borrow_mut()
            .rendered_call_components
            .insert(tool_call_id.to_owned(), Rc::clone(&component));
        let lines = render_component(&component, self.width);
        Some(ansi_lines_to_html(&lines))
    }

    fn render_result(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        result: &[TextOrImageContent],
        details: Option<&JsValue>,
        is_error: bool,
    ) -> Option<RenderedToolResult> {
        let tool_definition = (self.get_tool_definition)(tool_name)?;
        let render_result = tool_definition.render_result?;

        let agent_tool_result = AgentToolResult {
            content: result.to_vec(),
            details: details.cloned(),
            is_error: Some(is_error),
            terminate: None,
        };

        let last_component = self
            .state
            .borrow()
            .rendered_result_components
            .get(tool_call_id)
            .cloned();
        let context = self.render_context(tool_call_id, last_component, false, false, is_error);
        let collapsed_component = match render_result(
            &agent_tool_result,
            &ToolRenderResultOptions {
                expanded: false,
                is_partial: false,
            },
            self.theme.as_ref(),
            &context,
        ) {
            Ok(component) => component,
            Err(_) => return None,
        };
        self.state
            .borrow_mut()
            .rendered_result_components
            .insert(tool_call_id.to_owned(), Rc::clone(&collapsed_component));
        let collapsed = ansi_lines_to_html(&trim_rendered_result_lines(render_component(
            &collapsed_component,
            self.width,
        )));

        let last_component = self
            .state
            .borrow()
            .rendered_result_components
            .get(tool_call_id)
            .cloned();
        let context = self.render_context(tool_call_id, last_component, true, false, is_error);
        let expanded_component = match render_result(
            &agent_tool_result,
            &ToolRenderResultOptions {
                expanded: true,
                is_partial: false,
            },
            self.theme.as_ref(),
            &context,
        ) {
            Ok(component) => component,
            Err(_) => return None,
        };
        self.state
            .borrow_mut()
            .rendered_result_components
            .insert(tool_call_id.to_owned(), Rc::clone(&expanded_component));
        let expanded = ansi_lines_to_html(&trim_rendered_result_lines(render_component(
            &expanded_component,
            self.width,
        )));

        Some(RenderedToolResult {
            collapsed: (!collapsed.is_empty() && collapsed != expanded).then_some(collapsed),
            expanded: Some(expanded),
        })
    }
}

/// Create a tool HTML renderer from extension tool definitions.
pub fn create_tool_html_renderer(deps: ToolHtmlRendererDeps) -> Rc<dyn ToolHtmlRenderer> {
    Rc::new(ExtensionToolHtmlRenderer {
        get_tool_definition: deps.get_tool_definition,
        theme: deps.theme,
        cwd: deps.cwd,
        width: deps.width.unwrap_or(100),
        completion_executor: deps.completion_executor,
        state: RefCell::new(RendererState {
            rendered_call_components: HashMap::new(),
            rendered_result_components: HashMap::new(),
            rendered_states: HashMap::new(),
            rendered_args: HashMap::new(),
        }),
    })
}
