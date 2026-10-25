//! Frozen `coding-agent/src/modes/interactive/editor-dialog-restore.ts`.
use std::cell::RefCell;
use std::rc::Rc;

use pi_tui::Container;

use super::interactive_mode::InteractiveTuiHandle;

/// Capture child identities and focus, not a reconstructed default editor.
pub fn capture_editor_dialog_restore(container: Rc<RefCell<Container>>, ui: Rc<InteractiveTuiHandle>) -> Rc<dyn Fn()> {
    let children = container.borrow().children().to_vec();
    let focus = ui.get_focused_component();
    Rc::new(move || {
        {
            let mut container = container.borrow_mut();
            container.clear();
            for child in &children {
                container.add_child(Rc::clone(child));
            }
        }
        ui.set_focus(focus.clone());
        ui.request_render();
    })
}
