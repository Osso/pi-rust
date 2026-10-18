//! Ported from `packages/tui/src/components/stack.ts`.
//!
//! Stack layout entries, size allocation, and grow/shrink distribution shared by
//! [`super::v_stack`] and [`super::h_stack`].

use crate::layout_node::{
    LayoutViewport, StackAlign, StackBasis, StackKind, StackLayoutEntry, StackLayoutNode, VisibleFn,
};
use crate::tui::{ComponentRef, Container};

pub type StackEntry = StackLayoutEntry;

#[derive(Clone, Default)]
pub struct StackEntryOptions {
    pub basis: Option<StackBasis>,
    pub grow: Option<usize>,
    pub shrink: Option<usize>,
    pub min_size: Option<usize>,
    pub max_size: Option<usize>,
    pub visible: Option<VisibleFn>,
}

impl StackEntryOptions {
    pub fn apply_to(self, component: ComponentRef) -> StackLayoutEntry {
        StackLayoutEntry {
            component,
            basis: self.basis,
            grow: self.grow,
            shrink: self.shrink,
            min_size: self.min_size,
            max_size: self.max_size,
            visible: self.visible,
        }
    }
}

#[derive(Clone, Default)]
pub struct StackOptions {
    pub gap: Option<usize>,
    pub align: Option<StackAlign>,
}

pub struct Stack {
    container: Container,
    entries: Vec<StackLayoutEntry>,
    gap: usize,
    align: StackAlign,
    kind: StackKind,
}

impl Stack {
    pub fn new(kind: StackKind, children: Vec<StackLayoutEntry>, options: StackOptions) -> Self {
        let mut stack = Self {
            container: Container::new(),
            entries: Vec::new(),
            gap: options.gap.unwrap_or(0),
            align: options.align.unwrap_or(StackAlign::Stretch),
            kind,
        };
        for child in children {
            stack.add_child(child);
        }
        stack
    }

    pub fn add_child(&mut self, entry: StackLayoutEntry) {
        self.container.add_child(entry.component.clone());
        self.entries.push(entry);
    }

    pub fn remove_child(&mut self, component: &ComponentRef) {
        self.container.remove_child(component);
        self.entries
            .retain(|entry| !std::rc::Rc::ptr_eq(&entry.component, component));
    }

    pub fn clear(&mut self) {
        self.container.clear();
        self.entries.clear();
    }

    pub fn entries(&self) -> &[StackLayoutEntry] {
        &self.entries
    }

    pub fn gap(&self) -> usize {
        self.gap
    }

    pub fn align(&self) -> StackAlign {
        self.align
    }

    pub fn kind(&self) -> StackKind {
        self.kind
    }

    pub fn layout_node(&self) -> StackLayoutNode {
        StackLayoutNode {
            kind: self.kind,
            entries: self.entries.clone(),
            gap: self.gap,
            align: self.align,
        }
    }

    pub fn as_container(&self) -> &Container {
        &self.container
    }
}

/// Filter entries by their `visible` predicate for the given viewport.
pub fn visible_stack_entries(entries: &[StackLayoutEntry], viewport: LayoutViewport) -> Vec<StackLayoutEntry> {
    entries
        .iter()
        .filter(|entry| entry.visible.as_ref().is_none_or(|visible| visible(viewport)))
        .cloned()
        .collect()
}

fn clamp_size(size: usize, entry: &StackLayoutEntry) -> usize {
    let min = entry.min_size.unwrap_or(0);
    let max = entry.max_size.unwrap_or(usize::MAX).max(min);
    size.clamp(min, max)
}

fn weight(entry: &StackLayoutEntry, size: usize, mode: GrowShrink) -> usize {
    match mode {
        GrowShrink::Grow => entry.grow.unwrap_or(0),
        GrowShrink::Shrink => entry.shrink.unwrap_or(1) * size.max(1),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GrowShrink {
    Grow,
    Shrink,
}

fn is_candidate(entry: &StackLayoutEntry, size: usize, mode: GrowShrink) -> bool {
    match mode {
        GrowShrink::Grow => entry.grow.unwrap_or(0) > 0 && size < entry.max_size.unwrap_or(usize::MAX),
        GrowShrink::Shrink => entry.shrink.unwrap_or(1) > 0 && size > entry.min_size.unwrap_or(0),
    }
}

fn distribute(sizes: &mut [usize], entries: &[StackLayoutEntry], amount: usize, mode: GrowShrink) {
    let mut remaining = amount as i128;
    while remaining > 0 {
        let candidates: Vec<usize> = (0..entries.len())
            .filter(|index| is_candidate(&entries[*index], sizes[*index], mode))
            .collect();
        if candidates.is_empty() {
            return;
        }

        let total_weight: i128 = candidates
            .iter()
            .map(|index| weight(&entries[*index], sizes[*index], mode) as i128)
            .sum();
        let mut distributed: i128 = 0;
        for index in candidates {
            if remaining <= 0 {
                break;
            }
            let entry = &entries[index];
            let candidate_weight = weight(entry, sizes[index], mode) as i128;
            let proposed = (remaining * candidate_weight / total_weight).max(1);
            let capacity = match mode {
                GrowShrink::Grow => entry.max_size.unwrap_or(usize::MAX) as i128 - sizes[index] as i128,
                GrowShrink::Shrink => sizes[index] as i128 - entry.min_size.unwrap_or(0) as i128,
            };
            let delta = remaining.min(proposed).min(capacity);
            if delta <= 0 {
                continue;
            }
            sizes[index] = if mode == GrowShrink::Grow {
                sizes[index] + delta as usize
            } else {
                sizes[index] - delta as usize
            };
            remaining -= delta;
            distributed += delta;
        }
        if distributed == 0 {
            return;
        }
    }
}

/// Allocate entry sizes from intrinsic sizes, grow/shrink limits, and the
/// available space (`None` keeps intrinsic sizes).
pub fn allocate_stack_sizes(
    entries: &[StackLayoutEntry],
    intrinsic_sizes: &[usize],
    available_size: Option<usize>,
    gap: usize,
) -> Vec<usize> {
    let mut sizes: Vec<usize> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let size = match entry.basis {
                Some(StackBasis::Cells(cells)) => cells,
                Some(StackBasis::Auto) | None => intrinsic_sizes.get(index).copied().unwrap_or(0),
            };
            clamp_size(size, entry)
        })
        .collect();

    let Some(available_size) = available_size else {
        return sizes;
    };

    let content_size = available_size.saturating_sub(entries.len().saturating_sub(1) * gap);
    let total: usize = sizes.iter().sum();
    if total < content_size {
        distribute(&mut sizes, entries, content_size - total, GrowShrink::Grow);
    } else if total > content_size {
        distribute(&mut sizes, entries, total - content_size, GrowShrink::Shrink);
    }
    sizes
}
