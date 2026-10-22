//! Ported from `packages/tui/src/components/box.ts`.
//!
//! Container that applies horizontal/vertical padding and an optional background
//! to all child lines. The cache is validated by sampling the background
//! callback (`setBgFn` intentionally does not invalidate, matching the source).

use std::rc::Rc;

use super::text::StyleFn;
use pi_js_compat::js_json::JsString;

use crate::tui::{Component, ComponentRef};
use crate::utils::{apply_background_to_line, visible_width};

struct RenderCache {
    child_lines: Vec<JsString>,
    width: usize,
    bg_sample: Option<JsString>,
    lines: Vec<JsString>,
}

pub struct BoxComponent {
    children: Vec<ComponentRef>,
    padding_x: usize,
    padding_y: usize,
    bg_fn: Option<StyleFn>,
    cache: Option<RenderCache>,
}

impl Default for BoxComponent {
    fn default() -> Self {
        Self::new(1, 1, None)
    }
}

impl BoxComponent {
    pub fn new(padding_x: usize, padding_y: usize, bg_fn: Option<StyleFn>) -> Self {
        Self {
            children: Vec::new(),
            padding_x,
            padding_y,
            bg_fn,
            cache: None,
        }
    }

    pub fn children(&self) -> &[ComponentRef] {
        &self.children
    }

    pub fn add_child(&mut self, component: ComponentRef) {
        self.children.push(component);
        self.cache = None;
    }

    pub fn remove_child(&mut self, component: &ComponentRef) {
        let Some(index) = self.children.iter().position(|child| Rc::ptr_eq(child, component)) else {
            return;
        };
        self.children.remove(index);
        self.cache = None;
    }

    pub fn clear(&mut self) {
        self.children.clear();
        self.cache = None;
    }

    pub fn set_bg_fn(&mut self, bg_fn: Option<StyleFn>) {
        self.bg_fn = bg_fn;
    }

    fn match_cache(&self, width: usize, child_lines: &[JsString], bg_sample: Option<&JsString>) -> bool {
        let Some(cache) = &self.cache else {
            return false;
        };
        cache.width == width && cache.bg_sample.as_ref() == bg_sample && cache.child_lines == child_lines
    }

    fn apply_bg(&self, line: &JsString, width: usize) -> JsString {
        let padding_needed = width.saturating_sub(visible_width(line));
        let padded = line.clone() + " ".repeat(padding_needed);
        match &self.bg_fn {
            Some(bg_fn) => apply_background_to_line(&padded, width, |text| bg_fn(text)),
            None => padded,
        }
    }
}

impl Component for BoxComponent {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        if self.children.is_empty() {
            return Vec::new();
        }

        let content_width = width.saturating_sub(self.padding_x * 2).max(1);
        let left_pad = " ".repeat(self.padding_x);
        let mut child_lines = Vec::new();
        for child in &self.children {
            for line in child.borrow_mut().render(content_width) {
                child_lines.push(JsString::from(&left_pad) + line);
            }
        }
        if child_lines.is_empty() {
            return Vec::new();
        }

        let bg_sample = self.bg_fn.as_ref().map(|bg_fn| bg_fn(&JsString::from("test")));
        if self.match_cache(width, &child_lines, bg_sample.as_ref()) {
            return self.cache.as_ref().expect("cache matched").lines.clone();
        }

        let mut result = Vec::new();
        for _ in 0..self.padding_y {
            result.push(self.apply_bg(&JsString::default(), width));
        }
        for line in &child_lines {
            result.push(self.apply_bg(line, width));
        }
        for _ in 0..self.padding_y {
            result.push(self.apply_bg(&JsString::default(), width));
        }

        self.cache = Some(RenderCache {
            child_lines,
            width,
            bg_sample,
            lines: result.clone(),
        });
        result
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        self.cache = None;
        for child in &self.children {
            child.borrow_mut().invalidate();
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}
