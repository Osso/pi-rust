use crate::shared_components as components;
use crate::shared_virtual_terminal as virtual_terminal;

use pi_js_compat::js_json::JsString;
use std::cell::RefCell;
use std::rc::Rc;

use components::{TestState, container, focusable_component, inputs, test_component};
use pi_tui::TUI;
use pi_tui::tui::{ComponentRef, OverlayOptions, SizeValue, TuiExt};
use virtual_terminal::{VirtualTerminal, concrete};

fn wait<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>) {
    tui.flush_pending().unwrap();
    terminal.borrow_mut().flush();
}

fn send<T: TuiExt>(tui: &Rc<T>, terminal: &Rc<RefCell<VirtualTerminal>>, data: &str) {
    terminal.borrow_mut().send_input(data);
    tui.process_events();
}

fn non_capturing_options() -> OverlayOptions {
    OverlayOptions {
        non_capturing: true,
        ..Default::default()
    }
}

fn positioned(row: f64, col: f64, width: f64) -> OverlayOptions {
    OverlayOptions {
        row: Some(SizeValue::Absolute(row)),
        col: Some(SizeValue::Absolute(col)),
        width: Some(SizeValue::Absolute(width)),
        non_capturing: true,
        ..Default::default()
    }
}

fn set_input_handler(state: &Rc<RefCell<TestState>>, handler: impl Fn(&JsString) + 'static) {
    state.borrow_mut().on_input = Some(Rc::new(handler));
}

struct Fixture {
    terminal: Rc<RefCell<VirtualTerminal>>,
    tui: Rc<TUI>,
    editor: ComponentRef,
    editor_state: Rc<RefCell<TestState>>,
    editor_focus: Rc<std::cell::Cell<bool>>,
}

fn fixture() -> Fixture {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (editor, editor_state, editor_focus) = focusable_component(&["EDITOR"]);
    tui.add_child(test_component(&[]).0);
    tui.set_focus(Some(editor.clone()));
    tui.start().unwrap();
    Fixture {
        terminal,
        tui,
        editor,
        editor_state,
        editor_focus,
    }
}

fn overlay_component(lines: &[&str]) -> (ComponentRef, Rc<RefCell<TestState>>, Rc<std::cell::Cell<bool>>) {
    focusable_component(lines)
}

#[test]
fn non_capturing_overlay_preserves_focus_on_creation() {
    let f = fixture();
    let (overlay, _state, focus) = overlay_component(&["OVERLAY"]);
    f.tui.show_overlay(overlay, Some(non_capturing_options()));
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn focus_transfers_focus_to_the_overlay() {
    let f = fixture();
    let (overlay, _state, focus) = overlay_component(&["OVERLAY"]);
    let handle = f.tui.show_overlay(overlay, Some(non_capturing_options()));
    handle.focus();
    wait(&f.tui, &f.terminal);
    assert!(!f.editor_focus.get());
    assert!(focus.get());
    assert!(handle.is_focused());
    f.tui.stop().unwrap();
}

#[test]
fn unfocus_restores_previous_focus() {
    let f = fixture();
    let (overlay, _state, focus) = overlay_component(&["OVERLAY"]);
    let handle = f.tui.show_overlay(overlay, Some(non_capturing_options()));
    handle.focus();
    handle.unfocus();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!focus.get());
    assert!(!handle.is_focused());
    f.tui.stop().unwrap();
}

