use crate::shared_components as components;
use crate::shared_virtual_terminal as virtual_terminal;

use std::cell::RefCell;
use std::rc::Rc;

use components::{TestState, container, render_count, set_lines, test_component};
use pi_tui::root_compositor::{RootCompositor, RootCompositorOptions, RootLayoutEntry};
use pi_tui::terminal::Terminal;
use pi_tui::tui::{CURSOR_MARKER, RenderRegion, RenderRegionRect, TuiExt, TuiRenderer};
use virtual_terminal::{VirtualTerminal, concrete};

const KITTY_IMAGE_LINE: &str = "\x1b_Ga=T,f=100,t=f,d=base64data...\x1b\\\x1b_Gm=i=1;\x1b\\";

fn wait<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>) {
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();
}

struct Fixture {
    terminal: Rc<RefCell<VirtualTerminal>>,
    tui: Rc<TuiRenderer>,
    region: Rc<RenderRegion>,
    descendant: pi_tui::tui::ComponentRef,
    descendant_state: Rc<RefCell<TestState>>,
    unrelated: Rc<RefCell<TestState>>,
}

fn start_fixture(initial_lines: &[&str]) -> Fixture {
    let (terminal, handle) = concrete(24, 8);
    let tui = TuiRenderer::new(handle);
    let (region_root, region_component) = container();
    let (descendant, descendant_state) = test_component(initial_lines);
    let (unrelated_component, unrelated) = test_component(&["unrelated"]);
    region_root.borrow_mut().add_child(descendant.clone());
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
    Fixture {
        terminal,
        tui,
        region,
        descendant,
        descendant_state,
        unrelated,
    }
}

fn stop_fixture(fixture: &Fixture) {
    fixture.tui.stop().unwrap();
    fixture.terminal.borrow_mut().flush();
}

fn expect_normal_render_fallback(fixture: &Fixture) {
    let unrelated_renders = render_count(&fixture.unrelated);
    assert!(!fixture.tui.request_component_render(&fixture.descendant).unwrap());
    wait(&fixture.tui, &fixture.terminal);
    assert!(render_count(&fixture.unrelated) > unrelated_renders);
}

#[test]
fn uses_a_normal_root_render_for_non_full_width_placement() {
    let f = start_fixture(&["region one", "region two"]);
    f.region.place(RenderRegionRect {
        row: 0,
        col: 1,
        width: 23,
        height: 2,
    });
    set_lines(&f.descendant_state, &["changed one", "changed two"]);
    expect_normal_render_fallback(&f);
    stop_fixture(&f);
}

#[test]
fn uses_a_normal_root_render_for_negative_row_placement() {
    let f = start_fixture(&["region one", "region two"]);
    f.region.place(RenderRegionRect {
        row: -1,
        col: 0,
        width: 24,
        height: 2,
    });
    set_lines(&f.descendant_state, &["changed one", "changed two"]);
    expect_normal_render_fallback(&f);
    stop_fixture(&f);
}

#[test]
fn uses_a_normal_root_render_for_frame_overrun_placement() {
    let f = start_fixture(&["region one", "region two"]);
    f.region.place(RenderRegionRect {
        row: 0,
        col: 0,
        width: 24,
        height: 9,
    });
    set_lines(&f.descendant_state, &["changed one", "changed two"]);
    expect_normal_render_fallback(&f);
    stop_fixture(&f);
}

#[test]
fn uses_a_normal_root_render_for_cursor_marker_output() {
    let f = start_fixture(&["region one", "region two"]);
    set_lines(&f.descendant_state, &[&format!("cursor {CURSOR_MARKER}"), "second"]);
    expect_normal_render_fallback(&f);
    stop_fixture(&f);
}

#[test]
fn uses_a_normal_root_render_for_kitty_image_output() {
    let f = start_fixture(&["region one", "region two"]);
    set_lines(&f.descendant_state, &[KITTY_IMAGE_LINE, "second"]);
    expect_normal_render_fallback(&f);
    stop_fixture(&f);
}

#[test]
fn rejects_over_width_partial_output_before_a_normal_render_recovers() {
    let f = start_fixture(&["region one", "region two"]);
    let unrelated_renders = render_count(&f.unrelated);
    let long_line = "x".repeat(25);
    set_lines(&f.descendant_state, &[&long_line, "second"]);

    assert!(!f.tui.request_component_render(&f.descendant).unwrap());
    set_lines(&f.descendant_state, &["recovered", "second"]);
    wait(&f.tui, &f.terminal);

    assert!(render_count(&f.unrelated) > unrelated_renders);
    assert_eq!(f.terminal.borrow().viewport()[0], "recovered");
    stop_fixture(&f);
}

#[test]
fn uses_a_normal_root_render_when_cached_region_lines_contain_a_kitty_image() {
    let f = start_fixture(&[KITTY_IMAGE_LINE, "second"]);
    set_lines(&f.descendant_state, &["image removed", "second"]);
    expect_normal_render_fallback(&f);
    stop_fixture(&f);
}
