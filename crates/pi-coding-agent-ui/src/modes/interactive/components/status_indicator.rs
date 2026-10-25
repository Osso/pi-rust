//! Ported from `coding-agent/src/modes/interactive/components/status-indicator.ts`.

use pi_js_compat::{js_format, js_json::JsString};
use std::cell::RefCell;
use std::rc::Rc;

use pi_tui::components::loader::{Loader, LoaderIndicatorOptions};
use pi_tui::components::text::StyleFn;
use pi_tui::tui::Component;

use crate::modes::interactive::components::countdown_timer::CountdownTimer;
use crate::modes::interactive::components::keybinding_hints::key_text;
use crate::modes::interactive::theme::{ThemeColor, with_theme};

/// `WorkingIndicatorOptions`: spinner frames and interval.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkingIndicatorOptions {
    pub frames: Option<Vec<JsString>>,
    pub interval_ms: Option<u64>,
}

/// Which status an indicator represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusIndicatorKind {
    Working,
    Retry,
    Compaction,
    BranchSummary,
}

/// Loader subclass carrying a status kind.
pub struct StatusIndicator {
    kind: StatusIndicatorKind,
    loader: Rc<RefCell<Loader>>,
}

impl StatusIndicator {
    pub fn new(
        kind: StatusIndicatorKind,
        request_render: Rc<dyn Fn()>,
        spinner_color_fn: StyleFn,
        message_color_fn: StyleFn,
        message: impl Into<JsString>,
        indicator: Option<WorkingIndicatorOptions>,
    ) -> Self {
        let indicator = Some(LoaderIndicatorOptions {
            frames: Some(Vec::new()),
            interval_ms: indicator.and_then(|indicator| indicator.interval_ms),
        });
        Self {
            kind,
            loader: Rc::new(RefCell::new(Loader::new(
                request_render,
                spinner_color_fn,
                message_color_fn,
                message,
                indicator,
            ))),
        }
    }

    pub fn kind(&self) -> StatusIndicatorKind {
        self.kind
    }

    pub fn set_message(&mut self, message: impl Into<JsString>) {
        self.loader.borrow_mut().set_message(message);
    }

    pub fn dispose(&mut self) {
        self.loader.borrow_mut().stop();
    }

    pub fn loader(&self) -> &Rc<RefCell<Loader>> {
        &self.loader
    }
}

impl Component for StatusIndicator {
    fn wants_input(&self) -> bool {
        false
    }
    fn dispose(&mut self) {
        StatusIndicator::dispose(self);
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.loader.borrow_mut().render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.loader.borrow_mut().handle_input(data);
    }

    fn invalidate(&mut self) {
        self.loader.borrow_mut().invalidate();
    }
}

fn accent(text: &JsString) -> JsString {
    with_theme(|theme| theme.fg(ThemeColor::Accent, text))
}

fn warning(text: &JsString) -> JsString {
    with_theme(|theme| theme.fg(ThemeColor::Warning, text))
}

fn muted(text: &JsString) -> JsString {
    with_theme(|theme| theme.fg(ThemeColor::Muted, text))
}

/// Working spinner with accent-colored frames.
pub struct WorkingStatusIndicator {
    indicator: StatusIndicator,
}

impl WorkingStatusIndicator {
    pub fn new(
        request_render: Rc<dyn Fn()>,
        message: impl Into<JsString>,
        indicator: Option<WorkingIndicatorOptions>,
    ) -> Self {
        Self {
            indicator: StatusIndicator::new(
                StatusIndicatorKind::Working,
                request_render,
                Rc::new(accent),
                Rc::new(muted),
                message,
                indicator,
            ),
        }
    }

    pub fn dispose(&mut self) {
        self.indicator.dispose();
    }
}

impl std::ops::Deref for WorkingStatusIndicator {
    type Target = StatusIndicator;
    fn deref(&self) -> &Self::Target {
        &self.indicator
    }
}
impl std::ops::DerefMut for WorkingStatusIndicator {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.indicator
    }
}

impl Component for WorkingStatusIndicator {
    fn wants_input(&self) -> bool {
        false
    }
    fn dispose(&mut self) {
        WorkingStatusIndicator::dispose(self);
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.indicator.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.indicator.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.indicator.invalidate();
    }
}

/// Retry countdown with a warning-colored spinner.
pub struct RetryStatusIndicator {
    indicator: StatusIndicator,
    countdown: Option<CountdownTimer>,
}

