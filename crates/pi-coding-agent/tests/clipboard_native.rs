use std::cell::Cell;
use std::rc::Rc;

use pi_coding_agent::utils::clipboard_native::{
    ClipboardError, ClipboardModule, display_available, load_clipboard_native,
};

struct FakeClipboard {
    label: &'static str,
}

impl ClipboardModule for FakeClipboard {
    fn get_text(&self) -> Result<String, ClipboardError> {
        Ok(format!("text from {}", self.label))
    }

    fn set_text(&self, _text: &str) -> Result<(), ClipboardError> {
        Ok(())
    }

    fn has_image(&self) -> Result<bool, ClipboardError> {
        Ok(true)
    }

    fn get_image_binary(&self) -> Result<Vec<u8>, ClipboardError> {
        Ok(vec![1, 2, 3])
    }
}

#[test]
fn falls_back_to_the_next_loader_root() {
    let first_called = Rc::new(Cell::new(false));
    let second_called = Rc::new(Cell::new(false));

    let first_flag = first_called.clone();
    let first = move || -> Option<Box<dyn ClipboardModule>> {
        first_flag.set(true);
        None
    };
    let second_flag = second_called.clone();
    let second = move || -> Option<Box<dyn ClipboardModule>> {
        second_flag.set(true);
        Some(Box::new(FakeClipboard { label: "fallback" }))
    };

    let clipboard = load_clipboard_native(&[&first, &second]).expect("fallback clipboard");
    assert_eq!(clipboard.get_text().unwrap(), "text from fallback");
    assert!(clipboard.has_image().unwrap());
    assert_eq!(clipboard.get_image_binary().unwrap(), vec![1, 2, 3]);
    assert!(first_called.get());
    assert!(second_called.get());
}

#[test]
fn returns_none_when_no_loader_succeeds() {
    let missing = || -> Option<Box<dyn ClipboardModule>> { None };
    assert!(load_clipboard_native(&[&missing]).is_none());
}

#[test]
fn stops_after_the_first_successful_loader() {
    let second_called = Rc::new(Cell::new(false));
    let first = || -> Option<Box<dyn ClipboardModule>> { Some(Box::new(FakeClipboard { label: "primary" })) };
    let second_flag = second_called.clone();
    let second = move || -> Option<Box<dyn ClipboardModule>> {
        second_flag.set(true);
        Some(Box::new(FakeClipboard { label: "secondary" }))
    };

    let clipboard = load_clipboard_native(&[&first, &second]).expect("primary clipboard");
    assert_eq!(clipboard.get_text().unwrap(), "text from primary");
    assert!(!second_called.get());
}

#[test]
fn requires_a_display_on_linux() {
    assert!(display_available(false, None, None));
    assert!(!display_available(true, None, None));
    assert!(display_available(true, Some(":0"), None));
    assert!(display_available(true, None, Some("wayland-0")));
    assert!(!display_available(true, Some(""), Some("")));
}
