//! Ported from `packages/tui/src/components/alt-screen-flash.ts`.
//!
//! The TypeScript `setTimeout` expiry becomes a driver-owned deadline: call
//! [`AltScreenFlashContainer::poll`] with the current instant.

use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::tui::Component;
use crate::utils::truncate_to_width_with;

pub const DEFAULT_DURATION_MS: u64 = 1000;

struct FlashEntry {
    message: String,
    expires_at: Instant,
}

/// Transient messages composited by the alternate-screen renderer.
pub struct AltScreenFlashContainer {
    entries: Vec<FlashEntry>,
    request_render: Rc<dyn Fn()>,
}

impl AltScreenFlashContainer {
    pub fn new(request_render: Rc<dyn Fn()>) -> Self {
        Self {
            entries: Vec::new(),
            request_render,
        }
    }

    pub fn flash(&mut self, message: impl Into<String>, duration: Duration) {
        self.flash_at(message, duration, Instant::now());
    }

    pub fn flash_default(&mut self, message: impl Into<String>) {
        self.flash(message, Duration::from_millis(DEFAULT_DURATION_MS));
    }

    pub fn flash_at(&mut self, message: impl Into<String>, duration: Duration, now: Instant) {
        self.entries.push(FlashEntry {
            message: message.into(),
            expires_at: now + duration,
        });
        (self.request_render)();
    }

    /// Remove expired messages. Returns whether a render was requested.
    pub fn poll(&mut self, now: Instant) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.expires_at > now);
        if self.entries.len() == before {
            return false;
        }
        (self.request_render)();
        true
    }

    pub fn dispose(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Component for AltScreenFlashContainer {
    fn render(&mut self, width: usize) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| {
                let message = truncate_to_width_with(&format!(" {} ", entry.message), width, "", false);
                format!("\x1b[7m{message}\x1b[27m")
            })
            .collect()
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {}
}
