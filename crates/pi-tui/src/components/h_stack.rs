//! Ported from `packages/tui/src/components/h-stack.ts`.

use super::stack::{Stack, StackOptions, allocate_stack_sizes, visible_stack_entries};
use crate::layout_node::{LayoutNode, LayoutViewport, StackAlign, StackKind, StackLayoutEntry};
use crate::tui::{Component, Container, composite_tui_line};
use crate::utils::visible_width;

pub struct HStack {
    stack: Stack,
}

impl HStack {
    pub fn new(children: Vec<StackLayoutEntry>, options: StackOptions) -> Self {
        Self {
            stack: Stack::new(StackKind::HStack, children, options),
        }
    }

    pub fn stack(&self) -> &Stack {
        &self.stack
    }

    pub fn stack_mut(&mut self) -> &mut Stack {
        &mut self.stack
    }
}

impl Component for HStack {
    fn render(&mut self, width: usize) -> Vec<String> {
        let safe_width = width.max(1);
        let viewport = LayoutViewport {
            width: safe_width,
            height: usize::MAX,
        };
        let entries = visible_stack_entries(self.stack.entries(), viewport);
        if entries.is_empty() {
            return Vec::new();
        }

        let intrinsic_widths: Vec<usize> = entries
            .iter()
            .map(|entry| {
                entry
                    .component
                    .borrow_mut()
                    .render(safe_width)
                    .iter()
                    .map(|line| visible_width(line))
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let widths = allocate_stack_sizes(&entries, &intrinsic_widths, Some(safe_width), self.stack.gap());
        let rendered: Vec<Vec<String>> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                if widths[index] == 0 {
                    Vec::new()
                } else {
                    entry.component.borrow_mut().render(widths[index])
                }
            })
            .collect();
        let height = rendered.iter().map(Vec::len).max().unwrap_or(0);
        let mut result = vec![String::new(); height];
        let mut x = 0;
        for (index, lines) in rendered.iter().enumerate() {
            let child_width = widths[index];
            let offset = match self.stack.align() {
                StackAlign::Center => height.saturating_sub(lines.len()) / 2,
                StackAlign::End => height.saturating_sub(lines.len()),
                StackAlign::Stretch | StackAlign::Start => 0,
            };
            for (row, line) in lines.iter().enumerate() {
                let target = row + offset;
                if target >= result.len() {
                    continue;
                }
                result[target] = composite_tui_line(&result[target], line, x, child_width, safe_width);
            }
            x += child_width + self.stack.gap();
        }
        result
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