#[test]
fn set_hidden_false_on_non_capturing_overlay_does_not_auto_focus() {
    let f = fixture();
    let (overlay, _state, focus) = overlay_component(&["OVERLAY"]);
    let handle = f.tui.show_overlay(overlay, Some(non_capturing_options()));
    handle.set_hidden(true);
    handle.set_hidden(false);
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn hide_when_overlay_is_not_focused_does_not_change_focus() {
    let f = fixture();
    let (overlay, _state, _focus) = overlay_component(&["OVERLAY"]);
    let handle = f.tui.show_overlay(overlay, Some(non_capturing_options()));
    handle.hide();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn hide_when_focused_restores_focus() {
    let f = fixture();
    let (overlay, _state, focus) = overlay_component(&["OVERLAY"]);
    let handle = f.tui.show_overlay(overlay, Some(non_capturing_options()));
    handle.focus();
    handle.hide();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn capturing_overlay_removed_with_non_capturing_below_restores_focus_to_editor() {
    let f = fixture();
    let (non_capturing, _nc_state, nc_focus) = overlay_component(&["NC"]);
    let (capturing, _cap_state, cap_focus) = overlay_component(&["CAP"]);
    f.tui.show_overlay(non_capturing, Some(non_capturing_options()));
    let handle = f.tui.show_overlay(capturing, None);
    assert!(cap_focus.get());
    handle.hide();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!nc_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn sub_overlay_cleanup_then_hide_overlay_restores_focus_and_input_to_editor() {
    let f = fixture();
    let (timer, _timer_state, timer_focus) = overlay_component(&["TIMER"]);
    let (controller, controller_state, controller_focus) = overlay_component(&["CTRL"]);
    let timer_handle = f.tui.show_overlay(timer, Some(non_capturing_options()));
    f.tui.show_overlay(controller, None);
    assert!(controller_focus.get());
    assert!(!f.editor_focus.get());
    timer_handle.hide();
    f.tui.hide_overlay();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!controller_focus.get());
    assert!(!timer_focus.get());
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&f.editor_state), vec!["x".to_string()]);
    assert!(inputs(&controller_state).is_empty());
    f.tui.stop().unwrap();
}

#[test]
fn removed_focused_child_overlay_does_not_become_parent_overlay_fallback() {
    let f = fixture();
    let (child, _child_state, _child_focus) = overlay_component(&["CHILD"]);
    let (parent, _parent_state, _parent_focus) = overlay_component(&["PARENT"]);
    let child_handle = f.tui.show_overlay(child, Some(non_capturing_options()));
    child_handle.focus();
    let parent_handle = f.tui.show_overlay(parent, None);
    assert!(parent_handle.is_focused());
    child_handle.hide();
    parent_handle.hide();
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&f.editor_state), vec!["x".to_string()]);
    assert!(f.editor_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn microtask_deferred_sub_overlay_pattern_restores_focus() {
    let f = fixture();
    let (timer, _timer_state, timer_focus) = overlay_component(&["TIMER"]);
    let (controller, controller_state, controller_focus) = overlay_component(&["CTRL"]);
    let timer_handle = f.tui.show_overlay(timer, Some(non_capturing_options()));
    f.tui.show_overlay(controller, None);
    wait(&f.tui, &f.terminal);
    assert!(controller_focus.get());
    assert!(!f.editor_focus.get());

    timer_handle.hide();
    f.tui.hide_overlay();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!controller_focus.get());
    assert!(!timer_focus.get());

    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&f.editor_state), vec!["x".to_string()]);
    assert!(inputs(&controller_state).is_empty());
    f.tui.stop().unwrap();
}

#[test]
fn handle_input_redirection_skips_non_capturing_overlays_when_focused_overlay_becomes_invisible() {
    let f = fixture();
    let (fallback, fallback_state, fallback_focus) = overlay_component(&["FALLBACK"]);
    let (non_capturing_overlay, nc_state, _nc_focus) = overlay_component(&["NC"]);
    let (primary, primary_state, _primary_focus) = overlay_component(&["PRIMARY"]);
    let visible = Rc::new(std::cell::Cell::new(true));
    let visibility = Rc::clone(&visible);
    f.tui.show_overlay(fallback, None);
    f.tui.show_overlay(non_capturing_overlay, Some(non_capturing_options()));
    f.tui.show_overlay(
        primary,
        Some(OverlayOptions {
            visible: Some(Rc::new(move |_, _| visibility.get())),
            ..Default::default()
        }),
    );
    visible.set(false);
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert!(inputs(&primary_state).is_empty());
    assert!(inputs(&nc_state).is_empty());
    assert_eq!(inputs(&fallback_state), vec!["x".to_string()]);
    assert!(fallback_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn active_base_focus_replacement_receives_close_input_before_overlay_restore() {
    let f = fixture();
    let (replacement, replacement_state, _replacement_focus) = overlay_component(&["REPLACEMENT"]);
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    {
        let tui = Rc::clone(&f.tui);
        let replacement = replacement.clone();
        set_input_handler(&overlay_state, move |data| {
            if data == "b" {
                tui.set_focus(Some(replacement.clone()));
            }
        });
    }
    {
        let tui = Rc::clone(&f.tui);
        let editor = f.editor.clone();
        set_input_handler(&replacement_state, move |data| {
            if data == "\r" {
                tui.set_focus(Some(editor.clone()));
            }
        });
    }
    f.tui.show_overlay(overlay, None);
    send(&f.tui, &f.terminal, "b");
    wait(&f.tui, &f.terminal);
    assert!(_replacement_focus.get());

    send(&f.tui, &f.terminal, "\r");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&replacement_state), vec!["\r".to_string()]);
    assert_eq!(inputs(&overlay_state), vec!["b".to_string()]);
    assert!(_overlay_focus.get());

    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&overlay_state), vec!["b".to_string(), "x".to_string()]);
    f.tui.stop().unwrap();
}

