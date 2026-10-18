//! Ported from `packages/tui/src/layout-node.ts`.
//!
//! TypeScript exposes layout nodes through a global symbol method. Rust models
//! the same contract with [`LayoutComponent::layout_node`], which the
//! [`crate::tui::Component`] trait forwards by default.

use std::cell::RefCell;
use std::rc::Rc;

use crate::components::scroll_view::ScrollState;
use crate::tui::ComponentRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutViewport {
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackBasis {
    Auto,
    Cells(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackKind {
    VStack,
    HStack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackAlign {
    Stretch,
    Start,
    Center,
    End,
}

pub type VisibleFn = Rc<dyn Fn(LayoutViewport) -> bool>;

#[derive(Clone)]
pub struct StackLayoutEntry {
    pub component: ComponentRef,
    pub basis: Option<StackBasis>,
    pub grow: Option<usize>,
    pub shrink: Option<usize>,
    pub min_size: Option<usize>,
    pub max_size: Option<usize>,
    pub visible: Option<VisibleFn>,
}

impl StackLayoutEntry {
    pub fn new(component: ComponentRef) -> Self {
        Self {
            component,
            basis: None,
            grow: None,
            shrink: None,
            min_size: None,
            max_size: None,
            visible: None,
        }
    }
}

pub struct StackLayoutNode {
    pub kind: StackKind,
    pub entries: Vec<StackLayoutEntry>,
    pub gap: usize,
    pub align: StackAlign,
}

pub struct ScrollLayoutNode {
    pub component: ComponentRef,
    pub state: Rc<RefCell<ScrollState>>,
}

pub enum LayoutNode {
    Stack(StackLayoutNode),
    Scroll(ScrollLayoutNode),
}

/// Component that exposes layout geometry to [`crate::layout`].
pub trait LayoutComponent {
    fn layout_node(&self) -> Option<LayoutNode>;
}

/// Read the layout node of a component handle, if it exposes one.
pub fn get_layout_node(component: &ComponentRef) -> Option<LayoutNode> {
    component.borrow().layout_node()
}
