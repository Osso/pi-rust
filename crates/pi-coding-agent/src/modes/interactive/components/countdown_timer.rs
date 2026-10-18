//! Ported from `coding-agent/src/modes/interactive/components/countdown-timer.ts`.
//!
//! The TypeScript class owns a `setInterval`; the Rust port keeps the same
//! one-second schedule but exposes it as [`CountdownTimer::poll`], matching the
//! poll-driven animation used by the ported TUI loader/editor.

use std::rc::Rc;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(1000);

/// One-second countdown used by dialog components.
pub struct CountdownTimer {
    remaining_seconds: i64,
    next_tick_at: Instant,
    disposed: bool,
    request_render: Option<Rc<dyn Fn()>>,
    on_tick: Rc<dyn Fn(i64)>,
    on_expire: Rc<dyn Fn()>,
}

impl CountdownTimer {
    /// `new CountdownTimer`: reports `ceil(timeoutMs / 1000)` immediately.
    pub fn new(
        timeout_ms: f64,
        request_render: Option<Rc<dyn Fn()>>,
        on_tick: Rc<dyn Fn(i64)>,
        on_expire: Rc<dyn Fn()>,
    ) -> Self {
        let remaining_seconds = (timeout_ms / 1000.0).ceil();
        let remaining_seconds = remaining_seconds as i64;
        on_tick(remaining_seconds);
        Self {
            remaining_seconds,
            next_tick_at: Instant::now() + TICK,
            disposed: false,
            request_render,
            on_tick,
            on_expire,
        }
    }

    pub fn remaining_seconds(&self) -> i64 {
        self.remaining_seconds
    }

    pub fn is_disposed(&self) -> bool {
        self.disposed
    }

    /// Advance the countdown to `now`; returns whether any tick fired.
    pub fn poll(&mut self, now: Instant) -> bool {
        let mut ticked = false;
        while !self.disposed && now >= self.next_tick_at {
            self.next_tick_at += TICK;
            self.remaining_seconds -= 1;
            (self.on_tick)(self.remaining_seconds);
            if let Some(request_render) = &self.request_render {
                request_render();
            }
            ticked = true;
            if self.remaining_seconds <= 0 {
                self.dispose();
                (self.on_expire)();
            }
        }
        ticked
    }

    pub fn dispose(&mut self) {
        self.disposed = true;
    }
}