#[test]
fn active_replacement_still_receives_input_when_it_is_another_overlay_pre_focus() {
    let f = fixture();
    let (replacement, replacement_state, replacement_focus) = overlay_component(&["REPLACEMENT"]);
    let (passive, _passive_state, _passive_focus) = overlay_component(&["PASSIVE"]);
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    {
        let tui = Rc::clone(&f.tui);
        let replacement = replacement.clone();
        set_input_handler(&overlay_state, move |data| {
            if data == "b" {
                tui.set_focus(Some(replacement.clone()));
            }
        });
    }
    {
        let tui = Rc::clone(&f.tui);
        let editor = f.editor.clone();
        set_input_handler(&replacement_state, move |data| {
            if data == "\r" {
                tui.set_focus(Some(editor.clone()));
            }
        });
    }
    f.tui.set_focus(Some(replacement.clone()));
    f.tui.show_overlay(passive, Some(non_capturing_options()));
    f.tui.set_focus(Some(f.editor.clone()));
    f.tui.show_overlay(overlay, None);
    send(&f.tui, &f.terminal, "b");
    wait(&f.tui, &f.terminal);
    assert!(replacement_focus.get());

    send(&f.tui, &f.terminal, "1");
    send(&f.tui, &f.terminal, "\r");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&replacement_state), vec!["1".to_string(), "\r".to_string()]);
    assert_eq!(inputs(&overlay_state), vec!["b".to_string()]);
    f.tui.stop().unwrap();
}

