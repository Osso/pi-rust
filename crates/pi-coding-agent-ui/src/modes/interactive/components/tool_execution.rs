//! `coding-agent/src/modes/interactive/components/tool-execution.ts`.
//!
//! Frozen tool shells refresh on explicit updates; elapsed labels are not interval-driven.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pi_agent_core::AgentToolResult;
use pi_ai::types::TextOrImageContent;
use pi_js_compat::date::js_now_millis;
use pi_js_compat::{
    js_format,
    js_json::{JsString, JsValue},
};
use pi_tui::tui::{Component, ComponentRef, Container};
use pi_tui::{Box as BoxComponent, Image, ImageOptions, ImageProtocol, ImageTheme, Spacer, Text, get_capabilities};

use crate::core::extensions::types::{RenderShell, ToolDefinition, ToolRenderContext, ToolRenderResultOptions};
use crate::core::tools::render_utils;
use crate::core::tools::{ToolsOptions, create_all_tool_definitions};
use crate::modes::interactive::theme::{ThemeBg, ThemeColor, with_theme};
use crate::utils::image_convert::{ConvertedImage, convert_to_png};
use pi_js_compat::js_json;

use super::elapsed_time::format_elapsed_duration;

const MAX_TOOL_OUTPUT_LINES: usize = 100;

/// `ToolExecutionOptions`.
#[derive(Debug, Clone, Default)]
pub struct ToolExecutionOptions {
    pub show_images: Option<bool>,
    pub image_width_cells: Option<f64>,
    pub hide_output: Option<bool>,
}

/// `ToolExecutionStartOptions`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ToolExecutionStartOptions {
    pub show_live_elapsed: bool,
}

/// Result payload supplied by the session (`updateResult`).
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub content: Vec<TextOrImageContent>,
    pub details: Option<JsValue>,
    pub is_error: bool,
}

fn limit_tool_output_lines(lines: &[JsString]) -> Vec<JsString> {
    if lines.len() <= MAX_TOOL_OUTPUT_LINES {
        return lines.to_vec();
    }
    let head_line_count = MAX_TOOL_OUTPUT_LINES / 2;
    let tail_line_count = MAX_TOOL_OUTPUT_LINES - head_line_count;
    let hidden_line_count = lines.len() - MAX_TOOL_OUTPUT_LINES;
    let mut limited = lines[..head_line_count].to_vec();
    limited.push(js_format!("... ({} more lines hidden)", hidden_line_count));
    limited.extend_from_slice(&lines[lines.len() - tail_line_count..]);
    limited
}

/// `limitToolResultContent`.
pub fn limit_tool_result_content(content: &[TextOrImageContent]) -> Vec<TextOrImageContent> {
    let text_block_indexes = content
        .iter()
        .enumerate()
        .filter_map(|(index, item)| matches!(item, TextOrImageContent::Text(_)).then_some(index))
        .collect::<Vec<_>>();
    let Some(first_text_block_index) = text_block_indexes.first().copied() else {
        return content.to_vec();
    };
    let joined = JsString::join(
        text_block_indexes.iter().filter_map(|index| match &content[*index] {
            TextOrImageContent::Text(text) => Some(&text.text),
            TextOrImageContent::Image(_) => None,
        }),
        "\n",
    );
    let output_lines = joined.split('\n').collect::<Vec<_>>();
    if output_lines.len() <= MAX_TOOL_OUTPUT_LINES {
        return content.to_vec();
    }
    let limited_text = JsString::join(limit_tool_output_lines(&output_lines), "\n");
    content
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            TextOrImageContent::Text(text) if index == first_text_block_index => {
                let mut text = text.clone();
                text.text = limited_text.clone();
                Some(TextOrImageContent::Text(text))
            }
            TextOrImageContent::Text(_) => None,
            TextOrImageContent::Image(_) => Some(item.clone()),
        })
        .collect()
}

fn into_component_ref<T: Component + 'static>(component: Rc<RefCell<T>>) -> ComponentRef {
    component
}

