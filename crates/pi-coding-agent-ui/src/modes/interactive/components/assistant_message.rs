//! Frozen `coding-agent/src/modes/interactive/components/assistant-message.ts`.

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use pi_ai::types::{AssistantContent, AssistantMessage, StopReason};
use pi_js_compat::{js_format, js_json::JsString};
use pi_tui::tui::ComponentRef;
use pi_tui::{Component, Container, Markdown, MarkdownOptions, MarkdownTheme, Spacer, Text};

use crate::core::assistant_narration::is_server_narration_signature;
use crate::core::settings_manager::MermaidRenderingMode;
use crate::modes::interactive::theme::{ThemeColor, get_markdown_theme, with_theme};

const OSC133_ZONE_START: &str = "\x1b]133;A\x07";
const OSC133_ZONE_END: &str = "\x1b]133;B\x07";
const OSC133_ZONE_FINAL: &str = "\x1b]133;C\x07";

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContentType {
    Text,
    Thinking,
    ToolCall,
}

enum SlotComponent {
    Markdown(Rc<RefCell<Markdown>>),
    Text(Rc<RefCell<Text>>),
}
impl SlotComponent {
    fn component_ref(&self) -> ComponentRef {
        match self {
            Self::Markdown(component) => component.clone(),
            Self::Text(component) => component.clone(),
        }
    }
}
struct ContentSlot {
    content_type: ContentType,
    component: Option<SlotComponent>,
    trailing_spacer: Option<Rc<RefCell<Spacer>>>,
    component_text: JsString,
    visible: bool,
    hidden_thinking: bool,
}
struct TrailingStatusSlot {
    spacer: Rc<RefCell<Spacer>>,
    text: Rc<RefCell<Text>>,
    message: JsString,
}

/// Assistant text, thinking and status, retaining unchanged Markdown render state.
pub struct AssistantMessageComponent {
    container: Container,
    content_container: Rc<RefCell<Container>>,
    hide_thinking_block: bool,
    markdown_theme: Rc<dyn MarkdownTheme>,
    hidden_thinking_label: JsString,
    output_pad: usize,
    content_slots: Vec<ContentSlot>,
    leading_spacer: Option<Rc<RefCell<Spacer>>>,
    trailing_status_slot: Option<TrailingStatusSlot>,
    last_message: Option<AssistantMessage>,
    has_tool_calls: bool,
    mermaid_mode_source: Option<Rc<dyn Fn() -> MermaidRenderingMode>>,
    is_streaming: bool,
}

impl AssistantMessageComponent {
    pub fn new(
        message: Option<AssistantMessage>,
        hide_thinking_block: bool,
        markdown_theme: Option<Rc<dyn MarkdownTheme>>,
        hidden_thinking_label: Option<JsString>,
        output_pad: usize,
    ) -> Self {
        let content_container = Rc::new(RefCell::new(Container::new()));
        let content_child: ComponentRef = content_container.clone();
        let mut component = Self {
            container: Container::with_children(vec![content_child]),
            content_container,
            hide_thinking_block,
            markdown_theme: markdown_theme.unwrap_or_else(|| Rc::from(get_markdown_theme())),
            hidden_thinking_label: hidden_thinking_label.unwrap_or_else(|| "Thinking...".into()),
            output_pad,
            content_slots: Vec::new(),
            leading_spacer: None,
            trailing_status_slot: None,
            last_message: None,
            has_tool_calls: false,
            mermaid_mode_source: None,
            is_streaming: false,
        };
        if let Some(message) = message {
            component.update_content(message);
        }
        component
    }

    pub fn set_mermaid_mode_source(&mut self, source: Rc<dyn Fn() -> MermaidRenderingMode>) {
        self.mermaid_mode_source = Some(source);
    }

    pub fn set_streaming(&mut self, is_streaming: bool) {
        self.is_streaming = is_streaming;
    }

    pub fn set_hide_thinking_block(&mut self, hide: bool) {
        self.hide_thinking_block = hide;
        if let Some(message) = self.last_message.clone() {
            self.update_content(message);
        }
    }
    pub fn set_hidden_thinking_label(&mut self, label: impl Into<JsString>) {
        self.hidden_thinking_label = label.into();
        if let Some(message) = self.last_message.clone() {
            self.update_content(message);
        }
    }
    pub fn set_output_pad(&mut self, padding: usize) {
        self.output_pad = padding;
        if let Some(message) = self.last_message.clone() {
            self.update_content(message);
        }
    }

    pub fn update_content(&mut self, message: AssistantMessage) {
        self.last_message = Some(message.clone());
        let mut previous = std::mem::take(&mut self.content_slots).into_iter();
        self.content_slots = message
            .content
            .iter()
            .map(|content| self.reconcile_content_slot(previous.next(), content))
            .collect();
        self.update_thinking_spacers();
        self.has_tool_calls = message
            .content
            .iter()
            .any(|content| matches!(content, AssistantContent::ToolCall(_)));
        self.reconcile_trailing_status(&message);
        self.rebuild_content_children();
    }

