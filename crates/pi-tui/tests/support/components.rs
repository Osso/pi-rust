// Shared support module: each integration-test binary uses a subset of these helpers.
#![allow(dead_code)]

//! Shared test components for the ported TUI tests.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pi_tui::tui::{Component, ComponentRef, Container, Focusable, component};

pub type InputHandler = Rc<dyn Fn(&str)>;

#[derive(Default)]
pub struct TestState {
    pub lines: Vec<String>,
    pub render_count: usize,
    pub inputs: Vec<String>,
    pub last_render_width: Option<usize>,
    pub on_input: Option<InputHandler>,
}

impl TestState {
    pub fn set_lines(&mut self, lines: &[&str]) {
        self.lines = lines.iter().map(|line| (*line).to_string()).collect();
    }
}

pub struct TestComponent {
    state: Rc<RefCell<TestState>>,
    focus: Option<Rc<Cell<bool>>>,
    wants_input: bool,
    wants_key_release: bool,
}

impl Component for TestComponent {
    fn render(&mut self, width: usize) -> Vec<String> {
        let mut state = self.state.borrow_mut();
        state.render_count += 1;
        state.last_render_width = Some(width);
        state.lines.clone()
    }

    fn handle_input(&mut self, data: &str) {
        let mut state = self.state.borrow_mut();
        state.inputs.push(data.to_string());
        let handler = state.on_input.clone();
        drop(state);
        if let Some(handler) = handler {
            handler(data);
        }
    }

    fn wants_input(&self) -> bool {
        self.wants_input
    }

    fn wants_key_release(&self) -> bool {
        self.wants_key_release
    }

    fn as_focusable_mut(&mut self) -> Option<&mut dyn Focusable> {
        if self.focus.is_some() { Some(self) } else { None }
    }
}

impl Focusable for TestComponent {
    fn focused(&self) -> bool {
        self.focus.as_ref().is_some_and(|focus| focus.get())
    }

    fn set_focused(&mut self, focused: bool) {
        if let Some(flag) = &self.focus {
            flag.set(focused);
        }
    }
}

/// Create a non-focusable test component and its observable state.
pub fn test_component(lines: &[&str]) -> (ComponentRef, Rc<RefCell<TestState>>) {
    let state = Rc::new(RefCell::new(TestState {
        lines: lines.iter().map(|line| (*line).to_string()).collect(),
        ..TestState::default()
    }));
    let component = component(TestComponent {
        state: Rc::clone(&state),
        focus: None,
        wants_input: true,
        wants_key_release: false,
    });
    (component, state)
}

/// Create a focusable test component, its state, and its focus flag.
pub fn focusable_component(lines: &[&str]) -> (ComponentRef, Rc<RefCell<TestState>>, Rc<Cell<bool>>) {
    let state = Rc::new(RefCell::new(TestState {
        lines: lines.iter().map(|line| (*line).to_string()).collect(),
        ..TestState::default()
    }));
    let focus = Rc::new(Cell::new(false));
    let component = component(TestComponent {
        state: Rc::clone(&state),
        focus: Some(Rc::clone(&focus)),
        wants_input: true,
        wants_key_release: false,
    });
    (component, state, focus)
}

/// Component with no input handler (`handleInput` absent in the source).
pub fn passive_component(lines: &[&str]) -> (ComponentRef, Rc<RefCell<TestState>>) {
    let state = Rc::new(RefCell::new(TestState {
        lines: lines.iter().map(|line| (*line).to_string()).collect(),
        ..TestState::default()
    }));
    let component = component(TestComponent {
        state: Rc::clone(&state),
        focus: None,
        wants_input: false,
        wants_key_release: false,
    });
    (component, state)
}

/// Container plus its component handle.
pub fn container() -> (Rc<RefCell<Container>>, ComponentRef) {
    let container = Rc::new(RefCell::new(Container::new()));
    let component: ComponentRef = container.clone();
    (container, component)
}

pub fn set_lines(state: &Rc<RefCell<TestState>>, lines: &[&str]) {
    state.borrow_mut().set_lines(lines);
}

pub fn set_lines_owned(state: &Rc<RefCell<TestState>>, lines: Vec<String>) {
    state.borrow_mut().lines = lines;
}

pub fn render_count(state: &Rc<RefCell<TestState>>) -> usize {
    state.borrow().render_count
}

pub fn inputs(state: &Rc<RefCell<TestState>>) -> Vec<String> {
    state.borrow().inputs.clone()
}