fn background_fn(background: ThemeBg) -> pi_tui::components::text::StyleFn {
    Rc::new(move |text: &JsString| with_theme(|theme| theme.bg(background, text)))
}

fn image_blocks(content: &[TextOrImageContent]) -> Vec<&pi_ai::types::ImageContent> {
    content
        .iter()
        .filter_map(|item| match item {
            TextOrImageContent::Image(image) => Some(image),
            TextOrImageContent::Text(_) => None,
        })
        .collect()
}

/// Interactive tool call/result block with renderer, fallback, and timer support.
pub struct ToolExecutionComponent {
    self_weak: Weak<RefCell<Self>>,
    container: Container,
    content_box: Rc<RefCell<BoxComponent>>,
    content_text: Rc<RefCell<Text>>,
    self_render_container: Rc<RefCell<Container>>,
    call_renderer_component: Option<ComponentRef>,
    result_renderer_component: Option<ComponentRef>,
    renderer_state: crate::core::extensions::types::ToolRenderState,
    image_components: Vec<ComponentRef>,
    image_spacers: Vec<ComponentRef>,
    tool_name: String,
    tool_call_id: String,
    args: JsValue,
    expanded: bool,
    show_images: bool,
    image_width_cells: f64,
    hide_output: bool,
    show_live_elapsed: bool,
    is_partial: bool,
    tool_definition: Option<ToolDefinition>,
    built_in_tool_definition: Option<ToolDefinition>,
    request_render: Arc<dyn Fn() + Send + Sync>,
    cwd: JsString,
    execution_started: bool,
    execution_started_at: Option<f64>,
    execution_finished_at: Option<f64>,
    args_complete: bool,
    result: Option<ToolExecutionResult>,
    converted_images: HashMap<usize, ConvertedImage>,
    hide_component: bool,
    invalidate_requested: Arc<AtomicBool>,
}

impl ToolExecutionComponent {
    pub fn new(
        tool_name: impl Into<String>,
        tool_call_id: impl Into<String>,
        args: JsValue,
        options: ToolExecutionOptions,
        tool_definition: Option<ToolDefinition>,
        request_render: Arc<dyn Fn() + Send + Sync>,
        cwd: impl Into<JsString>,
    ) -> Rc<RefCell<Self>> {
        let tool_name = tool_name.into();
        let cwd = cwd.into();
        let built_in_tool_definition = create_all_tool_definitions(&cwd, ToolsOptions::default())
            .into_iter()
            .find(|(name, _)| name.as_str() == tool_name)
            .map(|(_, definition)| definition);

        let pending_bg = background_fn(ThemeBg::ToolPendingBg);
        let content_box = Rc::new(RefCell::new(BoxComponent::new(1, 1, Some(Rc::clone(&pending_bg)))));
        let content_text = Rc::new(RefCell::new(Text::with_background("", 1, 1, pending_bg)));
        let self_render_container = Rc::new(RefCell::new(Container::new()));

        let mut component = Self {
            self_weak: Weak::new(),
            container: Container::new(),
            content_box,
            content_text,
            self_render_container,
            call_renderer_component: None,
            result_renderer_component: None,
            renderer_state: crate::core::extensions::types::ToolRenderState::new(),
            image_components: Vec::new(),
            image_spacers: Vec::new(),
            tool_name,
            tool_call_id: tool_call_id.into(),
            args,
            expanded: false,
            show_images: options.show_images.unwrap_or(true),
            image_width_cells: options.image_width_cells.unwrap_or(60.0),
            hide_output: options.hide_output.unwrap_or(false),
            show_live_elapsed: false,
            is_partial: true,
            tool_definition,
            built_in_tool_definition,
            request_render,
            cwd,
            execution_started: false,
            execution_started_at: None,
            execution_finished_at: None,
            args_complete: false,
            result: None,
            converted_images: HashMap::new(),
            hide_component: false,
            invalidate_requested: Arc::new(AtomicBool::new(false)),
        };

        component.container.add_child(Rc::new(RefCell::new(Spacer::new(1))));
        if component.has_renderer_definition() {
            let child = if component.get_render_shell() == RenderShell::SelfRender {
                into_component_ref(Rc::clone(&component.self_render_container))
            } else {
                into_component_ref(Rc::clone(&component.content_box))
            };
            component.container.add_child(child);
        } else {
            component
                .container
                .add_child(into_component_ref(Rc::clone(&component.content_text)));
        }

        let component = Rc::new(RefCell::new(component));
        component.borrow_mut().self_weak = Rc::downgrade(&component);
        component.borrow_mut().update_display();
        component
    }

