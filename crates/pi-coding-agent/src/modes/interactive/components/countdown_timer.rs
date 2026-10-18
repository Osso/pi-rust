//! Ported from `packages/coding-agent/src/modes/interactive/components/countdown-timer.ts`.
//!
//! TypeScript drives ticks with `setInterval`; the Rust port exposes the same
//! one-second schedule as [`CountdownTimer::poll`] plus `next_deadline`, which
//! the TUI loop drives (the pattern used by the ported `Loader`).

use std::rc::Rc;
use std::time::{Duration, Instant};

pub struct CountdownTimer {
    remaining_seconds: i64,
    deadline: Option<Instant>,
    on_tick: Rc<dyn Fn(i64)>,
    on_expire: Rc<dyn Fn()>,
    request_render: Option<Rc<dyn Fn()>>,
}

impl CountdownTimer {
    pub fn new(
        timeout_ms: u64,
        request_render: Option<Rc<dyn Fn()>>,
        on_tick: Rc<dyn Fn(i64)>,
        on_expire: Rc<dyn Fn()>,
    ) -> Self {
        let remaining_seconds = timeout_ms.div_ceil(1000) as i64;
        on_tick(remaining_seconds);
        Self {
            remaining_seconds,
            deadline: Some(Instant::now() + Duration::from_secs(1)),
            on_tick,
            on_expire,
            request_render,
        }
    }

    /// Advance every tick that is due. Returns `true` once the timer expired.
    pub fn poll(&mut self, now: Instant) -> bool {
        while let Some(deadline) = self.deadline {
            if now < deadline {
                return false;
            }
            self.remaining_seconds -= 1;
            (self.on_tick)(self.remaining_seconds);
            if let Some(request_render) = &self.request_render {
                request_render();
            }
            if self.remaining_seconds <= 0 {
                self.dispose();
                (self.on_expire)();
                return true;
            }
            self.deadline = Some(deadline + Duration::from_secs(1));
        }
        false
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub fn remaining_seconds(&self) -> i64 {
        self.remaining_seconds
    }

    pub fn dispose(&mut self) {
        self.deadline = None;
    }

    pub fn is_disposed(&self) -> bool {
        self.deadline.is_none()
    }
}
