//! Ported from `coding-agent/src/modes/interactive/components/render-region-container.ts`.

use indexmap::IndexMap;
use pi_js_compat::js_json::JsString;
use pi_tui::tui::{Component, ComponentRef, Container, RenderRegion, RenderRegionRect};
use std::{cell::RefCell, rc::Rc};

struct ScopedChild {
    component: ComponentRef,
    region: Option<RenderRegion>,
    layout: Option<RenderRegionRect>,
}

/// Chat container with explicitly scoped, independently repaintable children.
pub struct RenderRegionContainer {
    container: Container,
    create_region: Rc<dyn Fn(ComponentRef) -> RenderRegion>,
    scoped: IndexMap<usize, ScopedChild>,
    layout: Option<RenderRegionRect>,
}

impl RenderRegionContainer {
    pub fn new(create_region: Rc<dyn Fn(ComponentRef) -> RenderRegion>) -> Self {
        Self {
            container: Container::new(),
            create_region,
            scoped: IndexMap::new(),
            layout: None,
        }
    }

    pub fn children(&self) -> &[ComponentRef] {
        self.container.children()
    }
    pub fn add_child(&mut self, child: ComponentRef) {
        self.container.add_child(child);
    }
    pub fn insert_child(&mut self, index: usize, child: ComponentRef) {
        self.container.insert_child(index, child);
    }
    /// Frozen's direct children-array assignment does not dispose moved children.
    pub fn set_children(&mut self, children: Vec<ComponentRef>) {
        self.container = Container::with_children(children);
    }

    pub fn track_scoped_child(&mut self, component: ComponentRef) {
        self.scoped.entry(component_id(&component)).or_insert(ScopedChild {
            component,
            region: None,
            layout: None,
        });
    }

    pub fn place(&mut self, layout: RenderRegionRect) {
        self.layout = Some(layout);
        for child in self.scoped.values() {
            place_child_region(self.layout, child);
        }
    }

    pub fn request_child_render(&mut self, component: &ComponentRef) -> bool {
        if !self.children().iter().any(|child| Rc::ptr_eq(child, component)) {
            return false;
        }
        let Some(child) = self.scoped.get_mut(&component_id(component)) else {
            return false;
        };
        if child.region.is_none() {
            child.region = Some((self.create_region)(component.clone()));
        }
        place_child_region(self.layout, child) && child.region.as_ref().is_some_and(RenderRegion::try_render)
    }

    pub fn remove_child(&mut self, component: &ComponentRef) {
        if let Some(child) = self.scoped.shift_remove(&component_id(component)) {
            dispose_scoped_child(child);
        }
        self.container.remove_child(component);
    }

    pub fn clear(&mut self) {
        for (_, child) in self.scoped.drain(..) {
            dispose_scoped_child(child);
        }
        self.layout = None;
        self.container.clear();
    }
}

fn component_id(component: &ComponentRef) -> usize {
    Rc::as_ptr(component).cast::<()>() as usize
}

fn place_child_region(layout: Option<RenderRegionRect>, child: &ScopedChild) -> bool {
    let Some(region) = &child.region else {
        return false;
    };
    let (Some(layout), Some(child_layout)) = (layout, child.layout) else {
        region.clear();
        return false;
    };
    region.place(RenderRegionRect {
        row: layout.row + child_layout.row,
        col: layout.col + child_layout.col,
        width: child_layout.width,
        height: child_layout.height,
    });
    true
}

fn dispose_scoped_child(child: ScopedChild) {
    if let Some(region) = child.region {
        region.dispose();
    }
    child.component.borrow_mut().dispose();
}

impl Component for RenderRegionContainer {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        for child in self.scoped.values_mut() {
            child.layout = None;
        }
        let mut lines = Vec::new();
        for component in self.container.children() {
            let child_lines = component.borrow_mut().render(width);
            if let Some(child) = self.scoped.get_mut(&component_id(component)) {
                child.layout = Some(RenderRegionRect {
                    row: lines.len() as i64,
                    col: 0,
                    width,
                    height: child_lines.len(),
                });
            }
            lines.extend(child_lines);
        }
        lines
    }
    fn invalidate(&mut self) {
        self.container.invalidate();
    }
    fn wants_input(&self) -> bool {
        false
    }
    fn as_container(&self) -> Option<&Container> {
        Some(&self.container)
    }
}

/// Mounts a typed chat container without erasing its scoped-render API.
pub struct SharedRenderRegionContainer(pub Rc<RefCell<RenderRegionContainer>>);
impl Component for SharedRenderRegionContainer {
    fn render(&mut self, width: usize) -> Vec<JsString> {
        self.0.borrow_mut().render(width)
    }
    fn invalidate(&mut self) {
        self.0.borrow_mut().invalidate();
    }
    fn wants_input(&self) -> bool {
        false
    }
}
