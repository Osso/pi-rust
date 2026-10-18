//! Generic undo stack with clone-on-push semantics.
//!
//! Stores clones of state snapshots. Popped snapshots are returned directly
//! because they are already detached. TypeScript uses `structuredClone`; Rust
//! callers supply the equivalent deep copy through [`Clone`].

/// Last-in-first-out stack of cloned state snapshots.
#[derive(Debug, Default)]
pub struct UndoStack<S> {
    stack: Vec<S>,
}

impl<S> UndoStack<S> {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Remove all snapshots.
    pub fn clear(&mut self) {
        self.stack.clear();
    }
}

impl<S: Clone> UndoStack<S> {
    /// Push a clone of the given state onto the stack.
    pub fn push(&mut self, state: &S) {
        self.stack.push(state.clone());
    }

    /// Pop and return the most recent snapshot, or `None` if empty.
    pub fn pop(&mut self) -> Option<S> {
        self.stack.pop()
    }
}
