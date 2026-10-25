use crate::shared_components as components;
use crate::shared_oracle as oracle;
use crate::shared_virtual_terminal as virtual_terminal;

use std::cell::RefCell;
use std::rc::Rc;

use components::{TestState, container, render_count, set_lines, test_component};
use pi_tui::root_compositor::{RootCompositor, RootCompositorOptions, RootLayoutEntry};
use pi_tui::terminal::Terminal;
use pi_tui::tui::{CURSOR_MARKER, RenderRegion, TuiExt, TuiRenderer};
use virtual_terminal::{VirtualTerminal, concrete};

fn wait<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>) {
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();
}

struct Fixture {
    terminal: Rc<RefCell<VirtualTerminal>>,
    tui: Rc<TuiRenderer>,
    region_root: Rc<RefCell<pi_tui::tui::Container>>,
    region: Rc<RenderRegion>,
    descendant: Rc<RefCell<TestState>>,
    unrelated: Rc<RefCell<TestState>>,
}

fn fixture() -> Fixture {
    let (terminal, handle) = concrete(24, 8);
    let tui = TuiRenderer::new(handle);
    let (region_root, region_component) = container();
    let (descendant_component, descendant) = test_component(&["region one", "region two"]);
    let (unrelated_component, unrelated) = test_component(&["unrelated row"]);
    region_root.borrow_mut().add_child(descendant_component);
    let region = Rc::new(tui.create_render_region(region_component.clone()));
    let placed = Rc::clone(&region);
    let root = RootCompositor::new(RootCompositorOptions {
        get_height: {
            let terminal = Rc::clone(&terminal);
            Rc::new(move || terminal.borrow().rows())
        },
        flow: vec![
            RootLayoutEntry::new(region_component).with_on_layout(Rc::new(move |rect| placed.place(rect))),
            RootLayoutEntry::new(unrelated_component),
        ],
        bottom: Vec::new(),
    });
    tui.add_child(Rc::new(RefCell::new(root)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    terminal.borrow_mut().clear_writes();
    Fixture {
        terminal,
        tui,
        region_root,
        region,
        descendant,
        unrelated,
    }
}

fn start_fixture() -> Fixture {
    fixture()
}

fn stop_fixture(fixture: &Fixture) {
    fixture.tui.stop().unwrap();
    fixture.terminal.borrow_mut().flush();
}

#[test]
fn renders_a_same_height_descendant_update_only_inside_its_region() {
    let f = start_fixture();
    let unrelated_renders = render_count(&f.unrelated);
    set_lines(&f.descendant, &["region changed", "second changed"]);

    assert!(f.tui.request_component_render(&descendant_ref(&f)).unwrap());
    wait(&f.tui, &f.terminal);

    assert_eq!(
        &f.terminal.borrow().viewport()[..3],
        ["region changed", "second changed", "unrelated row"]
    );
    assert_eq!(render_count(&f.unrelated), unrelated_renders);
    assert!(!f.terminal.borrow().writes().is_empty());
    stop_fixture(&f);
}

fn descendant_ref(f: &Fixture) -> pi_tui::tui::ComponentRef {
    f.region_root.borrow().children()[0].clone()
}

#[test]
fn grows_a_flow_tail_without_rendering_surrounding_components() {
    let (terminal, handle) = concrete(24, 8);
    let tui = TuiRenderer::new(handle);
    let (history_component, history_state) = test_component(&["history"]);
    let (tail_root, tail_component) = container();
    let (tail, tail_state) = test_component(&["tail one"]);
    let (following_component, following_state) = test_component(&["following"]);
    let (editor_component, editor_state) = test_component(&["editor"]);
    let (footer_component, footer_state) = test_component(&["footer"]);
    tail_root.borrow_mut().add_child(tail.clone());
    let region = Rc::new(tui.create_flow_render_region(tail_component.clone()));
    let placed = Rc::clone(&region);
    let root = RootCompositor::new(RootCompositorOptions {
        get_height: {
            let terminal = Rc::clone(&terminal);
            Rc::new(move || terminal.borrow().rows())
        },
        flow: vec![
            RootLayoutEntry::new(history_component),
            RootLayoutEntry::new(tail_component).with_on_flow_layout(Rc::new(move |layout| placed.place(layout))),
            RootLayoutEntry::new(following_component),
        ],
        bottom: vec![
            RootLayoutEntry::new(editor_component),
            RootLayoutEntry::new(footer_component),
        ],
    });
    tui.add_child(Rc::new(RefCell::new(root)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    terminal.borrow_mut().clear_writes();
    let history_renders = render_count(&history_state);
    let following_renders = render_count(&following_state);
    let editor_renders = render_count(&editor_state);
    let footer_renders = render_count(&footer_state);
    set_lines(&tail_state, &["tail one", "tail two", "tail three"]);

    assert!(tui.request_component_render(&tail).unwrap());
    wait(&tui, &terminal);

    assert_eq!(
        terminal.borrow().viewport(),
        vec![
            "history",
            "tail one",
            "tail two",
            "tail three",
            "following",
            "",
            "editor",
            "footer"
        ]
    );
    assert_eq!(render_count(&history_state), history_renders);
    assert_eq!(render_count(&following_state), following_renders);
    assert_eq!(render_count(&editor_state), editor_renders);
    assert_eq!(render_count(&footer_state), footer_renders);
    assert!(!terminal.borrow().writes().is_empty());
    tui.stop().unwrap();
}

#[test]
fn appends_and_routes_a_new_descendant_from_an_empty_flow_tail() {
    let (terminal, handle) = concrete(24, 6);
    let tui = TuiRenderer::new(handle);
    let (history_component, history_state) = test_component(&["history"]);
    let (tail_root, tail_component) = container();
    let (pending_component, pending_state) = test_component(&["pending"]);
    let (editor_component, editor_state) = test_component(&["editor"]);
    let (footer_component, footer_state) = test_component(&["footer"]);
    let region = Rc::new(tui.create_flow_render_region(tail_component.clone()));
    let placed = Rc::clone(&region);
    let root = RootCompositor::new(RootCompositorOptions {
        get_height: {
            let terminal = Rc::clone(&terminal);
            Rc::new(move || terminal.borrow().rows())
        },
        flow: vec![
            RootLayoutEntry::new(history_component),
            RootLayoutEntry::new(tail_component).with_on_flow_layout(Rc::new(move |layout| placed.place(layout))),
            RootLayoutEntry::new(pending_component),
        ],
        bottom: vec![
            RootLayoutEntry::new(editor_component),
            RootLayoutEntry::new(footer_component),
        ],
    });
    tui.add_child(Rc::new(RefCell::new(root)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    let unchanged = (
        render_count(&history_state),
        render_count(&pending_state),
        render_count(&editor_state),
        render_count(&footer_state),
    );
    let (message, message_state) = test_component(&["new message"]);
    tail_root.borrow_mut().add_child(message.clone());

    assert!(region.request_render().unwrap());
    wait(&tui, &terminal);
    set_lines(&message_state, &["updated message"]);
    assert!(tui.request_component_render(&message).unwrap());
    wait(&tui, &terminal);

    assert_eq!(
        terminal.borrow().viewport(),
        vec!["history", "updated message", "pending", "", "editor", "footer"]
    );
    assert_eq!(render_count(&history_state), unchanged.0);
    assert_eq!(render_count(&pending_state), unchanged.1);
    assert_eq!(render_count(&editor_state), unchanged.2);
    assert_eq!(render_count(&footer_state), unchanged.3);
    tui.stop().unwrap();
}

#[test]
fn updates_the_visible_rows_of_a_flow_tail_that_starts_above_the_viewport() {
    let (terminal, handle) = concrete(24, 4);
    let tui = TuiRenderer::new(handle);
    let (tail, tail_state) = test_component(&["tail one", "tail two", "tail three", "tail four", "tail five"]);
    let (editor_component, editor_state) = test_component(&["editor"]);
    let region = Rc::new(tui.create_flow_render_region(tail.clone()));
    let placed = Rc::clone(&region);
    let root = RootCompositor::new(RootCompositorOptions {
        get_height: {
            let terminal = Rc::clone(&terminal);
            Rc::new(move || terminal.borrow().rows())
        },
        flow: vec![RootLayoutEntry::new(tail.clone()).with_on_flow_layout(Rc::new(move |layout| placed.place(layout)))],
        bottom: vec![RootLayoutEntry::new(editor_component)],
    });
    tui.add_child(Rc::new(RefCell::new(root)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    let editor_renders = render_count(&editor_state);
    set_lines(
        &tail_state,
        &["tail one", "tail two", "tail three", "tail four", "tail changed"],
    );

    assert!(tui.request_component_render(&tail).unwrap());
    wait(&tui, &terminal);

    assert_eq!(
        terminal.borrow().viewport(),
        vec!["tail three", "tail four", "tail changed", "editor"]
    );
    assert_eq!(render_count(&editor_state), editor_renders);
    tui.stop().unwrap();
}

#[test]
fn grows_a_visible_flow_tail_while_preserving_terminal_scrollback() {
    let (terminal, handle) = concrete(24, 4);
    let tui = TuiRenderer::new(handle);
    let (history_component, history_state) = test_component(&["history one", "history two", "history three"]);
    let (tail, tail_state) = test_component(&["tail one"]);
    let (editor_component, editor_state) = test_component(&["editor"]);
    let region = Rc::new(tui.create_flow_render_region(tail.clone()));
    let placed = Rc::clone(&region);
    let root = RootCompositor::new(RootCompositorOptions {
        get_height: {
            let terminal = Rc::clone(&terminal);
            Rc::new(move || terminal.borrow().rows())
        },
        flow: vec![
            RootLayoutEntry::new(history_component),
            RootLayoutEntry::new(tail.clone()).with_on_flow_layout(Rc::new(move |layout| placed.place(layout))),
        ],
        bottom: vec![RootLayoutEntry::new(editor_component)],
    });
    tui.add_child(Rc::new(RefCell::new(root)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    let history_renders = render_count(&history_state);
    let editor_renders = render_count(&editor_state);
    set_lines(&tail_state, &["tail one", "tail two"]);

    assert!(tui.request_component_render(&tail).unwrap());
    wait(&tui, &terminal);

    assert_eq!(
        terminal.borrow().viewport(),
        vec!["history three", "tail one", "tail two", "editor"]
    );
    let scroll_buffer = terminal.borrow().scroll_buffer();
    assert_eq!(
        &scroll_buffer[scroll_buffer.len() - 6..],
        [
            "history one",
            "history two",
            "history three",
            "tail one",
            "tail two",
            "editor"
        ]
    );
    assert_eq!(render_count(&history_state), history_renders);
    assert_eq!(render_count(&editor_state), editor_renders);
    tui.stop().unwrap();
}

#[test]
fn preserves_a_bottom_hardware_cursor_while_a_flow_tail_grows() {
    let (terminal, handle) = concrete(24, 4);
    let tui = TuiRenderer::new(handle);
    let (history_component, _history_state) = test_component(&["history one", "history two", "history three"]);
    let (tail, tail_state) = test_component(&["tail one"]);
    let (editor_component, editor_state) = test_component(&[&format!("editor {CURSOR_MARKER}")]);
    let region = Rc::new(tui.create_flow_render_region(tail.clone()));
    let placed = Rc::clone(&region);
    let root = RootCompositor::new(RootCompositorOptions {
        get_height: {
            let terminal = Rc::clone(&terminal);
            Rc::new(move || terminal.borrow().rows())
        },
        flow: vec![
            RootLayoutEntry::new(history_component),
            RootLayoutEntry::new(tail.clone()).with_on_flow_layout(Rc::new(move |layout| placed.place(layout))),
        ],
        bottom: vec![RootLayoutEntry::new(editor_component)],
    });
    tui.add_child(Rc::new(RefCell::new(root)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    let editor_renders = render_count(&editor_state);
    set_lines(&tail_state, &["tail one", "tail two"]);

    assert!(tui.request_component_render(&tail).unwrap());
    wait(&tui, &terminal);

    // The marker is stripped; the screen model trims the trailing cursor cell.
    assert_eq!(
        terminal.borrow().viewport(),
        vec!["history three", "tail one", "tail two", "editor"]
    );
    assert_eq!(terminal.borrow().cursor_position(), (7, 3));
    assert_eq!(render_count(&editor_state), editor_renders);
    tui.stop().unwrap();
}

#[test]
fn shifts_a_fixed_bottom_cell_after_flow_tail_growth() {
    let (terminal, handle) = concrete(24, 4);
    let tui = TuiRenderer::new(handle);
    let (history_component, _history_state) = test_component(&["history one", "history two", "history three"]);
    let (tail, tail_state) = test_component(&["tail one"]);
    let (editor_component, _editor_state) = test_component(&["editor"]);
    let tail_region = Rc::new(tui.create_flow_render_region(tail.clone()));
    let placed = Rc::clone(&tail_region);
    let fixed_cell = tui.create_fixed_cell();
    fixed_cell.update("*");
    let cell_for_layout = fixed_cell.clone();
    let root = RootCompositor::new(RootCompositorOptions {
        get_height: {
            let terminal = Rc::clone(&terminal);
            Rc::new(move || terminal.borrow().rows())
        },
        flow: vec![
            RootLayoutEntry::new(history_component),
            RootLayoutEntry::new(tail.clone()).with_on_flow_layout(Rc::new(move |layout| placed.place(layout))),
        ],
        bottom: vec![
            RootLayoutEntry::new(editor_component)
                .with_on_layout(Rc::new(move |rect| cell_for_layout.place(rect.row, 0))),
        ],
    });
    tui.add_child(Rc::new(RefCell::new(root)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    set_lines(&tail_state, &["tail one", "tail two"]);
    assert!(tui.request_component_render(&tail).unwrap());
    wait(&tui, &terminal);

    assert!(fixed_cell.update("#"));
    terminal.borrow_mut().flush();

    assert_eq!(
        terminal.borrow().viewport(),
        vec!["history three", "tail one", "tail two", "#ditor"]
    );
    tui.stop().unwrap();
}

#[test]
fn shifts_a_registered_bottom_region_after_flow_tail_growth() {
    let (terminal, handle) = concrete(24, 4);
    let tui = TuiRenderer::new(handle);
    let (history_component, _history_state) = test_component(&["history one", "history two", "history three"]);
    let (tail, tail_state) = test_component(&["tail one"]);
    let (status_component, status_state) = test_component(&["status"]);
    let tail_region = Rc::new(tui.create_flow_render_region(tail.clone()));
    let tail_placed = Rc::clone(&tail_region);
    let status_region = Rc::new(tui.create_render_region(status_component.clone()));
    let status_placed = Rc::clone(&status_region);
    let root = RootCompositor::new(RootCompositorOptions {
        get_height: {
            let terminal = Rc::clone(&terminal);
            Rc::new(move || terminal.borrow().rows())
        },
        flow: vec![
            RootLayoutEntry::new(history_component),
            RootLayoutEntry::new(tail.clone()).with_on_flow_layout(Rc::new(move |layout| tail_placed.place(layout))),
        ],
        bottom: vec![
            RootLayoutEntry::new(status_component.clone())
                .with_on_layout(Rc::new(move |rect| status_placed.place(rect))),
        ],
    });
    tui.add_child(Rc::new(RefCell::new(root)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    set_lines(&tail_state, &["tail one", "tail two"]);
    assert!(tui.request_component_render(&tail).unwrap());
    wait(&tui, &terminal);
    set_lines(&status_state, &["status changed"]);

    assert!(tui.request_component_render(&status_component).unwrap());
    wait(&tui, &terminal);

    assert_eq!(
        terminal.borrow().viewport(),
        vec!["history three", "tail one", "tail two", "status changed"]
    );
    tui.stop().unwrap();
}

#[test]
fn synchronizes_the_previous_frame_after_a_partial_region_update() {
    let f = start_fixture();
    set_lines(&f.descendant, &["cached region", "cached second"]);
    assert!(f.tui.request_component_render(&descendant_ref(&f)).unwrap());
    wait(&f.tui, &f.terminal);
    f.terminal.borrow_mut().clear_writes();

    f.tui.request_render(false);
    wait(&f.tui, &f.terminal);

    assert_eq!(f.terminal.borrow().writes(), "");
    assert_eq!(
        &f.terminal.borrow().viewport()[..3],
        ["cached region", "cached second", "unrelated row"]
    );
    stop_fixture(&f);
}

#[test]
fn does_not_mutate_component_owned_cached_line_arrays() {
    let (terminal, handle) = concrete(24, 8);
    let tui = TuiRenderer::new(handle);
    let (component, state) = test_component(&["initial one", "initial two"]);
    let region = Rc::new(tui.create_render_region(component.clone()));
    let placed = Rc::clone(&region);
    let root = RootCompositor::new(RootCompositorOptions {
        get_height: {
            let terminal = Rc::clone(&terminal);
            Rc::new(move || terminal.borrow().rows())
        },
        flow: vec![RootLayoutEntry::new(component.clone()).with_on_layout(Rc::new(move |rect| placed.place(rect)))],
        bottom: Vec::new(),
    });
    tui.add_child(Rc::new(RefCell::new(root)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    set_lines(&state, &["owned region", "owned second"]);
    let owned_lines = state.borrow().lines.clone();

    assert!(tui.request_component_render(&component).unwrap());
    wait(&tui, &terminal);

    assert_eq!(owned_lines, vec!["owned region", "owned second"]);
    tui.stop().unwrap();
}

#[test]
fn refreshes_descendant_routing_after_a_same_height_subtree_replacement() {
    let f = start_fixture();
    let (replacement, replacement_state) = test_component(&["replacement one", "replacement two"]);
    f.region_root.borrow_mut().clear();
    f.region_root.borrow_mut().add_child(replacement.clone());

    assert!(f.region.request_render().unwrap());
    wait(&f.tui, &f.terminal);
    let unrelated_renders = render_count(&f.unrelated);
    set_lines(&replacement_state, &["replacement changed", "replacement second"]);

    assert!(f.tui.request_component_render(&replacement).unwrap());
    wait(&f.tui, &f.terminal);

    assert_eq!(render_count(&f.unrelated), unrelated_renders);
    assert_eq!(f.terminal.borrow().viewport()[0], "replacement changed");
    stop_fixture(&f);
}

#[test]
fn falls_back_to_a_normal_root_render_when_a_flow_tail_shrinks() {
    let (terminal, handle) = concrete(24, 6);
    let tui = TuiRenderer::new(handle);
    let (history_component, history_state) = test_component(&["history"]);
    let (tail, tail_state) = test_component(&["tail one", "tail two"]);
    let (pending_component, pending_state) = test_component(&["pending"]);
    let (editor_component, editor_state) = test_component(&["editor"]);
    let region = Rc::new(tui.create_flow_render_region(tail.clone()));
    let placed = Rc::clone(&region);
    let root = RootCompositor::new(RootCompositorOptions {
        get_height: {
            let terminal = Rc::clone(&terminal);
            Rc::new(move || terminal.borrow().rows())
        },
        flow: vec![
            RootLayoutEntry::new(history_component),
            RootLayoutEntry::new(tail.clone()).with_on_flow_layout(Rc::new(move |layout| placed.place(layout))),
            RootLayoutEntry::new(pending_component),
        ],
        bottom: vec![RootLayoutEntry::new(editor_component)],
    });
    tui.add_child(Rc::new(RefCell::new(root)));
    tui.start().unwrap();
    wait(&tui, &terminal);
    let history_renders = render_count(&history_state);
    let pending_renders = render_count(&pending_state);
    let editor_renders = render_count(&editor_state);
    set_lines(&tail_state, &["tail one"]);

    assert!(!tui.request_component_render(&tail).unwrap());
    wait(&tui, &terminal);

    assert!(render_count(&history_state) > history_renders);
    assert!(render_count(&pending_state) > pending_renders);
    assert!(render_count(&editor_state) > editor_renders);
    assert_eq!(
        terminal.borrow().viewport(),
        vec!["history", "tail one", "pending", "", "", "editor"]
    );
    tui.stop().unwrap();
}

#[test]
fn falls_back_to_a_normal_root_render_when_the_region_height_changes() {
    let f = start_fixture();
    let descendant_renders = render_count(&f.descendant);
    let unrelated_renders = render_count(&f.unrelated);
    set_lines(&f.descendant, &["expanded one", "expanded two", "expanded three"]);

    assert!(!f.tui.request_component_render(&descendant_ref(&f)).unwrap());
    wait(&f.tui, &f.terminal);

    assert!(render_count(&f.descendant) > descendant_renders);
    assert!(render_count(&f.unrelated) > unrelated_renders);
    assert_eq!(
        &f.terminal.borrow().viewport()[..4],
        ["expanded one", "expanded two", "expanded three", "unrelated row"]
    );
    stop_fixture(&f);
}

#[test]
fn uses_normal_rendering_when_render_resize_or_overlay_makes_placement_unsafe() {
    let pending = start_fixture();
    let pending_descendant_renders = render_count(&pending.descendant);
    let pending_unrelated_renders = render_count(&pending.unrelated);
    set_lines(&pending.descendant, &["pending region", "pending second"]);
    pending.tui.request_render(false);
    assert!(!pending.tui.request_component_render(&descendant_ref(&pending)).unwrap());
    wait(&pending.tui, &pending.terminal);
    assert_eq!(render_count(&pending.descendant), pending_descendant_renders + 1);
    assert_eq!(render_count(&pending.unrelated), pending_unrelated_renders + 1);
    assert_eq!(pending.terminal.borrow().viewport()[0], "pending region");
    stop_fixture(&pending);

    let resized = start_fixture();
    let resized_descendant_renders = render_count(&resized.descendant);
    let resized_unrelated_renders = render_count(&resized.unrelated);
    set_lines(&resized.descendant, &["resized region", "resized second"]);
    resized.terminal.borrow_mut().resize(30, 8);
    assert!(!resized.tui.request_component_render(&descendant_ref(&resized)).unwrap());
    wait(&resized.tui, &resized.terminal);
    assert_eq!(render_count(&resized.descendant), resized_descendant_renders + 1);
    assert_eq!(render_count(&resized.unrelated), resized_unrelated_renders + 1);
    assert_eq!(resized.terminal.borrow().viewport()[0], "resized region");
    stop_fixture(&resized);

    let overlay = start_fixture();
    let (overlay_component, _overlay_state) = test_component(&["visible overlay"]);
    let overlay_handle = overlay.tui.show_overlay(
        overlay_component,
        Some(pi_tui::tui::OverlayOptions {
            row: Some(pi_tui::tui::SizeValue::Absolute(0.0)),
            col: Some(pi_tui::tui::SizeValue::Absolute(0.0)),
            width: Some(pi_tui::tui::SizeValue::Absolute(16.0)),
            ..Default::default()
        }),
    );
    wait(&overlay.tui, &overlay.terminal);
    overlay.terminal.borrow_mut().clear_writes();
    let unrelated_renders = render_count(&overlay.unrelated);
    set_lines(&overlay.descendant, &["covered region", "covered second"]);

    assert!(!overlay.tui.request_component_render(&descendant_ref(&overlay)).unwrap());
    wait(&overlay.tui, &overlay.terminal);

    assert!(overlay.terminal.borrow().viewport()[0].starts_with("visible overlay"));
    assert!(render_count(&overlay.unrelated) > unrelated_renders);
    assert!(!overlay.terminal.borrow().writes().is_empty());
    overlay_handle.hide();
    wait(&overlay.tui, &overlay.terminal);
    stop_fixture(&overlay);
}

#[test]
fn rejects_a_placement_not_refreshed_by_the_latest_root_render() {
    let (terminal, handle) = concrete(24, 8);
    let tui = TuiRenderer::new(handle);
    let (region_root, region_component) = container();
    let (descendant, descendant_state) = test_component(&["region"]);
    let (unrelated_component, unrelated_state) = test_component(&["unrelated"]);
    region_root.borrow_mut().add_child(descendant.clone());
    let region = Rc::new(tui.create_render_region(region_component.clone()));
    let placed = Rc::clone(&region);
    let report_layout = Rc::new(std::cell::Cell::new(true));
    let report_for_layout = Rc::clone(&report_layout);
    let root = RootCompositor::new(RootCompositorOptions {
        get_height: {
            let terminal = Rc::clone(&terminal);
            Rc::new(move || terminal.borrow().rows())
        },
        flow: vec![
            RootLayoutEntry::new(region_component).with_on_layout(Rc::new(move |rect| {
                if report_for_layout.get() {
                    placed.place(rect);
                }
            })),
            RootLayoutEntry::new(unrelated_component),
        ],
        bottom: Vec::new(),
    });
    tui.add_child(Rc::new(RefCell::new(root)));
    tui.start().unwrap();
    wait(&tui, &terminal);

    report_layout.set(false);
    tui.request_render(false);
    wait(&tui, &terminal);
    let unrelated_renders = render_count(&unrelated_state);
    set_lines(&descendant_state, &["stale update"]);

    assert!(!tui.request_component_render(&descendant).unwrap());
    wait(&tui, &terminal);

    assert!(render_count(&unrelated_state) > unrelated_renders);
    assert_eq!(terminal.borrow().viewport()[0], "stale update");
    tui.stop().unwrap();
}

#[test]
fn falls_back_to_normal_rendering_for_an_unregistered_component() {
    let f = start_fixture();
    let (unregistered, _unregistered_state) = test_component(&["not in the root"]);
    let descendant_renders = render_count(&f.descendant);
    let unrelated_renders = render_count(&f.unrelated);

    assert!(!f.tui.request_component_render(&unregistered).unwrap());
    wait(&f.tui, &f.terminal);

    assert!(render_count(&f.descendant) > descendant_renders);
    assert!(render_count(&f.unrelated) > unrelated_renders);
    assert_eq!(
        &f.terminal.borrow().viewport()[..3],
        ["region one", "region two", "unrelated row"]
    );
    stop_fixture(&f);
}

#[test]
fn try_render_failure_does_not_schedule_root_render() {
    let f = fixture();
    f.region.clear();
    let before = render_count(&f.unrelated);
    assert!(!f.region.try_render().unwrap());
    wait(&f.tui, &f.terminal);
    assert_eq!(render_count(&f.unrelated), before);
    assert_eq!(f.terminal.borrow().writes(), "");
    assert!(!f.region.request_render().unwrap());
    wait(&f.tui, &f.terminal);
    assert!(render_count(&f.unrelated) > before);
    let expected = oracle::TsOracle::spawn("tui-verifier-oracle.mjs", "packages/tui/src/tui.ts")
        .request(&serde_json::json!({"scenario": "region"}));
    assert_eq!(serde_json::json!([false, false, false, true]), expected);
    stop_fixture(&f);
}

#[test]
fn try_render_writes_valid_region_without_rendering_surroundings() {
    let f = fixture();
    let before = render_count(&f.unrelated);
    set_lines(&f.descendant, &["changed", "second"]);
    assert!(f.region.try_render().unwrap());
    wait(&f.tui, &f.terminal);
    assert_eq!(
        &f.terminal.borrow().viewport()[..3],
        ["changed", "second", "unrelated row"]
    );
    assert_eq!(render_count(&f.unrelated), before);
    f.region.dispose();
    assert!(!f.region.try_render().unwrap());
    stop_fixture(&f);
}
