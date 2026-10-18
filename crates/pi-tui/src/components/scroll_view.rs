//! Ported from `packages/tui/src/components/scroll-view.ts`.
//!
//! The scroll position, content geometry, and scrollbar state live in a shared
//! [`ScrollState`] so `layout.rs` can drive layout through the same state the
//! component exposes. The TypeScript transient-scrollbar `setTimeout` becomes a
//! driver-owned deadline: call [`ScrollState::poll`] with the current instant.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::text::StyleFn;
use crate::layout_node::{LayoutNode, ScrollLayoutNode};
use crate::tui::{Component, ComponentRef, Container};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollViewScrollbar {
    Hidden,
    Auto,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollFollow {
    None,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollOverscroll {
    Chain,
    Contain,
}

#[derive(Clone, Default)]
pub struct ScrollViewOptions {
    pub follow: Option<ScrollFollow>,
    pub primary: bool,
    pub overscroll: Option<ScrollOverscroll>,
    pub scrollbar: Option<ScrollViewScrollbar>,
    pub scrollbar_style: Option<StyleFn>,
    pub scrollbar_hide_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollViewScrollToOptions {
    /// Keep follow-end disabled even when the target is the current content end.
    pub disable_follow: bool,
}

pub type RequestRenderFn = Rc<dyn Fn()>;

pub struct ScrollState {
    follow_end: bool,
    pub primary: bool,
    pub overscroll: ScrollOverscroll,
    scrollbar_style: StyleFn,
    current_scrollbar: ScrollViewScrollbar,
    scrollbar_hide_delay: Duration,
    current_scroll_top: usize,
    content_height: usize,
    current_viewport_height: usize,
    following_end: bool,
    follow_suppressed_at_end: bool,
    request_render: Option<RequestRenderFn>,
    transient_scrollbar_visible: bool,
    scrollbar_active: bool,
    scrollbar_hide_deadline: Option<Instant>,
}

impl ScrollState {
    fn new(options: &ScrollViewOptions) -> Self {
        let follow_end = options.follow.unwrap_or(ScrollFollow::None) == ScrollFollow::End;
        Self {
            follow_end,
            primary: options.primary,
            overscroll: options.overscroll.unwrap_or(ScrollOverscroll::Chain),
            scrollbar_style: options.scrollbar_style.clone().unwrap_or_else(default_scrollbar_style),
            current_scrollbar: options.scrollbar.unwrap_or(ScrollViewScrollbar::Hidden),
            scrollbar_hide_delay: Duration::from_millis(options.scrollbar_hide_delay_ms.unwrap_or(1000)),
            current_scroll_top: 0,
            content_height: 0,
            current_viewport_height: 0,
            following_end: follow_end,
            follow_suppressed_at_end: false,
            request_render: None,
            transient_scrollbar_visible: false,
            scrollbar_active: false,
            scrollbar_hide_deadline: None,
        }
    }

    pub fn scroll_top(&self) -> usize {
        self.current_scroll_top
    }

    pub fn is_following_end(&self) -> bool {
        self.following_end
    }

    pub fn viewport_height(&self) -> usize {
        self.current_viewport_height
    }

    pub fn content_height(&self) -> usize {
        self.content_height
    }

    pub fn scrollbar(&self) -> ScrollViewScrollbar {
        self.current_scrollbar
    }

    pub fn scrollbar_style(&self) -> &StyleFn {
        &self.scrollbar_style
    }

    pub fn is_scrollbar_visible(&self) -> bool {
        match self.current_scrollbar {
            ScrollViewScrollbar::Always => self.current_viewport_height > 0,
            ScrollViewScrollbar::Auto => {
                self.content_height > self.current_viewport_height && self.transient_scrollbar_visible
            }
            ScrollViewScrollbar::Hidden => false,
        }
    }

    pub fn get_content_width(&self, width: usize) -> usize {
        if self.current_scrollbar == ScrollViewScrollbar::Always && width > 1 {
            width - 1
        } else {
            width
        }
    }

    pub fn set_scrollbar(&mut self, scrollbar: ScrollViewScrollbar) {
        if scrollbar == self.current_scrollbar {
            return;
        }
        self.current_scrollbar = scrollbar;
        if scrollbar != ScrollViewScrollbar::Auto {
            self.hide_transient_scrollbar();
        } else if self.scrollbar_active {
            self.mark_scrollbar_activity();
        }
        self.request_render_now();
    }

    pub fn set_scrollbar_active(&mut self, active: bool) {
        if active == self.scrollbar_active {
            return;
        }
        self.scrollbar_active = active;
        self.mark_scrollbar_activity();
    }

    pub fn scroll_to(&mut self, scroll_top: i64, options: ScrollViewScrollToOptions) {
        let requested = scroll_top.max(0) as usize;
        let max_scroll_top = self.content_height.saturating_sub(self.current_viewport_height);
        let next = requested.min(max_scroll_top);
        let next_follow_suppressed_at_end = options.disable_follow && next == max_scroll_top;
        let next_following_end = !next_follow_suppressed_at_end && self.follow_end && next == max_scroll_top;
        if next == self.current_scroll_top
            && next_following_end == self.following_end
            && next_follow_suppressed_at_end == self.follow_suppressed_at_end
        {
            return;
        }
        let moved = next != self.current_scroll_top;
        self.current_scroll_top = next;
        self.following_end = next_following_end;
        self.follow_suppressed_at_end = next_follow_suppressed_at_end;
        if moved {
            self.mark_scrollbar_activity();
        }
        self.request_render_now();
    }

    /// Scroll by `lines`; returns the delta that could not be applied.
    pub fn scroll_by(&mut self, lines: i64) -> i64 {
        if lines == 0 {
            return 0;
        }
        let max_scroll_top = self.content_height.saturating_sub(self.current_viewport_height) as i64;
        let start = if self.following_end {
            max_scroll_top
        } else {
            self.current_scroll_top as i64
        };
        let next = (start + lines).clamp(0, max_scroll_top);
        let moved = next - start;
        let was_following_end = self.following_end;
        self.current_scroll_top = next as usize;
        self.following_end = self.follow_end && next == max_scroll_top;
        self.follow_suppressed_at_end = false;
        if moved != 0 {
            self.mark_scrollbar_activity();
        }
        if moved != 0 || self.following_end != was_following_end {
            self.request_render_now();
        }
        lines - moved
    }

    pub fn scroll_to_start(&mut self) {
        let follows_fitting_content = self.follow_end && self.content_height <= self.current_viewport_height;
        let changed = self.current_scroll_top != 0 || self.following_end != follows_fitting_content;
        self.current_scroll_top = 0;
        self.following_end = follows_fitting_content;
        self.follow_suppressed_at_end = false;
        if changed {
            self.mark_scrollbar_activity();
            self.request_render_now();
        }
    }

    pub fn scroll_to_end(&mut self) {
        let next = self.content_height.saturating_sub(self.current_viewport_height);
        let changed = self.current_scroll_top != next || self.following_end != self.follow_end;
        self.current_scroll_top = next;
        self.following_end = self.follow_end;
        self.follow_suppressed_at_end = false;
        if changed {
            self.mark_scrollbar_activity();
            self.request_render_now();
        }
    }

    pub fn update_layout(&mut self, content_height: usize, viewport_height: usize, request_render: RequestRenderFn) {
        self.content_height = content_height;
        self.current_viewport_height = viewport_height;
        self.request_render = Some(request_render);
        let max_scroll_top = content_height.saturating_sub(viewport_height);
        if self.following_end {
            self.current_scroll_top = max_scroll_top;
        } else {
            self.current_scroll_top = self.current_scroll_top.min(max_scroll_top);
        }
        if self.current_scroll_top < max_scroll_top {
            self.follow_suppressed_at_end = false;
        }
        if self.follow_end && self.current_scroll_top == max_scroll_top && !self.follow_suppressed_at_end {
            self.following_end = true;
        }
        if self.content_height <= self.current_viewport_height {
            self.hide_transient_scrollbar();
        }
    }

    /// Hide the transient scrollbar when its deadline passed.
    ///
    /// Returns whether a render was requested.
    pub fn poll(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.scrollbar_hide_deadline else {
            return false;
        };
        if now < deadline {
            return false;
        }
        self.scrollbar_hide_deadline = None;
        self.transient_scrollbar_visible = false;
        self.request_render_now();
        true
    }

    fn mark_scrollbar_activity(&mut self) {
        if self.current_scrollbar != ScrollViewScrollbar::Auto || self.content_height <= self.current_viewport_height {
            return;
        }
        self.transient_scrollbar_visible = true;
        self.scrollbar_hide_deadline = None;
        if self.scrollbar_active {
            return;
        }
        self.scrollbar_hide_deadline = Some(Instant::now() + self.scrollbar_hide_delay);
    }

    fn hide_transient_scrollbar(&mut self) {
        self.transient_scrollbar_visible = false;
        self.scrollbar_hide_deadline = None;
    }

    fn request_render_now(&self) {
        if let Some(request_render) = &self.request_render {
            request_render();
        }
    }
}

fn default_scrollbar_style() -> StyleFn {
    Rc::new(|text: &str| format!("\x1b[100m{text}\x1b[49m"))
}

/// Single-child scrollable container.
pub struct ScrollView {
    child: ComponentRef,
    container: Container,
    state: Rc<RefCell<ScrollState>>,
}

impl ScrollView {
    pub fn new(child: ComponentRef, options: ScrollViewOptions) -> Self {
        let state = Rc::new(RefCell::new(ScrollState::new(&options)));
        let container = Container::with_children(vec![Rc::clone(&child)]);
        Self {
            child,
            container,
            state,
        }
    }

    pub fn state(&self) -> &Rc<RefCell<ScrollState>> {
        &self.state
    }

    pub fn child(&self) -> &ComponentRef {
        &self.child
    }

    pub fn scroll_top(&self) -> usize {
        self.state.borrow().scroll_top()
    }

    pub fn is_following_end(&self) -> bool {
        self.state.borrow().is_following_end()
    }

    pub fn viewport_height(&self) -> usize {
        self.state.borrow().viewport_height()
    }

    pub fn scrollbar(&self) -> ScrollViewScrollbar {
        self.state.borrow().scrollbar()
    }

    pub fn is_scrollbar_visible(&self) -> bool {
        self.state.borrow().is_scrollbar_visible()
    }

    pub fn set_scrollbar(&mut self, scrollbar: ScrollViewScrollbar) {
        self.state.borrow_mut().set_scrollbar(scrollbar);
    }

    pub fn set_scrollbar_active(&mut self, active: bool) {
        self.state.borrow_mut().set_scrollbar_active(active);
    }

    pub fn get_content_width(&self, width: usize) -> usize {
        self.state.borrow().get_content_width(width)
    }

    pub fn scroll_to(&mut self, scroll_top: i64, options: ScrollViewScrollToOptions) {
        self.state.borrow_mut().scroll_to(scroll_top, options);
    }

    pub fn scroll_by(&mut self, lines: i64) -> i64 {
        self.state.borrow_mut().scroll_by(lines)
    }

    pub fn scroll_to_start(&mut self) {
        self.state.borrow_mut().scroll_to_start();
    }

    pub fn scroll_to_end(&mut self) {
        self.state.borrow_mut().scroll_to_end();
    }

    pub fn update_layout(&mut self, content_height: usize, viewport_height: usize, request_render: RequestRenderFn) {
        self.state
            .borrow_mut()
            .update_layout(content_height, viewport_height, request_render);
    }

    pub fn poll(&mut self, now: Instant) -> bool {
        self.state.borrow_mut().poll(now)
    }
}

impl Component for ScrollView {
    fn render(&mut self, width: usize) -> Vec<String> {
        let content_width = self.get_content_width(width);
        let mut lines = self.child.borrow_mut().render(content_width);
        if content_width != width {
            for line in &mut lines {
                line.push(' ');
            }
        }
        lines
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }

    fn layout_node(&self) -> Option<LayoutNode> {
        Some(LayoutNode::Scroll(ScrollLayoutNode {
            component: Rc::clone(&self.child),
            state: Rc::clone(&self.state),
        }))
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}
