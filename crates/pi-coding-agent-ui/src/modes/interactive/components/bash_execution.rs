//! Ported from `coding-agent/src/modes/interactive/components/bash-execution.ts`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pi_tui::components::dynamic_border::DynamicBorder;
use pi_tui::components::loader::{Loader, LoaderIndicatorOptions};
use pi_tui::components::spacer::Spacer;
use pi_tui::components::text::Text;
use pi_tui::tui::{Component, ComponentRef, Container, component};

use super::elapsed_time::{MIN_VISIBLE_ELAPSED_MS, format_elapsed_duration};
use super::keybinding_hints::{key_hint, key_text};
use super::visual_truncate::truncate_to_visual_lines;
use crate::core::tools::truncate::{
    DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, TruncationOptions, TruncationResult, truncate_tail,
};
use crate::modes::interactive::theme::{ThemeColor, with_theme};
use crate::utils::ansi::strip_ansi;
use pi_js_compat::{js_format, js_json::JsString};

const BASH_TIMER_INTERVAL_MS: u64 = 1000;
const PREVIEW_LINES: usize = 20;

/// `BashExecutionOptions`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BashExecutionOptions {
    pub show_elapsed: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BashStatus {
    Running,
    Complete,
    Cancelled,
    Error,
}

/// Displays a bash command execution with streaming output.
pub struct BashExecutionComponent {
    command: JsString,
    output_lines: Vec<JsString>,
    status: BashStatus,
    exit_code: Option<f64>,
    loader: ComponentRef,
    truncation_result: Option<TruncationResult>,
    full_output_path: Option<JsString>,
    expanded: bool,
    content_container: ComponentRef,
    started_at: Instant,
    finished_at: Option<Instant>,
    next_timer_tick: Option<Instant>,
    request_render: Rc<dyn Fn()>,
    show_elapsed: bool,
    container: Container,
}

impl BashExecutionComponent {
    pub fn new(
        command: impl Into<JsString>,
        request_render: Rc<dyn Fn()>,
        exclude_from_context: bool,
        options: BashExecutionOptions,
    ) -> Self {
        let command = command.into();
        let show_elapsed = options.show_elapsed.unwrap_or(true);
        let color_key = if exclude_from_context {
            ThemeColor::Dim
        } else {
            ThemeColor::BashMode
        };
        let border_color: pi_tui::components::dynamic_border::BorderFormatter =
            Rc::new(move |text: &JsString| with_theme(|theme| theme.fg(color_key, text)));

        let mut container = Container::new();
        container.add_child(component(Spacer::new(1)));
        container.add_child(component(DynamicBorder::new(Rc::clone(&border_color))));

        let content_container = component(Container::new());
        container.add_child(Rc::clone(&content_container));

        let loader = Loader::new(
            Rc::clone(&request_render),
            Rc::new(move |text: &JsString| with_theme(|theme| theme.fg(color_key, text))),
            Rc::new(|text: &JsString| with_theme(|theme| theme.fg(ThemeColor::Muted, text))),
            js_format!("Running... ({} to cancel)", key_text("tui.select.cancel")),
            Some(LoaderIndicatorOptions {
                frames: Some(Vec::new()),
                interval_ms: None,
            }),
        );
        let loader = component(loader);

        container.add_child(component(DynamicBorder::new(border_color)));

        let started_at = Instant::now();
        let next_timer_tick = show_elapsed.then(|| started_at + Duration::from_millis(BASH_TIMER_INTERVAL_MS));

        let mut component = Self {
            command,
            output_lines: Vec::new(),
            status: BashStatus::Running,
            exit_code: None,
            loader,
            truncation_result: None,
            full_output_path: None,
            expanded: false,
            content_container,
            started_at,
            finished_at: None,
            next_timer_tick,
            request_render,
            show_elapsed,
            container,
        };
        component.update_display();
        component
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
        self.update_display();
    }