    fn get_call_renderer(&self) -> Option<crate::core::extensions::types::ToolRenderer> {
        match (&self.built_in_tool_definition, &self.tool_definition) {
            (None, definition) => definition
                .as_ref()
                .and_then(|definition| definition.render_call.clone()),
            (Some(built_in), None) => built_in.render_call.clone(),
            (Some(built_in), Some(definition)) => {
                definition.render_call.clone().or_else(|| built_in.render_call.clone())
            }
        }
    }

    fn get_result_renderer(&self) -> Option<crate::core::extensions::types::ToolResultRenderer> {
        match (&self.built_in_tool_definition, &self.tool_definition) {
            (None, definition) => definition
                .as_ref()
                .and_then(|definition| definition.render_result.clone()),
            (Some(built_in), None) => built_in.render_result.clone(),
            (Some(built_in), Some(definition)) => definition
                .render_result
                .clone()
                .or_else(|| built_in.render_result.clone()),
        }
    }

    fn has_renderer_definition(&self) -> bool {
        self.built_in_tool_definition.is_some() || self.tool_definition.is_some()
    }

    fn get_render_shell(&self) -> RenderShell {
        match (&self.built_in_tool_definition, &self.tool_definition) {
            (None, definition) => definition
                .as_ref()
                .and_then(|definition| definition.render_shell)
                .unwrap_or(RenderShell::Default),
            (Some(built_in), None) => built_in.render_shell.unwrap_or(RenderShell::Default),
            (Some(built_in), Some(definition)) => definition
                .render_shell
                .or(built_in.render_shell)
                .unwrap_or(RenderShell::Default),
        }
    }

    pub fn set_render_completion_executor(
        &self,
        executor: Option<crate::core::extensions::types::ToolRenderCompletionExecutor>,
    ) {
        self.renderer_state.set_completion_executor(executor);
    }

    fn get_render_context(&self, last_component: Option<ComponentRef>) -> ToolRenderContext {
        let invalidate_requested = Arc::clone(&self.invalidate_requested);
        let request_render = Arc::clone(&self.request_render);
        ToolRenderContext {
            args: self.args.clone(),
            tool_call_id: self.tool_call_id.clone(),
            invalidate: Arc::new(move || {
                invalidate_requested.store(true, Ordering::SeqCst);
                request_render();
            }),
            last_component,
            call_component: self.call_renderer_component.clone(),
            state: self.renderer_state.clone(),
            cwd: self.cwd.clone(),
            execution_started: self.execution_started,
            args_complete: self.args_complete,
            is_partial: self.is_partial,
            expanded: self.expanded,
            show_images: self.show_images,
            is_error: self.result.as_ref().is_some_and(|result| result.is_error),
        }
    }

