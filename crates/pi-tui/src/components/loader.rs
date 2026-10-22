//! Ported from `packages/tui/src/components/loader.ts`.
//!
//! The TypeScript loader owns a `setInterval` timer. The Rust port keeps the
//! timer driver-owned (matching `stdin_buffer.rs`): [`Loader::poll`] advances the
//! frame when the caller reports that the deadline passed, so tests are
//! deterministic and no background thread is created.

use std::rc::Rc;
use std::time::{Duration, Instant};

use super::text::{StyleFn, Text};
use pi_js_compat::js_json::JsString;

use crate::tui::Component;

pub const DEFAULT_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const DEFAULT_INTERVAL_MS: u64 = 250;

/// Animation frames and interval override.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoaderIndicatorOptions {
    /// Use an empty array to hide the indicator.
    pub frames: Option<Vec<JsString>>,
    /// Frame interval in milliseconds for animated indicators.
    pub interval_ms: Option<u64>,
}

/// Loader component that updates with an optional spinning animation.
pub struct Loader {
    text: Text,
    frames: Vec<JsString>,
    interval_ms: u64,
    current_frame: usize,
    running: bool,
    next_frame_at: Option<Instant>,
    render_indicator_verbatim: bool,
    spinner_color_fn: StyleFn,
    message_color_fn: StyleFn,
    message: JsString,
    request_render: Rc<dyn Fn()>,
}

impl Loader {
    pub fn new(
        request_render: Rc<dyn Fn()>,
        spinner_color_fn: StyleFn,
        message_color_fn: StyleFn,
        message: impl Into<JsString>,
        indicator: Option<LoaderIndicatorOptions>,
    ) -> Self {
        let mut loader = Self {
            text: Text::new("", 1, 0),
            frames: default_frames(),
            interval_ms: DEFAULT_INTERVAL_MS,
            current_frame: 0,
            running: false,
            next_frame_at: None,
            render_indicator_verbatim: false,
            spinner_color_fn,
            message_color_fn,
            message: message.into(),
            request_render,
        };
        loader.set_indicator(indicator);
        loader
    }

    pub fn message(&self) -> &JsString {
        &self.message
    }

    pub fn frames(&self) -> &[JsString] {
        &self.frames
    }

    pub fn current_frame(&self) -> usize {
        self.current_frame
    }

    pub fn interval_ms(&self) -> u64 {
        self.interval_ms
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.next_frame_at
    }

    pub fn start(&mut self) {
        self.update_display();
        self.restart_animation();
    }

    pub fn stop(&mut self) {
        self.running = false;
        self.next_frame_at = None;
    }

    pub fn set_message(&mut self, message: impl Into<JsString>) {
        self.message = message.into();
        self.update_display();
    }

    pub fn set_indicator(&mut self, indicator: Option<LoaderIndicatorOptions>) {
        self.render_indicator_verbatim = indicator.is_some();
        self.frames = indicator
            .as_ref()
            .and_then(|options| options.frames.clone())
            .unwrap_or_else(default_frames);
        self.interval_ms = indicator
            .as_ref()
            .and_then(|options| options.interval_ms)
            .filter(|interval| *interval > 0)
            .unwrap_or(DEFAULT_INTERVAL_MS);
        self.current_frame = 0;
        self.start();
    }

    /// Advance the animation when `now` reached the scheduled frame deadline.
    ///
    /// Returns whether a new frame was rendered.
    pub fn poll(&mut self, now: Instant) -> bool {
        if !self.running || self.next_frame_at.is_none_or(|deadline| now < deadline) {
            return false;
        }
        self.current_frame = (self.current_frame + 1) % self.frames.len();
        self.next_frame_at = Some(now + Duration::from_millis(self.interval_ms));
        self.update_display();
        true
    }

    fn restart_animation(&mut self) {
        self.stop();
        if self.frames.len() <= 1 {
            return;
        }
        self.running = true;
        self.next_frame_at = Some(Instant::now() + Duration::from_millis(self.interval_ms));
    }

    fn update_display(&mut self) {
        let frame = self.frames.get(self.current_frame).cloned().unwrap_or_default();
        let rendered_frame = if self.render_indicator_verbatim {
            frame.clone()
        } else {
            (self.spinner_color_fn)(&frame)
        };
        let indicator = if frame.is_empty() {
            JsString::default()
        } else {
            rendered_frame + " "
        };
        let message = (self.message_color_fn)(&self.message);
        self.text.set_text(indicator + message);
        (self.request_render)();
    }
}

impl Component for Loader {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let mut lines = vec![JsString::default()];
        lines.extend(self.text.render(width));
        lines
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.text.invalidate();
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

fn default_frames() -> Vec<JsString> {
    DEFAULT_FRAMES.iter().map(|frame| JsString::from(*frame)).collect()
}