#[test]
fn blocked_replacement_can_move_focus_internally_before_overlay_restore() {
    let f = fixture();
    let (base, base_component) = container();
    let (editor, _editor_state, _editor_focus) = overlay_component(&["EDITOR"]);
    let (first, first_state, _first_focus) = overlay_component(&["FIRST"]);
    let (second, second_state, _second_focus) = overlay_component(&["SECOND"]);
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    base.borrow_mut().add_child(editor.clone());
    base.borrow_mut().add_child(first.clone());
    base.borrow_mut().add_child(second.clone());
    f.tui.add_child(base_component);
    f.tui.set_focus(Some(editor.clone()));
    {
        let tui = Rc::clone(&f.tui);
        let first = first.clone();
        set_input_handler(&overlay_state, move |data| {
            if data == "b" {
                tui.set_focus(Some(first.clone()));
            }
        });
    }
    {
        let tui = Rc::clone(&f.tui);
        let second = second.clone();
        set_input_handler(&first_state, move |data| {
            if data == "n" {
                tui.set_focus(Some(second.clone()));
            }
        });
    }
    {
        let tui = Rc::clone(&f.tui);
        let base = Rc::clone(&base);
        let editor = editor.clone();
        set_input_handler(&second_state, move |data| {
            if data == "\r" {
                base.borrow_mut().clear();
                base.borrow_mut().add_child(editor.clone());
                tui.set_focus(Some(editor.clone()));
            }
        });
    }
    f.tui.show_overlay(overlay, None);
    send(&f.tui, &f.terminal, "b");
    wait(&f.tui, &f.terminal);
    send(&f.tui, &f.terminal, "n");
    wait(&f.tui, &f.terminal);
    send(&f.tui, &f.terminal, "2");
    send(&f.tui, &f.terminal, "\r");
    wait(&f.tui, &f.terminal);

    assert_eq!(inputs(&overlay_state), vec!["b".to_string()]);
    assert_eq!(inputs(&first_state), vec!["n".to_string()]);
    assert_eq!(inputs(&second_state), vec!["2".to_string(), "\r".to_string()]);
    assert!(_overlay_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn removed_replacement_restores_overlay_even_when_overlay_pre_focus_differs() {
    let f = fixture();
    let (base, base_component) = container();
    let (editor, _editor_state, _editor_focus) = overlay_component(&["EDITOR"]);
    let (palette, _palette_state, _palette_focus) = overlay_component(&["PALETTE"]);
    let (replacement, replacement_state, _replacement_focus) = overlay_component(&["REPLACEMENT"]);
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    base.borrow_mut().add_child(editor.clone());
    base.borrow_mut().add_child(palette.clone());
    base.borrow_mut().add_child(replacement.clone());
    f.tui.add_child(base_component);
    f.tui.set_focus(Some(palette.clone()));
    {
        let tui = Rc::clone(&f.tui);
        let replacement = replacement.clone();
        set_input_handler(&overlay_state, move |data| {
            if data == "b" {
                tui.set_focus(Some(replacement.clone()));
            }
        });
    }
    {
        let tui = Rc::clone(&f.tui);
        let base = Rc::clone(&base);
        let editor = editor.clone();
        set_input_handler(&replacement_state, move |data| {
            if data == "\r" {
                base.borrow_mut().clear();
                base.borrow_mut().add_child(editor.clone());
                tui.set_focus(Some(editor.clone()));
            }
        });
    }
    f.tui.show_overlay(overlay, None);
    send(&f.tui, &f.terminal, "b");
    wait(&f.tui, &f.terminal);
    send(&f.tui, &f.terminal, "\r");
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);

    assert_eq!(inputs(&overlay_state), vec!["b".to_string(), "x".to_string()]);
    assert_eq!(inputs(&replacement_state), vec!["\r".to_string()]);
    assert!(_overlay_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn unfocus_target_releases_a_blocked_overlay_while_replacement_remains_focused() {
    let f = fixture();
    let (fallback, fallback_state, _fallback_focus) = overlay_component(&["FALLBACK"]);
    let (target, target_state, _target_focus) = overlay_component(&["TARGET"]);
    let (replacement, replacement_state, replacement_focus) = overlay_component(&["REPLACEMENT"]);
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    {
        let tui = Rc::clone(&f.tui);
        let fallback = fallback.clone();
        set_input_handler(&replacement_state, move |data| {
            if data == "\r" {
                tui.set_focus(Some(fallback.clone()));
            }
        });
    }
    let overlay_handle = f.tui.show_overlay(overlay, None);
    {
        let tui = Rc::clone(&f.tui);
        let replacement = replacement.clone();
        let handle = overlay_handle.clone();
        let target = target.clone();
        set_input_handler(&overlay_state, move |data| {
            if data == "b" {
                tui.set_focus(Some(replacement.clone()));
                handle.unfocus_to(Some(target.clone()));
            }
        });
    }
    send(&f.tui, &f.terminal, "b");
    wait(&f.tui, &f.terminal);
    assert!(replacement_focus.get());
    send(&f.tui, &f.terminal, "\r");
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);

    assert_eq!(inputs(&overlay_state), vec!["b".to_string()]);
    assert_eq!(inputs(&replacement_state), vec!["\r".to_string()]);
    assert!(inputs(&fallback_state).is_empty());
    assert_eq!(inputs(&target_state), vec!["x".to_string()]);
    f.tui.stop().unwrap();
}

#[test]
fn handle_input_restores_focus_to_a_visible_focused_overlay_after_base_focus_steal() {
    let f = fixture();
    let (replacement, _replacement_state, _replacement_focus) = overlay_component(&["REPLACEMENT"]);
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    f.tui.show_overlay(overlay, None);
    assert!(_overlay_focus.get());
    f.tui.set_focus(Some(replacement));
    f.tui.set_focus(Some(f.editor.clone()));
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&overlay_state), vec!["x".to_string()]);
    assert!(inputs(&f.editor_state).is_empty());
    f.tui.stop().unwrap();
}

#[test]
fn handle_input_restores_explicitly_focused_raw_sub_overlay_after_base_focus_steal() {
    let f = fixture();
    let (controller, controller_state, _controller_focus) = overlay_component(&["CONTROLLER"]);
    let (sub_overlay, sub_state, _sub_focus) = overlay_component(&["SUB"]);
    f.tui.show_overlay(controller, None);
    let sub_handle = f.tui.show_overlay(sub_overlay, Some(non_capturing_options()));
    sub_handle.focus();
    f.tui.set_focus(Some(f.editor.clone()));
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&sub_state), vec!["x".to_string()]);
    assert!(inputs(&controller_state).is_empty());
    assert!(inputs(&f.editor_state).is_empty());
    f.tui.stop().unwrap();
}