    fn reconcile_content_slot(&self, existing: Option<ContentSlot>, content: &AssistantContent) -> ContentSlot {
        match content {
            AssistantContent::Text(content) => {
                self.reconcile_markdown_slot(existing, content.text.trim(), ContentType::Text)
            }
            AssistantContent::Thinking(content)
                if is_server_narration_signature(content.thinking_signature.as_deref()) =>
            {
                self.reconcile_markdown_slot(existing, content.thinking.trim(), ContentType::Text)
            }
            AssistantContent::Thinking(content) if self.hide_thinking_block => {
                self.reconcile_hidden_thinking_slot(existing, content.thinking.trim())
            }
            AssistantContent::Thinking(content) => {
                self.reconcile_markdown_slot(existing, content.thinking.trim(), ContentType::Thinking)
            }
            AssistantContent::ToolCall(_) => ContentSlot {
                content_type: ContentType::ToolCall,
                component: None,
                trailing_spacer: None,
                component_text: JsString::default(),
                visible: false,
                hidden_thinking: false,
            },
        }
    }

    fn reconcile_markdown_slot(
        &self,
        mut existing: Option<ContentSlot>,
        text: JsString,
        kind: ContentType,
    ) -> ContentSlot {
        let same_text = existing.as_ref().is_some_and(|slot| slot.component_text == text);
        let trailing_spacer = take_thinking_spacer(&mut existing, kind);
        let markdown = existing
            .as_mut()
            .filter(|slot| slot.content_type == kind && !slot.hidden_thinking)
            .and_then(|slot| match slot.component.take() {
                Some(SlotComponent::Markdown(component)) => Some(component),
                _ => None,
            });
        let markdown = markdown.or_else(|| (!text.is_empty()).then(|| self.create_markdown(&text, kind)));
        if let Some(markdown) = &markdown {
            let mut markdown = markdown.borrow_mut();
            markdown.set_padding(self.output_pad, 0);
            if !same_text {
                markdown.set_text(text.clone());
            }
        }
        ContentSlot {
            content_type: kind,
            component: markdown.map(SlotComponent::Markdown),
            trailing_spacer,
            visible: !text.is_empty(),
            component_text: text,
            hidden_thinking: false,
        }
    }

    fn create_markdown(&self, text: &JsString, kind: ContentType) -> Rc<RefCell<Markdown>> {
        let style = (kind == ContentType::Thinking).then(|| pi_tui::DefaultTextStyle {
            color: Some(Box::new(|text| {
                with_theme(|theme| theme.fg(ThemeColor::ThinkingText, text))
            })),
            italic: true,
            ..Default::default()
        });
        Rc::new(RefCell::new(Markdown::with_shared_theme(
            text,
            self.output_pad,
            0,
            self.markdown_theme.clone(),
            style,
            MarkdownOptions::default(),
        )))
    }

    fn reconcile_hidden_thinking_slot(&self, mut existing: Option<ContentSlot>, text: JsString) -> ContentSlot {
        let label = with_theme(|theme| theme.italic(theme.fg(ThemeColor::ThinkingText, &self.hidden_thinking_label)));
        let same_label = existing.as_ref().is_some_and(|slot| slot.component_text == label);
        let trailing_spacer = take_thinking_spacer(&mut existing, ContentType::Thinking);
        let hidden_text = existing
            .as_mut()
            .filter(|slot| slot.content_type == ContentType::Thinking && slot.hidden_thinking)
            .and_then(|slot| match slot.component.take() {
                Some(SlotComponent::Text(component)) => Some(component),
                _ => None,
            });
        let hidden_text = hidden_text
            .or_else(|| (!text.is_empty()).then(|| Rc::new(RefCell::new(Text::new(&label, self.output_pad, 0)))));
        if let Some(hidden_text) = &hidden_text {
            let mut hidden_text = hidden_text.borrow_mut();
            hidden_text.set_padding(self.output_pad, 0);
            if !same_label {
                hidden_text.set_text(&label);
            }
        }
        ContentSlot {
            content_type: ContentType::Thinking,
            component: hidden_text.map(SlotComponent::Text),
            trailing_spacer,
            component_text: label,
            visible: !text.is_empty(),
            hidden_thinking: true,
        }
    }

    fn update_thinking_spacers(&mut self) {
        let mut has_visible_after = false;
        for slot in self.content_slots.iter_mut().rev() {
            let needs_spacer = slot.content_type == ContentType::Thinking && slot.visible && has_visible_after;
            if needs_spacer {
                slot.trailing_spacer
                    .get_or_insert_with(|| Rc::new(RefCell::new(Spacer::new(1))));
            } else {
                slot.trailing_spacer = None;
            }
            if slot.visible {
                has_visible_after = true;
            }
        }
    }

