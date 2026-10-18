//! Ported from `packages/tui/src/components/v-stack.ts`.

use super::stack::{Stack, StackOptions, allocate_stack_sizes, visible_stack_entries};
use crate::layout_node::{LayoutNode, LayoutViewport, StackKind, StackLayoutEntry};
use crate::tui::{Component, Container};

pub struct VStack {
    stack: Stack,
}

impl VStack {
    pub fn new(children: Vec<StackLayoutEntry>, options: StackOptions) -> Self {
        Self {
            stack: Stack::new(StackKind::VStack, children, options),
        }
    }

    pub fn stack(&self) -> &Stack {
        &self.stack
    }

    pub fn stack_mut(&mut self) -> &mut Stack {
        &mut self.stack
    }
}

impl Component for VStack {
    fn render(&mut self, width: usize) -> Vec<String> {
        let viewport = LayoutViewport {
            width: width.max(1),
            height: usize::MAX,
        };
        let entries = visible_stack_entries(self.stack.entries(), viewport);
        let rendered: Vec<Vec<String>> = entries
            .iter()
            .map(|entry| entry.component.borrow_mut().render(viewport.width))
            .collect();
        let intrinsic_heights: Vec<usize> = rendered.iter().map(Vec::len).collect();
        let sizes = allocate_stack_sizes(&entries, &intrinsic_heights, None, self.stack.gap());

        let mut lines = Vec::new();
        for (index, child_lines) in rendered.iter().enumerate() {
            if index > 0 {
                lines.extend(std::iter::repeat_n(String::new(), self.stack.gap()));
            }
            let size = sizes[index];
            lines.extend(child_lines.iter().take(size).cloned());
            lines.extend(std::iter::repeat_n(
                String::new(),
                size.saturating_sub(child_lines.len()),
            ));
        }
        lines
    }

    fn wants_input(&self) -> bool {
        false
    }

    fn invalidate(&mut self) {
        for entry in self.stack.entries() {
            entry.component.borrow_mut().invalidate();
        }
    }

    fn as_container(&self) -> Option<&Container> {
        Some(self.stack.as_container())
    }

    fn layout_node(&self) -> Option<LayoutNode> {
        Some(LayoutNode::Stack(self.stack.layout_node()))
    }
}