impl RetryStatusIndicator {
    pub fn new(request_render: Rc<dyn Fn()>, attempt: f64, max_attempts: f64, delay_ms: f64) -> Self {
        let retry_message = move |seconds: f64| {
            js_format!(
                "Retrying ({}/{}) in {}s... ({} to cancel)",
                pi_js_compat::number::js_number_to_string(attempt),
                pi_js_compat::number::js_number_to_string(max_attempts),
                pi_js_compat::number::js_number_to_string(seconds),
                key_text("app.interrupt")
            )
        };
        let indicator = StatusIndicator::new(
            StatusIndicatorKind::Retry,
            Rc::clone(&request_render),
            Rc::new(warning),
            Rc::new(muted),
            retry_message((delay_ms / 1000.0).ceil()),
            None,
        );
        let loader = Rc::clone(indicator.loader());
        let tick = Rc::new(move |seconds: f64| {
            loader.borrow_mut().set_message(retry_message(seconds));
        });
        let countdown = CountdownTimer::new(delay_ms, Some(request_render), tick, Rc::new(|| {}));

        Self {
            indicator,
            countdown: Some(countdown),
        }
    }

    pub fn dispose(&mut self) {
        if let Some(mut countdown) = self.countdown.take() {
            countdown.dispose();
        }
        self.indicator.dispose();
    }

    pub fn poll_countdown(&mut self, now: std::time::Instant) -> bool {
        self.countdown.as_mut().is_some_and(|countdown| countdown.poll(now))
    }

    pub fn next_countdown_deadline(&self) -> Option<std::time::Instant> {
        self.countdown.as_ref().and_then(CountdownTimer::next_deadline)
    }
}

impl std::ops::Deref for RetryStatusIndicator {
    type Target = StatusIndicator;
    fn deref(&self) -> &Self::Target {
        &self.indicator
    }
}
impl std::ops::DerefMut for RetryStatusIndicator {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.indicator
    }
}

impl Component for RetryStatusIndicator {
    fn wants_input(&self) -> bool {
        false
    }
    fn dispose(&mut self) {
        RetryStatusIndicator::dispose(self);
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.indicator.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.indicator.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.indicator.invalidate();
    }
}

/// Compaction spinner with the cancellation hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactionStatusReason {
    Manual,
    Threshold,
    Overflow,
}

pub struct CompactionStatusIndicator {
    indicator: StatusIndicator,
}

impl CompactionStatusIndicator {
    pub fn new(request_render: Rc<dyn Fn()>, reason: CompactionStatusReason) -> Self {
        let cancel_hint = js_format!("({} to cancel)", key_text("app.interrupt"));
        let label = match reason {
            CompactionStatusReason::Manual => js_format!("Compacting context... {}", cancel_hint),
            CompactionStatusReason::Threshold => js_format!("Auto-compacting... {}", cancel_hint),
            CompactionStatusReason::Overflow => {
                js_format!("Context overflow detected, Auto-compacting... {}", cancel_hint)
            }
        };
        Self {
            indicator: StatusIndicator::new(
                StatusIndicatorKind::Compaction,
                request_render,
                Rc::new(accent),
                Rc::new(muted),
                label,
                None,
            ),
        }
    }

    pub fn dispose(&mut self) {
        self.indicator.dispose();
    }
}

impl std::ops::Deref for CompactionStatusIndicator {
    type Target = StatusIndicator;
    fn deref(&self) -> &Self::Target {
        &self.indicator
    }
}
impl std::ops::DerefMut for CompactionStatusIndicator {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.indicator
    }
}

impl Component for CompactionStatusIndicator {
    fn wants_input(&self) -> bool {
        false
    }
    fn dispose(&mut self) {
        CompactionStatusIndicator::dispose(self);
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.indicator.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.indicator.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.indicator.invalidate();
    }
}

/// Branch-summary spinner.
pub struct BranchSummaryStatusIndicator {
    indicator: StatusIndicator,
}

impl BranchSummaryStatusIndicator {
    pub fn new(request_render: Rc<dyn Fn()>) -> Self {
        Self {
            indicator: StatusIndicator::new(
                StatusIndicatorKind::BranchSummary,
                request_render,
                Rc::new(accent),
                Rc::new(muted),
                js_format!("Summarizing branch... ({} to cancel)", key_text("app.interrupt")),
                None,
            ),
        }
    }

    pub fn dispose(&mut self) {
        self.indicator.dispose();
    }
}

impl std::ops::Deref for BranchSummaryStatusIndicator {
    type Target = StatusIndicator;
    fn deref(&self) -> &Self::Target {
        &self.indicator
    }
}
impl std::ops::DerefMut for BranchSummaryStatusIndicator {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.indicator
    }
}

impl Component for BranchSummaryStatusIndicator {
    fn wants_input(&self) -> bool {
        false
    }
    fn dispose(&mut self) {
        BranchSummaryStatusIndicator::dispose(self);
    }

    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.indicator.render(width)
    }

    fn handle_input(&mut self, data: &JsString) {
        self.indicator.handle_input(data);
    }

    fn invalidate(&mut self) {
        self.indicator.invalidate();
    }
}

/// Empty two-line status used when idle.
#[derive(Default)]
pub struct IdleStatus;

impl Component for IdleStatus {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        let empty_line = JsString::from(" ".repeat(width));
        vec![empty_line.clone(), empty_line]
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}
