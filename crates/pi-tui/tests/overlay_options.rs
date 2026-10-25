use crate::shared_components as components;
use crate::shared_virtual_terminal as virtual_terminal;

use std::cell::RefCell;
use std::rc::Rc;

use components::{TestState, test_component};
use pi_tui::TUI;
use pi_tui::tui::{ComponentRef, OverlayAnchor, OverlayMargin, OverlayOptions, SizeValue, TuiExt};
use virtual_terminal::{VirtualTerminal, concrete};

fn wait<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>) {
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();
}

fn absolute(value: f64) -> Option<SizeValue> {
    Some(SizeValue::Absolute(value))
}

fn percent(value: f64) -> Option<SizeValue> {
    Some(SizeValue::Percent(value))
}

fn overlay(lines: &[&str]) -> (ComponentRef, Rc<RefCell<TestState>>) {
    test_component(lines)
}

#[test]
fn truncates_overlay_lines_that_exceed_declared_width() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&[&"X".repeat(100)]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            width: absolute(20.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    for line in terminal.borrow().viewport() {
        assert!(pi_tui::utils::visible_width(&line) <= 80);
    }
    tui.stop().unwrap();
}

#[test]
fn handles_overlay_with_complex_ansi_sequences_without_crashing() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let complex_line = format!(
        "\x1b[48;2;40;50;40m \x1b[38;2;128;128;128mSome styled content\x1b[39m\x1b[49m\x1b]8;;http://example.com\x07link\x1b]8;;\x07{}",
        " more content ".repeat(10)
    );
    let (content, _state) = overlay(&[&complex_line, &complex_line, &complex_line]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            width: absolute(60.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert_eq!(terminal.borrow().viewport().len(), 24);
    tui.stop().unwrap();
}

#[test]
fn handles_overlay_composited_on_styled_base_content() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (base, base_state) = test_component(&[]);
    base_state.borrow_mut().lines = vec![
        format!("\x1b[1m\x1b[38;2;255;0;0m{}\x1b[0m", "X".repeat(80)).into(),
        format!("\x1b[1m\x1b[38;2;255;0;0m{}\x1b[0m", "X".repeat(80)).into(),
        format!("\x1b[1m\x1b[38;2;255;0;0m{}\x1b[0m", "X".repeat(80)).into(),
    ];
    tui.add_child(base);
    let (content, _overlay_state) = overlay(&["OVERLAY"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            width: absolute(20.0),
            anchor: Some(OverlayAnchor::Center),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert!(terminal.borrow().viewport().iter().any(|line| line.contains("OVERLAY")));
    tui.stop().unwrap();
}

#[test]
fn handles_wide_characters_at_the_overlay_boundary() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["中文日本語한글テスト漢字"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            width: absolute(15.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert_eq!(terminal.borrow().viewport().len(), 24);
    tui.stop().unwrap();
}

#[test]
fn handles_overlay_positioned_at_the_terminal_edge() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&[&"X".repeat(50)]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            col: absolute(60.0),
            width: absolute(20.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert_eq!(terminal.borrow().viewport().len(), 24);
    tui.stop().unwrap();
}

#[test]
fn handles_overlay_on_base_content_with_osc_sequences() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (base, base_state) = test_component(&[]);
    let link = "\x1b]8;;file:///path/to/file.ts\x07file.ts\x1b]8;;\x07";
    base_state.borrow_mut().lines = vec![
        format!("See {link} for details {}", "X".repeat(50)).into(),
        format!("See {link} for details {}", "X".repeat(50)).into(),
        format!("See {link} for details {}", "X".repeat(50)).into(),
    ];
    tui.add_child(base);
    let (content, _overlay_state) = overlay(&["OVERLAY-TEXT"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::Center),
            width: absolute(20.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert_eq!(terminal.borrow().viewport().len(), 24);
    tui.stop().unwrap();
}

#[test]
fn renders_overlay_at_percentage_of_terminal_width() {
    let (terminal, handle) = concrete(100, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, state) = overlay(&["test"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            width: percent(50.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert_eq!(state.borrow().last_render_width, Some(50));
    tui.stop().unwrap();
}

#[test]
fn respects_min_width_when_percentage_results_in_smaller_width() {
    let (terminal, handle) = concrete(100, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, state) = overlay(&["test"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            width: percent(10.0),
            min_width: Some(30),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert_eq!(state.borrow().last_render_width, Some(30));
    tui.stop().unwrap();
}

#[test]
fn positions_overlay_at_top_left() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["TOP-LEFT"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::TopLeft),
            width: absolute(10.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert!(terminal.borrow().viewport()[0].starts_with("TOP-LEFT"));
    tui.stop().unwrap();
}

#[test]
fn positions_overlay_at_bottom_right() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["BTM-RIGHT"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::BottomRight),
            width: absolute(10.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    let viewport = terminal.borrow().viewport();
    assert!(viewport[23].contains("BTM-RIGHT"));
    assert!(viewport[23].trim_end().ends_with("BTM-RIGHT"));
    tui.stop().unwrap();
}

#[test]
fn positions_overlay_at_top_center() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["CENTERED"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::TopCenter),
            width: absolute(10.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    let first_row = terminal.borrow().viewport()[0].clone();
    assert!(first_row.contains("CENTERED"));
    let col = first_row.find("CENTERED").unwrap();
    assert!((30..=40).contains(&col), "expected centered, got col {col}");
    tui.stop().unwrap();
}

#[test]
fn clamps_negative_margins_to_zero() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["NEG-MARGIN"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::TopLeft),
            width: absolute(12.0),
            margin: Some(OverlayMargin {
                top: Some(-5),
                left: Some(-10),
                right: Some(0),
                bottom: Some(0),
            }),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert!(terminal.borrow().viewport()[0].starts_with("NEG-MARGIN"));
    tui.stop().unwrap();
}

#[test]
fn respects_margin_as_a_number() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["MARGIN"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::TopLeft),
            width: absolute(10.0),
            margin: Some(OverlayMargin {
                top: Some(5),
                right: Some(5),
                bottom: Some(5),
                left: Some(5),
            }),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    let viewport = terminal.borrow().viewport();
    assert!(!viewport[0].contains("MARGIN"));
    assert!(!viewport[4].contains("MARGIN"));
    assert!(viewport[5].contains("MARGIN"));
    assert_eq!(viewport[5].find("MARGIN"), Some(5));
    tui.stop().unwrap();
}

#[test]
fn respects_margin_object() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["MARGIN"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::TopLeft),
            width: absolute(10.0),
            margin: Some(OverlayMargin {
                top: Some(2),
                left: Some(3),
                right: Some(0),
                bottom: Some(0),
            }),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    let viewport = terminal.borrow().viewport();
    assert!(viewport[2].contains("MARGIN"));
    assert_eq!(viewport[2].find("MARGIN"), Some(3));
    tui.stop().unwrap();
}

#[test]
fn applies_offset_x_and_offset_y_from_anchor_position() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["OFFSET"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::TopLeft),
            width: absolute(10.0),
            offset_x: Some(10),
            offset_y: Some(5),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    let viewport = terminal.borrow().viewport();
    assert!(viewport[5].contains("OFFSET"));
    assert_eq!(viewport[5].find("OFFSET"), Some(10));
    tui.stop().unwrap();
}

#[test]
fn positions_with_row_and_col_percentages() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["PCT"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            width: absolute(10.0),
            row: percent(50.0),
            col: percent(50.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    let found_row = terminal
        .borrow()
        .viewport()
        .iter()
        .position(|line| line.contains("PCT"))
        .expect("PCT must be visible");
    assert!((10..=13).contains(&found_row), "got row {found_row}");
    tui.stop().unwrap();
}

#[test]
fn row_percent_zero_positions_at_top() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["TOP"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            width: absolute(10.0),
            row: percent(0.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert!(terminal.borrow().viewport()[0].contains("TOP"));
    tui.stop().unwrap();
}

#[test]
fn row_percent_hundred_positions_at_bottom() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["BOTTOM"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            width: absolute(10.0),
            row: percent(100.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert!(terminal.borrow().viewport()[23].contains("BOTTOM"));
    tui.stop().unwrap();
}

#[test]
fn truncates_overlay_to_max_height() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["Line 1", "Line 2", "Line 3", "Line 4", "Line 5"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            max_height: absolute(3.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    let content = terminal.borrow().viewport().join("\n");
    assert!(content.contains("Line 1"));
    assert!(content.contains("Line 2"));
    assert!(content.contains("Line 3"));
    assert!(!content.contains("Line 4"));
    assert!(!content.contains("Line 5"));
    tui.stop().unwrap();
}

#[test]
fn truncates_overlay_to_max_height_percentage() {
    let (terminal, handle) = concrete(80, 10);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let lines: Vec<String> = (1..=10).map(|index| format!("L{index}")).collect();
    let (content, _state) = test_component(&[]);
    components::set_lines_owned(&_state, lines);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            max_height: percent(50.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    let content = terminal.borrow().viewport().join("\n");
    assert!(content.contains("L1"));
    assert!(content.contains("L5"));
    assert!(!content.contains("L6"));
    tui.stop().unwrap();
}

#[test]
fn row_and_col_override_anchor() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (content, _state) = overlay(&["ABSOLUTE"]);
    tui.show_overlay(
        content,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::BottomRight),
            row: absolute(3.0),
            col: absolute(5.0),
            width: absolute(10.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    let viewport = terminal.borrow().viewport();
    assert!(viewport[3].contains("ABSOLUTE"));
    assert_eq!(viewport[3].find("ABSOLUTE"), Some(5));
    tui.stop().unwrap();
}

#[test]
fn renders_multiple_overlays_with_later_ones_on_top() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (first, _first_state) = overlay(&["FIRST-OVERLAY"]);
    tui.show_overlay(
        first,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::TopLeft),
            width: absolute(20.0),
            ..Default::default()
        }),
    );
    let (second, _second_state) = overlay(&["SECOND"]);
    tui.show_overlay(
        second,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::TopLeft),
            width: absolute(10.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    assert!(terminal.borrow().viewport()[0].contains("SECOND"));
    tui.stop().unwrap();
}

#[test]
fn handles_overlays_at_different_positions_without_interference() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (top, _top_state) = overlay(&["TOP-LEFT"]);
    tui.show_overlay(
        top,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::TopLeft),
            width: absolute(15.0),
            ..Default::default()
        }),
    );
    let (bottom, _bottom_state) = overlay(&["BTM-RIGHT"]);
    tui.show_overlay(
        bottom,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::BottomRight),
            width: absolute(15.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);

    let viewport = terminal.borrow().viewport();
    assert!(viewport[0].contains("TOP-LEFT"));
    assert!(viewport[23].contains("BTM-RIGHT"));
    tui.stop().unwrap();
}

#[test]
fn properly_hides_overlays_in_stack_order() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    let (first, _first_state) = overlay(&["FIRST"]);
    tui.show_overlay(
        first,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::TopLeft),
            width: absolute(10.0),
            ..Default::default()
        }),
    );
    let (second, _second_state) = overlay(&["SECOND"]);
    tui.show_overlay(
        second,
        Some(OverlayOptions {
            anchor: Some(OverlayAnchor::TopLeft),
            width: absolute(10.0),
            ..Default::default()
        }),
    );
    tui.start().unwrap();
    tui.request_render(true);
    wait(&tui, &terminal);
    assert!(terminal.borrow().viewport()[0].contains("SECOND"));

    tui.hide_overlay();
    tui.request_render(true);
    wait(&tui, &terminal);
    assert!(terminal.borrow().viewport()[0].contains("FIRST"));
    tui.stop().unwrap();
}