    fn create_call_fallback(&self) -> ComponentRef {
        Rc::new(RefCell::new(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::ToolTitle, theme.bold(&self.tool_name))),
            0,
            0,
        )))
    }

    fn format_timer_text(&self) -> Option<JsString> {
        let started_at = self.execution_started_at?;
        let live_finished_at =
            (self.is_partial && (self.result.is_some() || self.show_live_elapsed)).then(js_now_millis);
        let finished_at = self.execution_finished_at.or(live_finished_at)?;
        Some(js_format!(
            "Elapsed: {}",
            format_elapsed_duration(finished_at - started_at)
        ))
    }

    fn format_completion_time(&self) -> Option<JsString> {
        self.execution_finished_at.map(|millis| {
            pi_js_compat::date::to_locale_string(millis, pi_js_compat::date::LocaleDateFormat::TimeEnGb).into()
        })
    }

    fn create_timer_component(&self) -> Option<ComponentRef> {
        let timer_text = JsString::join(
            [self.format_timer_text(), self.format_completion_time()]
                .into_iter()
                .flatten(),
            "\n",
        );
        if timer_text.is_empty() {
            return None;
        }
        Some(Rc::new(RefCell::new(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::Muted, &timer_text)),
            0,
            0,
        ))))
    }

    fn get_text_output(&self, content: &[TextOrImageContent]) -> JsString {
        let result = AgentToolResult {
            content: content.to_vec(),
            details: None,
            is_error: None,
            terminate: None,
        };
        render_utils::get_text_output(Some(&result), self.show_images)
    }

    fn create_result_fallback(&self) -> Option<ComponentRef> {
        let output = self.get_text_output(&self.get_display_content());
        if output.is_empty() {
            return None;
        }
        Some(Rc::new(RefCell::new(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::ToolOutput, &output)),
            0,
            0,
        ))))
    }

    pub fn update_args(&mut self, args: JsValue) {
        self.args = args;
        self.update_display();
    }

    pub fn mark_execution_started(&mut self, started_at: f64, options: ToolExecutionStartOptions) {
        self.execution_started = true;
        self.show_live_elapsed = options.show_live_elapsed;
        self.execution_started_at = Some(started_at);
        self.execution_finished_at = None;
        self.update_display();
        (self.request_render)();
    }

    pub fn set_args_complete(&mut self) {
        self.args_complete = true;
        self.update_display();
        (self.request_render)();
    }

    pub fn update_result(&mut self, result: ToolExecutionResult, is_partial: bool, finished_at: Option<f64>) {
        self.result = Some(result);
        self.is_partial = is_partial;
        if !is_partial {
            self.execution_finished_at = Some(finished_at.unwrap_or_else(js_now_millis));
        }
        self.update_display();
        self.maybe_convert_images_for_kitty();
    }

    fn maybe_convert_images_for_kitty(&mut self) {
        let capabilities = get_capabilities();
        if capabilities.images != Some(ImageProtocol::Kitty) {
            return;
        }
        let Some(result) = &self.result else {
            return;
        };
        let images = image_blocks(&result.content)
            .into_iter()
            .map(|image| (image.data.clone(), image.mime_type.clone()))
            .collect::<Vec<_>>();
        for (index, (data, mime_type)) in images.iter().enumerate() {
            if data.is_empty() || mime_type.is_empty() {
                continue;
            }
            if mime_type == "image/png" {
                continue;
            }
            if self.converted_images.contains_key(&index) {
                continue;
            }
            let component = self
                .self_weak
                .upgrade()
                .expect("tool row was initialized by its constructor");
            let data = data.clone();
            let mime_type = mime_type.clone();
            tokio::task::spawn_local(async move {
                // The source awaits Photon before decoding, even when its module is cached.
                tokio::task::yield_now().await;
                if let Some(converted) = convert_to_png(&data, &mime_type) {
                    let request_render = {
                        let mut component = component.borrow_mut();
                        component.converted_images.insert(index, converted);
                        component.update_display();
                        component.request_render.clone()
                    };
                    request_render();
                }
            });
        }
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
        self.update_display();
    }

    pub fn set_show_images(&mut self, show: bool) {
        self.show_images = show;
        self.update_display();
    }

    pub fn set_hide_output(&mut self, hide: bool) {
        self.hide_output = hide;
        self.update_display();
    }

    pub fn set_image_width_cells(&mut self, width: f64) {
        self.image_width_cells = pi_js_compat::number::js_math_max(1.0, width.floor());
        self.update_display();
    }

    fn render_self_timer(&self, width: usize) -> Vec<JsString> {
        self.create_timer_component()
            .map(|component| component.borrow_mut().render(width))
            .unwrap_or_default()
    }

    fn render_self_shell(&mut self, width: usize) -> Vec<JsString> {
        let content_lines = self.self_render_container.borrow_mut().render(width);
        if content_lines.is_empty() && self.image_components.is_empty() {
            return Vec::new();
        }
        let mut lines = if content_lines.is_empty() {
            Vec::new()
        } else {
            let mut lines = vec![JsString::default()];
            lines.extend(content_lines);
            lines.extend(self.render_self_timer(width));
            lines
        };
        for index in 0..self.image_components.len() {
            if let Some(spacer) = self.image_spacers.get(index) {
                lines.extend(spacer.borrow_mut().render(width));
            }
            if let Some(image) = self.image_components.get(index) {
                lines.extend(image.borrow_mut().render(width));
            }
        }
        lines
    }

    fn get_display_content(&self) -> Vec<TextOrImageContent> {
        let content = self
            .result
            .as_ref()
            .map(|result| result.content.as_slice())
            .unwrap_or_default();
        limit_tool_result_content(content)
    }

    fn format_tool_execution(&self) -> JsString {
        let mut text = with_theme(|theme| theme.fg(ThemeColor::ToolTitle, theme.bold(&self.tool_name)));
        let content = js_json::stringify_lossless_pretty(&self.args);
        if !content.is_empty() {
            text.push_str("\n\n");
            text.push_str(&content);
        }
        if let Some(timer_text) = self.format_timer_text() {
            text.push_str("\n");
            text.push_str(&timer_text);
        }
        if let Some(completion) = self.format_completion_time() {
            text.push_str("\n");
            text.push_str(with_theme(|theme| theme.fg(ThemeColor::Muted, completion)));
        }
        if !self.hide_output {
            let output = self.get_text_output(&self.get_display_content());
            if !output.is_empty() {
                text.push_str("\n");
                text.push_str(&output);
            }
        }
        text
    }

    fn update_display(&mut self) {
        let background = if self.is_partial {
            ThemeBg::ToolPendingBg
        } else if self.result.as_ref().is_some_and(|result| result.is_error) {
            ThemeBg::ToolErrorBg
        } else {
            ThemeBg::ToolSuccessBg
        };
        let bg_fn = background_fn(background);

        // Both renderer and fallback paths always produce content (source sets it in every branch).
        let has_content = true;
        self.hide_component = false;
        if self.has_renderer_definition() {
            let shell = self.get_render_shell();
            if shell != RenderShell::SelfRender {
                self.content_box.borrow_mut().set_bg_fn(Some(Rc::clone(&bg_fn)));
            }
            if shell == RenderShell::SelfRender {
                self.self_render_container.borrow_mut().clear();
            } else {
                self.content_box.borrow_mut().clear();
            }

            let call_renderer = self.get_call_renderer();
            match call_renderer {
                None => {
                    self.add_render_child(self.create_call_fallback());
                }
                Some(call_renderer) => {
                    let context = self.get_render_context(self.call_renderer_component.clone());
                    match call_renderer(&self.args, &with_theme(Clone::clone), &context) {
                        Ok(component) => {
                            self.call_renderer_component = Some(Rc::clone(&component));
                            self.add_render_child(component);
                        }
                        Err(_) => {
                            self.call_renderer_component = None;
                            self.add_render_child(self.create_call_fallback());
                        }
                    }
                }
            }

            if shell != RenderShell::SelfRender
                && let Some(timer_component) = self.create_timer_component()
            {
                self.add_render_child(timer_component);
            }

            if self.result.is_some() {
                // Hidden renderers still own and update their live renderer state.
                if let Some(component) = self.render_result_component()
                    && !self.hide_output
                {
                    self.add_render_child(component);
                }
            }
        } else {
            self.content_text.borrow_mut().set_custom_bg_fn(Some(bg_fn));
            self.content_text.borrow_mut().set_text(self.format_tool_execution());
        }

        for image in std::mem::take(&mut self.image_components) {
            self.container.remove_child(&image);
        }
        for spacer in std::mem::take(&mut self.image_spacers) {
            self.container.remove_child(&spacer);
        }

        if let Some(result) = &self.result
            && !self.hide_output
        {
            let capabilities = get_capabilities();
            let images = image_blocks(&result.content);
            for (index, image) in images.iter().enumerate() {
                if capabilities.images.is_some()
                    && self.show_images
                    && !image.data.is_empty()
                    && !image.mime_type.is_empty()
                {
                    let converted = self.converted_images.get(&index);
                    let image_data = converted.map_or(image.data.as_str(), |converted| converted.data.as_str());
                    let image_mime_type =
                        converted.map_or(image.mime_type.as_str(), |converted| converted.mime_type.as_str());
                    if capabilities.images == Some(ImageProtocol::Kitty) && image_mime_type != "image/png" {
                        continue;
                    }

                    let spacer: ComponentRef = Rc::new(RefCell::new(Spacer::new(1)));
                    self.container.add_child(Rc::clone(&spacer));
                    self.image_spacers.push(spacer);
                    let fallback_theme = Rc::new(ImageTheme {
                        fallback_color: Rc::new(|text: &JsString| {
                            with_theme(|theme| theme.fg(ThemeColor::ToolOutput, text))
                        }),
                    });
                    let image_component: ComponentRef = Rc::new(RefCell::new(Image::new(
                        image_data,
                        image_mime_type,
                        fallback_theme,
                        ImageOptions {
                            max_width_cells: Some(self.image_width_cells),
                            ..Default::default()
                        },
                        None,
                    )));
                    self.image_components.push(Rc::clone(&image_component));
                    self.container.add_child(image_component);
                }
            }
        }

        if self.has_renderer_definition() && !has_content && self.image_components.is_empty() {
            self.hide_component = true;
        }
    }

    fn render_result_component(&mut self) -> Option<ComponentRef> {
        let Some(result_renderer) = self.get_result_renderer() else {
            return self.create_result_fallback();
        };
        let result = self.result.as_ref()?;
        let rendered_result = AgentToolResult {
            content: self.get_display_content(),
            details: result.details.clone(),
            is_error: None,
            terminate: None,
        };
        let options = ToolRenderResultOptions {
            expanded: self.expanded,
            is_partial: self.is_partial,
        };
        let context = self.get_render_context(self.result_renderer_component.clone());
        match result_renderer(&rendered_result, &options, &with_theme(Clone::clone), &context) {
            Ok(component) => {
                self.result_renderer_component = Some(component.clone());
                Some(component)
            }
            Err(_) => {
                self.result_renderer_component = None;
                self.create_result_fallback()
            }
        }
    }

    fn add_render_child(&mut self, component: ComponentRef) {
        if self.get_render_shell() == RenderShell::SelfRender {
            self.self_render_container.borrow_mut().add_child(component);
        } else {
            self.content_box.borrow_mut().add_child(component);
        }
    }
}

