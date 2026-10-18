//! Ported from `coding-agent/src/modes/interactive/components/status-indicator.ts`.

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
    pub frames: Option<Vec<String>>,
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
        message: impl Into<String>,
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

    pub fn set_message(&mut self, message: impl Into<String>) {
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
    fn render(&mut self, width: usize) -> Vec<String> {
        self.loader.borrow_mut().render(width)
    }

    fn handle_input(&mut self, data: &str) {
        self.loader.borrow_mut().handle_input(data);
    }

    fn invalidate(&mut self) {
        self.loader.borrow_mut().invalidate();
    }
}

fn accent(text: &str) -> String {
    with_theme(|theme| theme.fg(ThemeColor::Accent, text))
}

fn warning(text: &str) -> String {
    with_theme(|theme| theme.fg(ThemeColor::Warning, text))
}

fn muted(text: &str) -> String {
    with_theme(|theme| theme.fg(ThemeColor::Muted, text))
}

/// Working spinner with accent-colored frames.
pub struct WorkingStatusIndicator {
    indicator: StatusIndicator,
}

impl WorkingStatusIndicator {
    pub fn new(
        request_render: Rc<dyn Fn()>,
        message: impl Into<String>,
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

impl Component for WorkingStatusIndicator {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.indicator.render(width)
    }

    fn handle_input(&mut self, data: &str) {
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
    pub fn new(request_render: Rc<dyn Fn()>, attempt: u64, max_attempts: u64, delay_ms: u64) -> Self {
        let retry_message = move |seconds: i64| {
            format!(
                "Retrying ({attempt}/{max_attempts}) in {seconds}s... ({} to cancel)",
                key_text("app.interrupt")
            )
        };
        let indicator = StatusIndicator::new(
            StatusIndicatorKind::Retry,
            Rc::clone(&request_render),
            Rc::new(warning),
            Rc::new(muted),
            retry_message(delay_ms.div_ceil(1000) as i64),
            None,
        );
        let loader = Rc::clone(indicator.loader());
        let tick = Rc::new(move |seconds: i64| {
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

impl Component for RetryStatusIndicator {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.indicator.render(width)
    }

    fn handle_input(&mut self, data: &str) {
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
        let cancel_hint = format!("({} to cancel)", key_text("app.interrupt"));
        let label = match reason {
            CompactionStatusReason::Manual => format!("Compacting context... {cancel_hint}"),
            CompactionStatusReason::Threshold => format!("Auto-compacting... {cancel_hint}"),
            CompactionStatusReason::Overflow => {
                format!("Context overflow detected, Auto-compacting... {cancel_hint}")
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

impl Component for CompactionStatusIndicator {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.indicator.render(width)
    }

    fn handle_input(&mut self, data: &str) {
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
                format!("Summarizing branch... ({} to cancel)", key_text("app.interrupt")),
                None,
            ),
        }
    }

    pub fn dispose(&mut self) {
        self.indicator.dispose();
    }
}

impl Component for BranchSummaryStatusIndicator {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.indicator.render(width)
    }

    fn handle_input(&mut self, data: &str) {
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
    fn render(&mut self, width: usize) -> Vec<String> {
        let empty_line = " ".repeat(width);
        vec![empty_line.clone(), empty_line]
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}