    fn reconcile_trailing_status(&mut self, message: &AssistantMessage) {
        let Some(status) = self.status_message(message) else {
            self.trailing_status_slot = None;
            return;
        };
        if let Some(slot) = &mut self.trailing_status_slot {
            slot.text.borrow_mut().set_padding(self.output_pad, 0);
            if slot.message != status {
                slot.text.borrow_mut().set_text(&status);
                slot.message = status;
            }
        } else {
            self.trailing_status_slot = Some(TrailingStatusSlot {
                spacer: Rc::new(RefCell::new(Spacer::new(1))),
                text: Rc::new(RefCell::new(Text::new(&status, self.output_pad, 0))),
                message: status,
            });
        }
    }

    fn status_message(&self, message: &AssistantMessage) -> Option<JsString> {
        if message.stop_reason == StopReason::Length {
            return Some(with_theme(|theme| {
                theme.fg(ThemeColor::Error, "Error: Model stopped because it reached the maximum output token limit. The response may be incomplete.")
            }));
        }
        if self.has_tool_calls {
            return None;
        }
        match message.stop_reason {
            StopReason::Aborted => {
                let error = message
                    .error_message
                    .as_ref()
                    .filter(|error| !error.is_empty() && *error != "Request was aborted");
                Some(with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Error,
                        error.cloned().unwrap_or_else(|| "Operation aborted".into()),
                    )
                }))
            }
            StopReason::Error => {
                let error = message
                    .error_message
                    .as_ref()
                    .filter(|error| !error.is_empty())
                    .cloned()
                    .unwrap_or_else(|| "Unknown error".into());
                Some(with_theme(|theme| {
                    theme.fg(ThemeColor::Error, js_format!("Error: {}", error))
                }))
            }
            _ => None,
        }
    }

    fn rebuild_content_children(&mut self) {
        let mut container = self.content_container.borrow_mut();
        container.clear();
        if self.content_slots.iter().any(|slot| slot.visible) {
            let spacer = self
                .leading_spacer
                .get_or_insert_with(|| Rc::new(RefCell::new(Spacer::new(1))));
            let child: ComponentRef = spacer.clone();
            container.add_child(child);
        }
        for slot in &self.content_slots {
            if !slot.visible {
                continue;
            }
            if let Some(component) = &slot.component {
                container.add_child(component.component_ref());
                if let Some(spacer) = &slot.trailing_spacer {
                    let child: ComponentRef = spacer.clone();
                    container.add_child(child);
                }
            }
        }
        if let Some(slot) = &self.trailing_status_slot {
            let spacer: ComponentRef = slot.spacer.clone();
            let text: ComponentRef = slot.text.clone();
            container.add_child(spacer);
            container.add_child(text);
        }
    }
}

fn take_thinking_spacer(existing: &mut Option<ContentSlot>, kind: ContentType) -> Option<Rc<RefCell<Spacer>>> {
    if kind != ContentType::Thinking {
        return None;
    }
    existing
        .as_mut()
        .filter(|slot| slot.content_type == ContentType::Thinking)
        .and_then(|slot| slot.trailing_spacer.take())
}
impl Deref for AssistantMessageComponent {
    type Target = Container;
    fn deref(&self) -> &Self::Target {
        &self.container
    }
}
impl DerefMut for AssistantMessageComponent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.container
    }
}
impl Component for AssistantMessageComponent {
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
    fn render(&mut self, width: usize) -> Vec<JsString> {
        if let Some(source) = &self.mermaid_mode_source {
            let enabled = match source() {
                MermaidRenderingMode::Off => false,
                MermaidRenderingMode::Final => !self.is_streaming,
                MermaidRenderingMode::Streaming => true,
            };
            for slot in &self.content_slots {
                if let Some(SlotComponent::Markdown(markdown)) = &slot.component {
                    markdown
                        .borrow_mut()
                        .set_mermaid_rendering(enabled && slot.content_type == ContentType::Text, self.is_streaming);
                }
            }
        }
        let mut lines = self.container.render(width);
        if self.has_tool_calls || lines.is_empty() {
            return lines;
        }
        lines[0] = js_format!("{}{}", OSC133_ZONE_START, &lines[0]);
        let last = lines.len() - 1;
        lines[last] = js_format!("{}{}{}", OSC133_ZONE_END, OSC133_ZONE_FINAL, &lines[last]);
        lines
    }
    fn wants_input(&self) -> bool {
        false
    }
    fn invalidate(&mut self) {
        self.container.invalidate();
        if let Some(message) = self.last_message.clone() {
            self.update_content(message);
        }
    }
}