    pub fn append_output(&mut self, chunk: impl Into<JsString>) {
        let chunk = chunk.into();
        let clean = strip_ansi(&chunk).replace("\r\n", "\n").replace("\r", "\n");
        let new_lines: Vec<JsString> = clean.split('\n').collect();
        if self.output_lines.is_empty() {
            self.output_lines.extend(new_lines);
        } else {
            let first = new_lines.first().cloned().unwrap_or_default();
            if let Some(last) = self.output_lines.last_mut() {
                last.push_str(&first);
            }
            self.output_lines.extend(new_lines.into_iter().skip(1));
        }
        self.update_display();
    }

    pub fn set_complete(
        &mut self,
        exit_code: Option<f64>,
        cancelled: bool,
        truncation_result: Option<TruncationResult>,
        full_output_path: Option<JsString>,
    ) {
        self.exit_code = exit_code;
        self.status = if cancelled {
            BashStatus::Cancelled
        } else if exit_code.is_some_and(|code| code != 0.0) {
            BashStatus::Error
        } else {
            BashStatus::Complete
        };
        self.truncation_result = truncation_result;
        self.full_output_path = full_output_path;
        self.finished_at = Some(Instant::now());
        self.stop_loader();
        self.stop_timer();
        self.update_display();
    }

    /// Advance the one-second elapsed-time timer when due.
    pub fn poll_timer(&mut self, now: Instant) -> bool {
        let Some(mut next_tick) = self.next_timer_tick else {
            return false;
        };
        let mut ticked = false;
        while now >= next_tick {
            self.update_display();
            (self.request_render)();
            ticked = true;
            next_tick += Duration::from_millis(BASH_TIMER_INTERVAL_MS);
        }
        self.next_timer_tick = Some(next_tick);
        ticked
    }

    pub fn next_timer_deadline(&self) -> Option<Instant> {
        self.next_timer_tick
    }

