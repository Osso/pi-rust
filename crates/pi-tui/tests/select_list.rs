use std::cell::RefCell;
use std::rc::Rc;

use pi_tui::components::select_list::{
    SelectItem, SelectList, SelectListLayoutOptions, SelectListTheme, SelectListTruncatePrimaryContext,
};
use pi_tui::tui::Component;
use pi_tui::utils::visible_width;

fn list(items: Vec<SelectItem>, max_visible: usize) -> SelectList {
    SelectList::new(
        items,
        max_visible,
        SelectListTheme::plain(),
        SelectListLayoutOptions::default(),
    )
}

fn item(value: &str) -> SelectItem {
    SelectItem::new(value, value)
}

fn visible_index(line: &str, text: &str) -> usize {
    let index = line.find(text).expect("line must contain text");
    visible_width(&line[..index])
}

#[test]
fn normalizes_multiline_descriptions_to_single_line() {
    let items = vec![SelectItem::new("test", "test").with_description("Line one\nLine two\nLine three")];
    let mut list = list(items, 5);
    let rendered = list.render(100);

    assert!(!rendered.is_empty());
    assert!(!rendered[0].contains('\n'));
    assert!(rendered[0].contains("Line one Line two Line three"));
}

#[test]
fn keeps_descriptions_aligned_when_primary_text_is_truncated() {
    let items = vec![
        SelectItem::new("short", "short").with_description("short description"),
        SelectItem::new(
            "very-long-command-name-that-needs-truncation",
            "very-long-command-name-that-needs-truncation",
        )
        .with_description("long description"),
    ];
    let mut list = list(items, 5);
    let rendered = list.render(80);

    assert_eq!(
        visible_index(&rendered[0], "short description"),
        visible_index(&rendered[1], "long description")
    );
}

#[test]
fn uses_configured_minimum_primary_column_width() {
    let items = vec![
        item("a").with_description("first"),
        item("bb").with_description("second"),
    ];
    let layout = SelectListLayoutOptions {
        min_primary_column_width: Some(12),
        max_primary_column_width: Some(20),
        ..SelectListLayoutOptions::default()
    };
    let mut list = SelectList::new(items, 5, SelectListTheme::plain(), layout);
    let rendered = list.render(80);

    assert_eq!(visible_index(&rendered[0], "first"), 14);
    assert_eq!(visible_index(&rendered[1], "second"), 14);
}

#[test]
fn uses_configured_maximum_primary_column_width() {
    let items = vec![
        item("very-long-command-name-that-needs-truncation").with_description("first"),
        item("short").with_description("second"),
    ];
    let layout = SelectListLayoutOptions {
        min_primary_column_width: Some(12),
        max_primary_column_width: Some(20),
        ..SelectListLayoutOptions::default()
    };
    let mut list = SelectList::new(items, 5, SelectListTheme::plain(), layout);
    let rendered = list.render(80);

    assert_eq!(visible_index(&rendered[0], "first"), 22);
    assert_eq!(visible_index(&rendered[1], "second"), 22);
}

#[test]
fn allows_overriding_primary_truncation_while_preserving_alignment() {
    let items = vec![
        item("very-long-command-name-that-needs-truncation").with_description("first"),
        item("short").with_description("second"),
    ];
    let layout = SelectListLayoutOptions {
        min_primary_column_width: Some(12),
        max_primary_column_width: Some(12),
        truncate_primary: Some(Rc::new(|context: &SelectListTruncatePrimaryContext<'_>| {
            if context.text.len() <= context.max_width {
                return context.text.to_string();
            }
            let keep = context.max_width.saturating_sub(1);
            format!("{}…", &context.text[..keep.min(context.text.len())])
        })),
    };
    let mut list = SelectList::new(items, 5, SelectListTheme::plain(), layout);
    let rendered = list.render(80);

    assert!(rendered[0].contains('…'));
    assert_eq!(
        visible_index(&rendered[0], "first"),
        visible_index(&rendered[1], "second")
    );
}

#[test]
fn up_and_down_wrap_and_notify_selection_change() {
    let mut list = list(vec![item("one"), item("two"), item("three")], 5);
    let changes = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&changes);
    list.on_selection_change = Some(Rc::new(move |item: &SelectItem| {
        sink.borrow_mut().push(item.value.clone());
    }));

    list.handle_input("\x1b[A");
    assert_eq!(list.selected_index(), 2);
    list.handle_input("\x1b[B");
    assert_eq!(list.selected_index(), 0);
    list.handle_input("\x1b[B");
    assert_eq!(list.selected_index(), 1);

    assert_eq!(*changes.borrow(), vec!["three", "one", "two"]);
}

#[test]
fn confirm_and_cancel_invoke_callbacks() {
    let mut list = list(vec![item("one"), item("two")], 5);
    let events = Rc::new(RefCell::new(Vec::new()));
    let select_events = Rc::clone(&events);
    list.on_select = Some(Rc::new(move |item: &SelectItem| {
        select_events.borrow_mut().push(format!("select:{}", item.value));
    }));
    let cancel_events = Rc::clone(&events);
    list.on_cancel = Some(Rc::new(move || {
        cancel_events.borrow_mut().push("cancel".to_string());
    }));

    list.handle_input("\r");
    list.handle_input("\x1b");
    list.handle_input("\x03");

    assert_eq!(*events.borrow(), vec!["select:one", "cancel", "cancel"]);
}

#[test]
fn filtering_resets_selection_and_reports_no_match() {
    let mut list = list(vec![item("alpha"), item("beta"), item("alphabet")], 5);
    list.set_filter("ALPHA");
    assert_eq!(list.get_selected_item().unwrap().value, "alpha");

    list.set_selected_index(1);
    list.handle_input("\x1b[A");
    assert_eq!(list.selected_index(), 0);

    list.set_filter("zzz");
    assert!(list.get_selected_item().is_none());
    assert_eq!(list.render(80), vec!["  No matching commands"]);
}

#[test]
fn scrolls_window_and_renders_scroll_indicator() {
    let items: Vec<SelectItem> = (0..10).map(|index| item(&format!("item{index}"))).collect();
    let mut list = list(items, 3);
    list.set_selected_index(9);
    let rendered = list.render(40);

    assert_eq!(rendered.len(), 4);
    assert_eq!(rendered[0], "  item7");
    assert_eq!(rendered[1], "  item8");
    assert_eq!(rendered[2], "→ item9");
    assert_eq!(rendered[3], "  (10/10)");
}

#[test]
fn uses_label_fallback_value_and_selected_theme() {
    let items = vec![SelectItem::new("value-only", "")];
    let mut list = SelectList::new(
        items,
        5,
        SelectListTheme {
            selected_prefix: Rc::new(|text: &str| format!("«{text}»")),
            selected_text: Rc::new(|text: &str| format!("{{{text}}}")),
            ..SelectListTheme::plain()
        },
        SelectListLayoutOptions::default(),
    );
    let rendered = list.render(20);

    assert_eq!(rendered[0], "{→ value-only}");
}