impl Deref for ToolExecutionComponent {
    type Target = Container;

    fn deref(&self) -> &Self::Target {
        &self.container
    }
}

impl DerefMut for ToolExecutionComponent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.container
    }
}

impl Component for ToolExecutionComponent {
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        if self.invalidate_requested.swap(false, Ordering::SeqCst) {
            self.update_display();
        }
        if self.hide_component {
            return Vec::new();
        }
        if self.has_renderer_definition() && self.get_render_shell() == RenderShell::SelfRender {
            return self.render_self_shell(width);
        }
        self.container.render(width)
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
        self.update_display();
    }

    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
}

#[cfg(test)]
mod lossless_tests {
    use super::*;
    use pi_js_compat::js_json::JsString;
    #[test]
    fn output_limit_retains_surrogates_and_original_result() {
        let raw = JsString::join((0..101).map(|_| JsString::from_utf16(vec![0xd800])), "\n");
        let original = vec![TextOrImageContent::Text(pi_ai::types::TextContent::new(raw.clone()))];
        let output = limit_tool_result_content(&original);
        let TextOrImageContent::Text(text) = &output[0] else {
            panic!("text")
        };
        assert_eq!(text.text.units()[0], 0xd800);
        assert!(text.text.contains("... (1 more lines hidden)"));
        assert_eq!(
            original[0],
            TextOrImageContent::Text(pi_ai::types::TextContent::new(raw))
        );
    }
}