#[test]
fn passive_non_capturing_overlay_does_not_regain_input_after_base_focus() {
    let f = fixture();
    let (passive, passive_state, _passive_focus) = overlay_component(&["PASSIVE"]);
    f.tui.show_overlay(passive, Some(non_capturing_options()));
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&f.editor_state), vec!["x".to_string()]);
    assert!(inputs(&passive_state).is_empty());
    assert!(f.editor_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn explicitly_focused_non_capturing_overlay_regains_input_after_base_focus_steal() {
    let f = fixture();
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["NC"]);
    let handle = f.tui.show_overlay(overlay, Some(non_capturing_options()));
    handle.focus();
    f.tui.set_focus(Some(f.editor.clone()));
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&overlay_state), vec!["x".to_string()]);
    assert!(inputs(&f.editor_state).is_empty());
    f.tui.stop().unwrap();
}

#[test]
fn unfocus_prevents_a_visible_overlay_from_regaining_input() {
    let f = fixture();
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    let handle = f.tui.show_overlay(overlay, None);
    handle.unfocus();
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&f.editor_state), vec!["x".to_string()]);
    assert!(inputs(&overlay_state).is_empty());
    assert!(f.editor_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn set_focus_null_explicitly_clears_visible_overlay_restore() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (overlay, overlay_state, overlay_focus) = overlay_component(&["OVERLAY"]);
    tui.add_child(test_component(&[]).0);
    tui.start().unwrap();
    tui.show_overlay(overlay, None);
    tui.set_focus(None);
    send(&tui, &terminal, "x");
    wait(&tui, &terminal);
    assert!(inputs(&overlay_state).is_empty());
    assert!(!overlay_focus.get());
    tui.stop().unwrap();
}

#[test]
fn blocked_replacement_set_focus_null_resumes_the_visible_overlay() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (replacement, replacement_state, _replacement_focus) = overlay_component(&["REPLACEMENT"]);
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    {
        let tui = Rc::clone(&tui);
        set_input_handler(&replacement_state, move |data| {
            if data == "\r" {
                tui.set_focus(None);
            }
        });
    }
    {
        let tui = Rc::clone(&tui);
        let replacement = replacement.clone();
        set_input_handler(&overlay_state, move |data| {
            if data == "b" {
                tui.set_focus(Some(replacement.clone()));
            }
        });
    }
    tui.add_child(test_component(&[]).0);
    tui.start().unwrap();
    tui.show_overlay(overlay, None);
    send(&tui, &terminal, "b");
    wait(&tui, &terminal);
    send(&tui, &terminal, "\r");
    send(&tui, &terminal, "x");
    wait(&tui, &terminal);
    assert_eq!(inputs(&replacement_state), vec!["\r".to_string()]);
    assert_eq!(inputs(&overlay_state), vec!["b".to_string(), "x".to_string()]);
    tui.stop().unwrap();
}

#[test]
fn temporarily_invisible_focused_overlay_falls_back_without_losing_restore_eligibility() {
    let f = fixture();
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    let visible = Rc::new(std::cell::Cell::new(true));
    let visibility = Rc::clone(&visible);
    f.tui.show_overlay(
        overlay,
        Some(OverlayOptions {
            visible: Some(Rc::new(move |_, _| visibility.get())),
            ..Default::default()
        }),
    );
    f.tui.set_focus(Some(f.editor.clone()));
    visible.set(false);
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&f.editor_state), vec!["x".to_string()]);
    assert!(inputs(&overlay_state).is_empty());
    visible.set(true);
    send(&f.tui, &f.terminal, "y");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&f.editor_state), vec!["x".to_string()]);
    assert_eq!(inputs(&overlay_state), vec!["y".to_string()]);
    f.tui.stop().unwrap();
}

#[test]
fn temporarily_invisible_focused_overlay_with_null_pre_focus_restores_when_visible_again() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    let visible = Rc::new(std::cell::Cell::new(true));
    let visibility = Rc::clone(&visible);
    tui.add_child(test_component(&[]).0);
    tui.start().unwrap();
    tui.show_overlay(
        overlay,
        Some(OverlayOptions {
            visible: Some(Rc::new(move |_, _| visibility.get())),
            ..Default::default()
        }),
    );
    visible.set(false);
    send(&tui, &terminal, "x");
    wait(&tui, &terminal);
    assert!(inputs(&overlay_state).is_empty());
    visible.set(true);
    send(&tui, &terminal, "y");
    wait(&tui, &terminal);
    assert_eq!(inputs(&overlay_state), vec!["y".to_string()]);
    tui.stop().unwrap();
}

