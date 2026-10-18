use pi_tui::undo_stack::UndoStack;

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    text: String,
    cursor: usize,
}

#[test]
fn pop_on_empty_stack_returns_none() {
    let mut stack: UndoStack<Snapshot> = UndoStack::new();
    assert_eq!(stack.pop(), None);
    assert_eq!(stack.len(), 0);
}

#[test]
fn push_stores_a_clone_and_pop_returns_it() {
    let mut stack = UndoStack::new();
    let state = Snapshot {
        text: "hello".to_string(),
        cursor: 5,
    };
    stack.push(&state);
    assert_eq!(stack.len(), 1);

    let popped = stack.pop().expect("snapshot");
    assert_eq!(popped, state);
    assert_eq!(stack.len(), 0);
}

#[test]
fn mutating_the_source_after_push_does_not_change_the_snapshot() {
    let mut stack = UndoStack::new();
    let mut state = Snapshot {
        text: "first".to_string(),
        cursor: 1,
    };
    stack.push(&state);
    state.text = "second".to_string();
    state.cursor = 2;

    assert_eq!(
        stack.pop(),
        Some(Snapshot {
            text: "first".to_string(),
            cursor: 1,
        })
    );
}

#[test]
fn pop_returns_snapshots_in_last_in_first_out_order() {
    let mut stack = UndoStack::new();
    stack.push(&Snapshot {
        text: "one".to_string(),
        cursor: 0,
    });
    stack.push(&Snapshot {
        text: "two".to_string(),
        cursor: 0,
    });
    stack.push(&Snapshot {
        text: "three".to_string(),
        cursor: 0,
    });

    assert_eq!(stack.pop().unwrap().text, "three");
    assert_eq!(stack.pop().unwrap().text, "two");
    assert_eq!(stack.pop().unwrap().text, "one");
    assert_eq!(stack.pop(), None);
}

#[test]
fn clear_removes_all_snapshots() {
    let mut stack = UndoStack::new();
    stack.push(&Snapshot {
        text: "one".to_string(),
        cursor: 0,
    });
    stack.push(&Snapshot {
        text: "two".to_string(),
        cursor: 0,
    });
    stack.clear();
    assert_eq!(stack.len(), 0);
    assert_eq!(stack.pop(), None);
}
