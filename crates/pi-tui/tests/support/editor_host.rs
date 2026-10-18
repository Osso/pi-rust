//! Shared test host for the ported editor tests.

#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use pi_tui::components::editor::{Editor, EditorFixedCell, EditorHost, EditorOptions, EditorTheme};
use pi_tui::tui::Component;

#[derive(Default)]
pub struct FakeHostState {
    pub rows: usize,
    pub render_count: usize,
    pub writes: Vec<FixedCellWrite>,
    pub position: Option<(i64, i64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixedCellWrite {
    pub row: i64,
    pub col: i64,
    pub value: String,
}

pub struct FakeHost {
    state: Rc<RefCell<FakeHostState>>,
}

impl FakeHost {
    pub fn state(&self) -> Rc<RefCell<FakeHostState>> {
        Rc::clone(&self.state)
    }
}

pub struct FakeFixedCell {
    state: Rc<RefCell<FakeHostState>>,
}

impl EditorFixedCell for FakeFixedCell {
    fn place(&self, row: i64, col: i64) {
        self.state.borrow_mut().position = Some((row, col));
    }

    fn clear(&self) {
        self.state.borrow_mut().position = None;
    }

    fn update(&self, value: &str) -> bool {
        let mut state = self.state.borrow_mut();
        let Some((row, col)) = state.position else {
            return false;
        };
        state.writes.push(FixedCellWrite {
            row,
            col,
            value: value.to_string(),
        });
        true
    }
}

impl EditorHost for FakeHost {
    fn terminal_rows(&self) -> usize {
        self.state.borrow().rows
    }

    fn request_render(&self) {
        self.state.borrow_mut().render_count += 1;
    }

    fn create_fixed_cell(&self) -> Rc<dyn EditorFixedCell> {
        Rc::new(FakeFixedCell {
            state: Rc::clone(&self.state),
        })
    }
}

pub fn fake_host(rows: usize) -> (Rc<FakeHost>, Rc<RefCell<FakeHostState>>) {
    let state = Rc::new(RefCell::new(FakeHostState {
        rows,
        ..FakeHostState::default()
    }));
    (
        Rc::new(FakeHost {
            state: Rc::clone(&state),
        }),
        state,
    )
}

pub fn plain_theme() -> EditorTheme {
    EditorTheme::plain()
}

pub fn editor_with_rows(rows: usize) -> (Editor, Rc<RefCell<FakeHostState>>) {
    let (host, state) = fake_host(rows);
    let editor = Editor::new(host, plain_theme(), EditorOptions::default());
    (editor, state)
}

pub fn editor() -> (Editor, Rc<RefCell<FakeHostState>>) {
    editor_with_rows(24)
}

pub fn send(editor: &mut Editor, input: &str) {
    editor.handle_input(input);
}

pub fn cursor(editor: &Editor) -> (usize, usize) {
    editor.get_cursor()
}

pub fn render_plain(editor: &mut Editor, width: usize) -> Vec<String> {
    editor
        .render(width)
        .into_iter()
        .map(|line| pi_tui::utils::strip_terminal_sequences(&line))
        .collect()
}