    fn stop_loader(&mut self) {
        if let Some(loader) = self
            .loader
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Loader>())
        {
            loader.stop();
        }
    }

    fn stop_timer(&mut self) {
        self.next_timer_tick = None;
    }

    fn format_timer(&self) -> Option<JsString> {
        let end_time = self.finished_at.unwrap_or_else(Instant::now);
        let elapsed_ms = end_time.duration_since(self.started_at).as_secs_f64() * 1000.0;
        if elapsed_ms < MIN_VISIBLE_ELAPSED_MS {
            return None;
        }
        Some(format_elapsed_duration(elapsed_ms).into())
    }

    pub fn dispose(&mut self) {
        self.stop_loader();
        self.stop_timer();
    }

    fn update_display(&mut self) {
        let full_output = JsString::join(&self.output_lines, "\n");
        let context_truncation = truncate_tail(
            &full_output,
            TruncationOptions {
                max_lines: DEFAULT_MAX_LINES as f64,
                max_bytes: DEFAULT_MAX_BYTES as f64,
            },
        );
        let available_lines: Vec<JsString> = if context_truncation.content.is_empty() {
            Vec::new()
        } else {
            context_truncation.content.split('\n').collect()
        };
        let preview_start = available_lines.len().saturating_sub(PREVIEW_LINES);
        let preview_logical_lines = &available_lines[preview_start..];
        let hidden_line_count = available_lines.len() - preview_logical_lines.len();

        let mut children: Vec<ComponentRef> = Vec::new();
        children.push(component(Text::new(
            with_theme(|theme| theme.fg(ThemeColor::BashMode, theme.bold(&js_format!("$ {}", &self.command)))),
            1,
            0,
        )));

        if !available_lines.is_empty() {
            if self.expanded {
                let display_text = JsString::join(
                    available_lines
                        .iter()
                        .map(|line| with_theme(|theme| theme.fg(ThemeColor::Muted, line))),
                    "\n",
                );
                children.push(component(Text::new(js_format!("\n{}", display_text), 1, 0)));
            } else {
                let styled_output = JsString::join(
                    preview_logical_lines
                        .iter()
                        .map(|line| with_theme(|theme| theme.fg(ThemeColor::Muted, line))),
                    "\n",
                );
                children.push(component(VisualPreview::new(
                    js_format!("\n{}", styled_output),
                    PREVIEW_LINES,
                    1,
                )));
            }
        }

        if self.status == BashStatus::Running {
            let elapsed = self.show_elapsed.then(|| self.format_timer()).flatten();
            let running_prefix = elapsed.map_or_else(
                || JsString::from("Running..."),
                |elapsed| js_format!("Running {}...", elapsed),
            );
            if let Some(loader) = self
                .loader
                .borrow_mut()
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<Loader>())
            {
                loader.set_message(js_format!(
                    "{} ({} to cancel)",
                    running_prefix,
                    key_text("tui.select.cancel")
                ));
            }
            children.push(Rc::clone(&self.loader));
        } else {
            let elapsed = self.show_elapsed.then(|| self.format_timer()).flatten();
            let mut status_parts: Vec<JsString> = elapsed
                .map(|elapsed| {
                    vec![with_theme(|theme| {
                        theme.fg(ThemeColor::Muted, &js_format!("(elapsed {})", elapsed))
                    })]
                })
                .unwrap_or_default();

            if hidden_line_count > 0 {
                if self.expanded {
                    status_parts.push(js_format!(
                        "{}{}{}",
                        with_theme(|theme| theme.fg(ThemeColor::Muted, "(")),
                        key_hint("app.tools.expand", "to collapse"),
                        with_theme(|theme| theme.fg(ThemeColor::Muted, ")"))
                    ));
                } else {
                    status_parts.push(js_format!(
                        "{}{}{}",
                        with_theme(
                            |theme| theme.fg(ThemeColor::Muted, &js_format!("... {} more lines (", hidden_line_count))
                        ),
                        key_hint("app.tools.expand", "to expand"),
                        with_theme(|theme| theme.fg(ThemeColor::Muted, ")"))
                    ));
                }
            }

            match self.status {
                BashStatus::Cancelled => {
                    status_parts.push(with_theme(|theme| theme.fg(ThemeColor::Warning, "(cancelled)")));
                }
                BashStatus::Error => {
                    status_parts.push(with_theme(|theme| {
                        theme.fg(
                            ThemeColor::Error,
                            js_format!(
                                "(exit {})",
                                pi_js_compat::number::js_number_to_string(self.exit_code.unwrap_or_default())
                            ),
                        )
                    }));
                }
                _ => {}
            }

            let was_truncated =
                self.truncation_result.as_ref().is_some_and(|result| result.truncated) || context_truncation.truncated;
            if was_truncated && let Some(full_output_path) = &self.full_output_path {
                status_parts.push(with_theme(|theme| {
                    theme.fg(
                        ThemeColor::Warning,
                        &js_format!("Output truncated. Full output: {}", full_output_path),
                    )
                }));
            }

            if !status_parts.is_empty() {
                children.push(component(Text::new(
                    js_format!("\n{}", JsString::join(status_parts, "\n")),
                    1,
                    0,
                )));
            }
        }

        if let Some(content) = self
            .content_container
            .borrow_mut()
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<Container>())
        {
            content.clear();
            for child in children {
                content.add_child(child);
            }
        }
    }

    /// Raw output used to build the `BashExecutionMessage`.
    pub fn get_output(&self) -> JsString {
        JsString::join(&self.output_lines, "\n")
    }

    pub fn get_command(&self) -> &JsString {
        &self.command
    }
}

/// Width-aware visual preview matching the source's cached render closure.
struct VisualPreview {
    input: JsString,
    max_lines: usize,
    padding_x: usize,
    cache: RefCell<Option<(usize, Vec<JsString>)>>,
}

impl VisualPreview {
    fn new(input: JsString, max_lines: usize, padding_x: usize) -> Self {
        Self {
            input,
            max_lines,
            padding_x,
            cache: RefCell::new(None),
        }
    }
}

impl Component for VisualPreview {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut cache = self.cache.borrow_mut();
        if let Some((cached_width, lines)) = cache.as_ref()
            && *cached_width == width
        {
            return lines.clone();
        }
        let result = truncate_to_visual_lines(&self.input, self.max_lines, width, self.padding_x);
        *cache = Some((width, result.visual_lines.clone()));
        result.visual_lines
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        *self.cache.borrow_mut() = None;
    }
}

impl Component for BashExecutionComponent {
    fn dispose(&mut self) {
        BashExecutionComponent::dispose(self);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.container.render(width)
    }

    fn invalidate(&mut self) {
        self.container.invalidate();
        self.update_display();
    }
}