#[test]
fn cyclic_overlay_pre_focus_ancestry_does_not_hang_focus_changes() {
    let f = fixture();
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    f.tui.set_focus(Some(overlay.clone()));
    let handle = f.tui.show_overlay(overlay, Some(non_capturing_options()));
    handle.focus();
    f.tui.set_focus(Some(f.editor.clone()));
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&f.editor_state), vec!["x".to_string()]);
    assert!(inputs(&overlay_state).is_empty());
    f.tui.stop().unwrap();
}

#[test]
fn handle_input_restores_the_focus_order_top_overlay_after_base_focus_steal() {
    let f = fixture();
    let (lower, lower_state, _lower_focus) = overlay_component(&["LOWER"]);
    let (upper, upper_state, _upper_focus) = overlay_component(&["UPPER"]);
    let lower_handle = f.tui.show_overlay(lower, None);
    f.tui.show_overlay(upper, None);
    lower_handle.focus();
    f.tui.set_focus(Some(f.editor.clone()));
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&lower_state), vec!["x".to_string()]);
    assert!(inputs(&upper_state).is_empty());
    assert!(inputs(&f.editor_state).is_empty());
    f.tui.stop().unwrap();
}

#[test]
fn hide_overlay_does_not_reassign_focus_when_topmost_overlay_is_non_capturing() {
    let f = fixture();
    let (capturing, _capturing_state, capturing_focus) = overlay_component(&["CAP"]);
    let (non_capturing_overlay, _nc_state, _nc_focus) = overlay_component(&["NC"]);
    f.tui.show_overlay(capturing, None);
    f.tui.show_overlay(non_capturing_overlay, Some(non_capturing_options()));
    assert!(capturing_focus.get());
    f.tui.hide_overlay();
    wait(&f.tui, &f.terminal);
    assert!(capturing_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn multiple_capturing_and_non_capturing_overlays_restore_focus_through_removals() {
    let f = fixture();
    let (c1, _c1_state, c1_focus) = overlay_component(&["C1"]);
    let (n1, _n1_state, _n1_focus) = overlay_component(&["N1"]);
    let (c2, _c2_state, c2_focus) = overlay_component(&["C2"]);
    let (n2, _n2_state, _n2_focus) = overlay_component(&["N2"]);
    let c1_handle = f.tui.show_overlay(c1, None);
    f.tui.show_overlay(n1, Some(non_capturing_options()));
    let c2_handle = f.tui.show_overlay(c2, None);
    f.tui.show_overlay(n2, Some(non_capturing_options()));
    assert!(c2_focus.get());
    c2_handle.hide();
    wait(&f.tui, &f.terminal);
    assert!(c1_focus.get());
    c1_handle.hide();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn capturing_overlay_unfocus_on_topmost_capturing_overlay_falls_back_to_pre_focus() {
    let f = fixture();
    let (capturing, _capturing_state, capturing_focus) = overlay_component(&["CAP"]);
    let handle = f.tui.show_overlay(capturing, None);
    assert!(capturing_focus.get());
    handle.unfocus();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!capturing_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn focus_on_hidden_overlay_is_a_no_op() {
    let f = fixture();
    let (overlay, _overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    let handle = f.tui.show_overlay(overlay, Some(non_capturing_options()));
    handle.set_hidden(true);
    handle.focus();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!handle.is_focused());
    f.tui.stop().unwrap();
}

#[test]
fn focus_after_hide_is_a_no_op() {
    let f = fixture();
    let (overlay, _overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    let handle = f.tui.show_overlay(overlay, Some(non_capturing_options()));
    handle.hide();
    handle.focus();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!handle.is_focused());
    f.tui.stop().unwrap();
}

#[test]
fn unfocus_when_overlay_does_not_have_focus_is_a_no_op() {
    let f = fixture();
    let (overlay, _overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    let handle = f.tui.show_overlay(overlay, Some(non_capturing_options()));
    handle.unfocus();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!handle.is_focused());
    f.tui.stop().unwrap();
}

#[test]
fn unfocus_with_null_pre_focus_clears_focus_and_does_not_route_input_back_to_overlay() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    tui.add_child(test_component(&[]).0);
    tui.start().unwrap();
    let overlay_handle = tui.show_overlay(overlay, None);
    assert!(overlay_handle.is_focused());
    overlay_handle.unfocus();
    assert!(!overlay_handle.is_focused());
    send(&tui, &terminal, "x");
    wait(&tui, &terminal);
    assert!(inputs(&overlay_state).is_empty());
    assert!(!overlay_handle.is_focused());
    tui.stop().unwrap();
}

#[test]
fn toggle_focus_between_non_capturing_overlays_then_unfocus_returns_to_editor() {
    let f = fixture();
    let (a, _a_state, a_focus) = overlay_component(&["A"]);
    let (b, _b_state, b_focus) = overlay_component(&["B"]);
    let a_handle = f.tui.show_overlay(a, Some(non_capturing_options()));
    let b_handle = f.tui.show_overlay(b, Some(non_capturing_options()));
    a_handle.focus();
    b_handle.focus();
    a_handle.focus();
    a_handle.unfocus();
    wait(&f.tui, &f.terminal);
    assert!(f.editor_focus.get());
    assert!(!a_focus.get());
    assert!(!b_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn explicit_unfocus_target_supports_cycling_between_three_overlays_and_editor() {
    let f = fixture();
    let (a, a_state, _a_focus) = overlay_component(&["A"]);
    let (b, b_state, _b_focus) = overlay_component(&["B"]);
    let (c, c_state, _c_focus) = overlay_component(&["C"]);
    let a_handle = f.tui.show_overlay(a, None);
    let b_handle = f.tui.show_overlay(b, None);
    let c_handle = f.tui.show_overlay(c, None);

    a_handle.focus();
    send(&f.tui, &f.terminal, "a");
    wait(&f.tui, &f.terminal);
    b_handle.focus();
    send(&f.tui, &f.terminal, "b");
    wait(&f.tui, &f.terminal);
    c_handle.focus();
    send(&f.tui, &f.terminal, "c");
    wait(&f.tui, &f.terminal);
    c_handle.unfocus_to(Some(f.editor.clone()));
    send(&f.tui, &f.terminal, "e");
    wait(&f.tui, &f.terminal);
    a_handle.focus();
    send(&f.tui, &f.terminal, "A");
    wait(&f.tui, &f.terminal);
    a_handle.unfocus_to(Some(f.editor.clone()));
    send(&f.tui, &f.terminal, "E");
    wait(&f.tui, &f.terminal);

    assert_eq!(inputs(&a_state), vec!["a".to_string(), "A".to_string()]);
    assert_eq!(inputs(&b_state), vec!["b".to_string()]);
    assert_eq!(inputs(&c_state), vec!["c".to_string()]);
    assert_eq!(inputs(&f.editor_state), vec!["e".to_string(), "E".to_string()]);
    assert!(f.editor_focus.get());
    f.tui.stop().unwrap();
}

#[test]
fn explicit_null_unfocus_target_clears_focus_without_restoring_overlays() {
    let (terminal, handle) = concrete(80, 24);
    let tui = TUI::new(handle);
    let (overlay, overlay_state, _overlay_focus) = overlay_component(&["OVERLAY"]);
    tui.add_child(test_component(&[]).0);
    tui.start().unwrap();
    let overlay_handle = tui.show_overlay(overlay, None);
    overlay_handle.unfocus_to(None);
    send(&tui, &terminal, "x");
    wait(&tui, &terminal);
    assert!(inputs(&overlay_state).is_empty());
    assert!(!overlay_handle.is_focused());
    tui.stop().unwrap();
}

#[test]
fn hiding_focused_overlay_falls_back_to_next_visual_frontmost_overlay() {
    let f = fixture();
    let (a, a_state, _a_focus) = overlay_component(&["A"]);
    let (b, b_state, _b_focus) = overlay_component(&["B"]);
    let (c, c_state, _c_focus) = overlay_component(&["C"]);
    let a_handle = f.tui.show_overlay(a, None);
    let b_handle = f.tui.show_overlay(b, None);
    f.tui.show_overlay(c, None);
    a_handle.focus();
    b_handle.focus();
    b_handle.set_hidden(true);
    send(&f.tui, &f.terminal, "x");
    wait(&f.tui, &f.terminal);
    assert_eq!(inputs(&a_state), vec!["x".to_string()]);
    assert!(inputs(&b_state).is_empty());
    assert!(inputs(&c_state).is_empty());
    f.tui.stop().unwrap();
}

#[test]
fn focus_on_already_focused_overlay_bumps_visual_order() {
    let (terminal, handle) = concrete(20, 6);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    tui.start().unwrap();
    let (a, _a_state) = test_component(&["A"]);
    let (b, _b_state) = test_component(&["B"]);
    let (c, _c_state) = test_component(&["C"]);
    let a_handle = tui.show_overlay(a, Some(positioned(0.0, 0.0, 1.0)));
    tui.show_overlay(b, Some(positioned(0.0, 0.0, 1.0)));
    a_handle.focus();
    tui.show_overlay(c, Some(positioned(0.0, 0.0, 1.0)));
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('C'));
    a_handle.focus();
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('A'));
    assert!(a_handle.is_focused());
    tui.stop().unwrap();
}

#[test]
fn default_rendering_order_for_overlapping_overlays_follows_creation_order() {
    let (terminal, handle) = concrete(20, 6);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    tui.start().unwrap();
    let (a, _a_state) = test_component(&["A"]);
    let (b, _b_state) = test_component(&["B"]);
    tui.show_overlay(a, Some(positioned(0.0, 0.0, 1.0)));
    tui.show_overlay(b, Some(positioned(0.0, 0.0, 1.0)));
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('B'));
    tui.stop().unwrap();
}

#[test]
fn focus_on_lower_overlay_renders_it_on_top() {
    let (terminal, handle) = concrete(20, 6);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    tui.start().unwrap();
    let (a, _a_state) = test_component(&["A"]);
    let (b, _b_state) = test_component(&["B"]);
    let lower = tui.show_overlay(a, Some(positioned(0.0, 0.0, 1.0)));
    tui.show_overlay(b, Some(positioned(0.0, 0.0, 1.0)));
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('B'));
    lower.focus();
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('A'));
    tui.stop().unwrap();
}

#[test]
fn focusing_middle_overlay_places_it_on_top_while_preserving_others_relative_order() {
    let (terminal, handle) = concrete(20, 6);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    tui.start().unwrap();
    let (a, _a_state) = test_component(&["A"]);
    let (b, _b_state) = test_component(&["B"]);
    let (c, _c_state) = test_component(&["C"]);
    tui.show_overlay(a, Some(positioned(0.0, 0.0, 1.0)));
    let middle = tui.show_overlay(b, Some(positioned(0.0, 0.0, 1.0)));
    let top = tui.show_overlay(c, Some(positioned(0.0, 0.0, 1.0)));
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('C'));
    middle.focus();
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('B'));
    middle.hide();
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('C'));
    top.hide();
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('A'));
    tui.stop().unwrap();
}

#[test]
fn capturing_overlay_hidden_and_shown_again_renders_on_top_after_unhide() {
    let (terminal, handle) = concrete(20, 6);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    tui.start().unwrap();
    let (a, _a_state) = test_component(&["A"]);
    let (b, _b_state) = test_component(&["B"]);
    let (c, _c_state) = test_component(&["C"]);
    tui.show_overlay(a, Some(positioned(0.0, 0.0, 1.0)));
    let capturing = tui.show_overlay(
        b,
        Some(OverlayOptions {
            row: Some(SizeValue::Absolute(0.0)),
            col: Some(SizeValue::Absolute(0.0)),
            width: Some(SizeValue::Absolute(1.0)),
            ..Default::default()
        }),
    );
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('B'));
    capturing.set_hidden(true);
    tui.show_overlay(c, Some(positioned(0.0, 0.0, 1.0)));
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('C'));
    capturing.set_hidden(false);
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('B'));
    tui.stop().unwrap();
}

#[test]
fn unfocus_does_not_change_visual_order_until_another_overlay_is_focused() {
    let (terminal, handle) = concrete(20, 6);
    let tui = TUI::new(handle);
    tui.add_child(test_component(&[]).0);
    tui.start().unwrap();
    let (a, _a_state) = test_component(&["A"]);
    let (b, _b_state) = test_component(&["B"]);
    let a_handle = tui.show_overlay(a, Some(positioned(0.0, 0.0, 1.0)));
    let b_handle = tui.show_overlay(b, Some(positioned(0.0, 0.0, 1.0)));
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('B'));
    a_handle.focus();
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('A'));
    a_handle.unfocus();
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('A'));
    b_handle.focus();
    tui.request_render(true);
    wait(&tui, &terminal);
    assert_eq!(terminal.borrow().viewport()[0].chars().next(), Some('B'));
    tui.stop().unwrap();
}
