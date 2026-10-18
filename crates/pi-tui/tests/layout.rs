use std::cell::{Cell, RefMut};
use std::rc::Rc;
use std::time::{Duration, Instant};

use pi_tui::components::h_stack::HStack;
use pi_tui::components::scroll_view::{ScrollFollow, ScrollView, ScrollViewOptions, ScrollViewScrollbar};
use pi_tui::components::stack::StackOptions;
use pi_tui::components::text::{StyleFn, Text};
use pi_tui::components::v_stack::VStack;
use pi_tui::layout::{LayoutFrame, render_layout_frame};
use pi_tui::layout_node::{StackAlign, StackBasis, StackLayoutEntry};
use pi_tui::terminal_image::{KittyImageMetadata, encode_kitty, register_kitty_image_metadata};
use pi_tui::tui::{Component, ComponentRef, component};
use pi_tui::utils::strip_terminal_sequences;

fn text(value: &str) -> ComponentRef {
    component(Text::new(value, 0, 0))
}

fn entry(component_ref: ComponentRef) -> StackLayoutEntry {
    StackLayoutEntry::new(component_ref)
}

fn frame(root: &ComponentRef, width: usize, height: usize) -> LayoutFrame {
    render_layout_frame(root, width, height, Rc::new(|| {}))
}

fn visible_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| strip_terminal_sequences(line).trim_end().to_string())
        .collect()
}

fn scroll_mut(component_ref: &ComponentRef) -> RefMut<'_, ScrollView> {
    RefMut::map(component_ref.borrow_mut(), |value| {
        value
            .as_any_mut()
            .and_then(|value| value.downcast_mut::<ScrollView>())
            .expect("scroll view component")
    })
}

fn text_mut(component_ref: &ComponentRef) -> RefMut<'_, Text> {
    RefMut::map(component_ref.borrow_mut(), |value| {
        value
            .as_any_mut()
            .and_then(|value| value.downcast_mut::<Text>())
            .expect("text component")
    })
}

#[test]
fn allocates_vertical_grow_space_deterministically() {
    let mut top = entry(text("top"));
    top.basis = Some(StackBasis::Cells(1));
    top.shrink = Some(0);
    let mut body = entry(text("body"));
    body.basis = Some(StackBasis::Cells(0));
    body.grow = Some(1);

    let root = component(VStack::new(vec![top, body], StackOptions::default()));
    let frame = frame(&root, 10, 4);

    assert_eq!(
        frame
            .root
            .children
            .iter()
            .map(|child| child.rect.height)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    assert_eq!(visible_lines(&frame.lines), vec!["top", "body", "", ""]);
}

#[test]
fn does_not_render_fixed_basis_scroll_content_during_stack_measurement() {
    let renders = Rc::new(Cell::new(0usize));
    let counter = Rc::clone(&renders);
    let transcript_lines = Rc::new(vec!["one".to_string(), "two".to_string(), "three".to_string()]);
    let lines = Rc::clone(&transcript_lines);
    let transcript_component = component(CountingLines {
        renders: counter,
        lines,
    });
    let transcript = component(ScrollView::new(transcript_component, ScrollViewOptions::default()));

    let mut transcript_entry = entry(transcript);
    transcript_entry.basis = Some(StackBasis::Cells(0));
    transcript_entry.grow = Some(1);
    let mut dock_entry = entry(text("dock"));
    dock_entry.basis = Some(StackBasis::Auto);

    let root = component(VStack::new(vec![transcript_entry, dock_entry], StackOptions::default()));
    frame(&root, 10, 3);

    assert_eq!(renders.get(), 1);
}

struct StaticLines {
    lines: Vec<String>,
}

impl Component for StaticLines {
    fn render(&mut self, _width: usize) -> Vec<String> {
        self.lines.clone()
    }
}

struct CountingLines {
    renders: Rc<Cell<usize>>,
    lines: Rc<Vec<String>>,
}

impl Component for CountingLines {
    fn render(&mut self, _width: usize) -> Vec<String> {
        self.renders.set(self.renders.get() + 1);
        (*self.lines).clone()
    }
}

#[test]
fn paints_only_clipped_rows_from_large_scroll_content() {
    // The source test uses a sparse 1e9-line JS array; Rust `Vec<String>` cannot
    // represent that without allocating, so this port uses 10k lines and keeps
    // the same clipping assertion. Recorded as a gap in the traceability doc.
    let line_count = 10_000;
    let mut lines = vec![String::new(); line_count];
    lines[line_count - 4] = "before".to_string();
    lines[line_count - 3] = "visible 1".to_string();
    lines[line_count - 2] = "visible 2".to_string();
    lines[line_count - 1] = "visible 3".to_string();
    let transcript = component(ScrollView::new(
        component(StaticLines { lines }),
        ScrollViewOptions {
            follow: Some(ScrollFollow::End),
            ..ScrollViewOptions::default()
        },
    ));

    let frame = frame(&transcript, 10, 3);
    assert_eq!(visible_lines(&frame.lines), vec!["visible 1", "visible 2", "visible 3"]);
}

#[test]
fn shrinks_entries_to_their_minimum_sizes() {
    let mut first = entry(text("a1\na2\na3"));
    first.shrink = Some(1);
    first.min_size = Some(1);
    let mut second = entry(text("b1\nb2\nb3"));
    second.shrink = Some(0);

    let root = component(VStack::new(vec![first, second], StackOptions::default()));
    let frame = frame(&root, 10, 4);

    assert_eq!(
        frame
            .root
            .children
            .iter()
            .map(|child| child.rect.height)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    assert_eq!(visible_lines(&frame.lines), vec!["a1", "b1", "b2", "b3"]);
}

#[test]
fn includes_nested_minimum_sizes_in_intrinsic_stack_measurement() {
    let mut selector = entry(text("selector"));
    selector.min_size = Some(3);
    let mut footer = entry(text("footer"));
    footer.min_size = Some(1);
    let dock = component(VStack::new(
        vec![entry(text("top1\ntop2\ntop3")), selector, entry(text("below")), footer],
        StackOptions::default(),
    ));

    let mut body = entry(text("body"));
    body.basis = Some(StackBasis::Cells(0));
    body.grow = Some(1);
    body.min_size = Some(1);
    let mut dock_entry = entry(dock);
    dock_entry.basis = Some(StackBasis::Auto);
    dock_entry.min_size = Some(1);

    let root = component(VStack::new(vec![body, dock_entry], StackOptions::default()));
    let frame = frame(&root, 10, 9);

    assert_eq!(
        visible_lines(&frame.lines),
        vec!["body", "top1", "top2", "top3", "selector", "", "", "below", "footer"]
    );
}

#[test]
fn omits_gaps_around_invisible_entries() {
    let mut hidden = entry(text("hidden"));
    hidden.visible = Some(Rc::new(|_viewport| false));
    let mut stack = VStack::new(
        vec![entry(text("one")), hidden, entry(text("two"))],
        StackOptions {
            gap: Some(1),
            align: None,
        },
    );
    let lines: Vec<String> = stack
        .render(10)
        .iter()
        .map(|line| strip_terminal_sequences(line).trim_end().to_string())
        .collect();
    assert_eq!(lines, vec!["one", "", "two"]);
}

#[test]
fn crops_kitty_images_at_a_scroll_views_lower_boundary() {
    let image_id = 124;
    let image_line = encode_kitty(
        "AAAA",
        &pi_tui::terminal_image::KittyEncodeOptions {
            columns: Some(2),
            rows: Some(3),
            image_id: Some(image_id),
            move_cursor: Some(false),
        },
    );
    register_kitty_image_metadata(KittyImageMetadata {
        image_id,
        columns: 2,
        rows: 3,
        width_px: 100,
        height_px: 100,
    });
    let transcript = component(ScrollView::new(
        component(StaticLines {
            lines: vec![
                "one".to_string(),
                "two".to_string(),
                image_line,
                String::new(),
                String::new(),
            ],
        }),
        ScrollViewOptions::default(),
    ));

    let mut transcript_entry = entry(transcript);
    transcript_entry.basis = Some(StackBasis::Cells(0));
    transcript_entry.grow = Some(1);
    let root = component(VStack::new(
        vec![transcript_entry, entry(text("dock"))],
        StackOptions::default(),
    ));
    let frame = frame(&root, 20, 4);

    assert!(frame.lines[2].contains("y=0,h=34,r=1"));
}

#[test]
fn composes_horizontal_children_at_allocated_widths() {
    let mut left = entry(text("left"));
    left.basis = Some(StackBasis::Cells(6));
    left.shrink = Some(0);
    let mut right = entry(text("right"));
    right.basis = Some(StackBasis::Cells(6));
    right.shrink = Some(0);

    let root = component(HStack::new(vec![left, right], StackOptions::default()));
    let frame = frame(&root, 12, 1);
    assert_eq!(visible_lines(&frame.lines), vec!["left  right"]);
}

#[test]
fn does_not_paint_zero_width_horizontal_children() {
    let mut hidden = entry(text("hidden"));
    hidden.basis = Some(StackBasis::Cells(0));
    hidden.shrink = Some(0);
    let mut shown = entry(text("shown"));
    shown.basis = Some(StackBasis::Cells(0));
    shown.grow = Some(1);

    let root = component(HStack::new(vec![hidden, shown], StackOptions::default()));
    let frame = frame(&root, 5, 1);
    assert_eq!(visible_lines(&frame.lines), vec!["shown"]);
}

#[test]
fn tracks_follow_end_state_and_returns_unused_scroll_delta() {
    let scroll = component(ScrollView::new(
        text("1\n2\n3\n4\n5\n6"),
        ScrollViewOptions {
            follow: Some(ScrollFollow::End),
            primary: true,
            ..ScrollViewOptions::default()
        },
    ));
    frame(&scroll, 10, 3);
    assert_eq!(scroll_mut(&scroll).scroll_top(), 3);
    assert!(scroll_mut(&scroll).is_following_end());

    assert_eq!(scroll_mut(&scroll).scroll_by(-2), 0);
    assert_eq!(scroll_mut(&scroll).scroll_top(), 1);
    assert!(!scroll_mut(&scroll).is_following_end());
    assert_eq!(scroll_mut(&scroll).scroll_by(-3), -2);
    assert_eq!(scroll_mut(&scroll).scroll_top(), 0);
    assert_eq!(scroll_mut(&scroll).scroll_by(10), 7);
    assert_eq!(scroll_mut(&scroll).scroll_top(), 3);
    assert!(scroll_mut(&scroll).is_following_end());
}

const CONTENT_BACKGROUND: &str = "\x1b[42m";
const SCROLLBAR_BACKGROUND: &str = "\x1b[48;5;1m";

fn red_scrollbar() -> StyleFn {
    Rc::new(|text: &str| format!("{SCROLLBAR_BACKGROUND}{text}\x1b[49m"))
}

fn thumb_rows(lines: &[String]) -> Vec<bool> {
    lines.iter().map(|line| line.contains(SCROLLBAR_BACKGROUND)).collect()
}

#[test]
fn renders_a_transient_proportional_scrollbar_without_replacing_cell_content() {
    let source_lines = [
        "abcd界", "abcde2", "abcde3", "abcde4", "abcde5", "abcde6", "abcde7", "abcde8",
    ];
    let content = component(Text::with_background(
        source_lines.join("\n"),
        0,
        0,
        Rc::new(|text: &str| format!("{CONTENT_BACKGROUND}{text}\x1b[49m")),
    ));
    let scroll = component(ScrollView::new(
        content,
        ScrollViewOptions {
            scrollbar: Some(ScrollViewScrollbar::Auto),
            scrollbar_style: Some(red_scrollbar()),
            scrollbar_hide_delay_ms: Some(10),
            ..ScrollViewOptions::default()
        },
    ));
    let render = |scroll: &ComponentRef| frame(scroll, 6, 4);

    let mut lines = render(&scroll).lines;
    assert_eq!(thumb_rows(&lines), vec![false, false, false, false]);
    assert_eq!(
        lines
            .iter()
            .map(|line| strip_terminal_sequences(line))
            .collect::<Vec<_>>(),
        source_lines[..4]
    );

    scroll_mut(&scroll).scroll_by(2);
    lines = render(&scroll).lines;
    assert_eq!(thumb_rows(&lines), vec![false, true, true, false]);
    assert_eq!(
        lines
            .iter()
            .map(|line| strip_terminal_sequences(line))
            .collect::<Vec<_>>(),
        source_lines[2..6]
    );
    assert!(lines[1].rfind(CONTENT_BACKGROUND).unwrap() < lines[1].rfind(SCROLLBAR_BACKGROUND).unwrap());

    let hidden_at = Instant::now() + Duration::from_millis(30);
    assert!(scroll_mut(&scroll).poll(hidden_at));
    lines = render(&scroll).lines;
    assert_eq!(thumb_rows(&lines), vec![false, false, false, false]);

    scroll_mut(&scroll).scroll_to_end();
    lines = render(&scroll).lines;
    assert_eq!(thumb_rows(&lines), vec![false, false, true, true]);
    assert_eq!(
        lines
            .iter()
            .map(|line| strip_terminal_sequences(line))
            .collect::<Vec<_>>(),
        source_lines[4..]
    );

    let followed_content = component(Text::new(source_lines.join("\n"), 0, 0));
    let followed = component(ScrollView::new(
        followed_content,
        ScrollViewOptions {
            follow: Some(ScrollFollow::End),
            scrollbar: Some(ScrollViewScrollbar::Auto),
            scrollbar_style: Some(red_scrollbar()),
            ..ScrollViewOptions::default()
        },
    ));
    frame(&followed, 6, 4);
    assert_eq!(scroll_mut(&followed).scroll_top(), 4);
    let followed_text = followed.borrow().as_container().unwrap().children()[0].clone();
    text_mut(&followed_text).set_text(format!("{}\nabcde9", source_lines.join("\n")));
    let growth_frame = frame(&followed, 6, 4);
    assert_eq!(scroll_mut(&followed).scroll_top(), 5);
    assert!(
        growth_frame
            .lines
            .iter()
            .all(|line| !line.contains(SCROLLBAR_BACKGROUND))
    );

    let fitting_content = component(Text::new("1\n2", 0, 0));
    let automatic = component(ScrollView::new(
        fitting_content,
        ScrollViewOptions {
            scrollbar: Some(ScrollViewScrollbar::Auto),
            scrollbar_style: Some(red_scrollbar()),
            ..ScrollViewOptions::default()
        },
    ));
    frame(&automatic, 6, 4);
    scroll_mut(&automatic).scroll_by(1);
    assert!(
        frame(&automatic, 6, 4)
            .lines
            .iter()
            .all(|line| !line.contains(SCROLLBAR_BACKGROUND))
    );

    let always_fitting = component(ScrollView::new(
        text("1\n2"),
        ScrollViewOptions {
            scrollbar: Some(ScrollViewScrollbar::Always),
            scrollbar_style: Some(red_scrollbar()),
            ..ScrollViewOptions::default()
        },
    ));
    let always_fitting_frame = frame(&always_fitting, 6, 4);
    assert_eq!(always_fitting_frame.root.children[0].rect.width, 5);
    assert!(
        always_fitting_frame
            .lines
            .iter()
            .all(|line| line.contains(SCROLLBAR_BACKGROUND))
    );

    let always_overflowing = component(ScrollView::new(
        text(&source_lines.join("\n")),
        ScrollViewOptions {
            scrollbar: Some(ScrollViewScrollbar::Always),
            scrollbar_style: Some(red_scrollbar()),
            ..ScrollViewOptions::default()
        },
    ));
    let always_overflowing_frame = frame(&always_overflowing, 6, 4);
    assert_eq!(always_overflowing_frame.root.children[0].rect.width, 5);
    assert_eq!(
        always_overflowing_frame
            .lines
            .iter()
            .filter(|line| line.contains(SCROLLBAR_BACKGROUND))
            .count(),
        2
    );

    let thumb_height_for = |content_height: usize| {
        let content = component(Text::new(vec!["x"; content_height].join("\n"), 0, 0));
        let sized = component(ScrollView::new(
            content,
            ScrollViewOptions {
                scrollbar: Some(ScrollViewScrollbar::Auto),
                scrollbar_style: Some(red_scrollbar()),
                ..ScrollViewOptions::default()
            },
        ));
        frame(&sized, 6, 20);
        scroll_mut(&sized).scroll_by(1);
        frame(&sized, 6, 20)
            .lines
            .iter()
            .filter(|line| line.contains(SCROLLBAR_BACKGROUND))
            .count()
    };
    assert_eq!(thumb_height_for(21), 19);
    assert_eq!(thumb_height_for(40), 10);
    assert_eq!(thumb_height_for(100), 4);
    assert_eq!(thumb_height_for(400), 2);
}

#[test]
fn updates_reserved_scrollbar_layout_at_runtime() {
    let scroll = component(ScrollView::new(
        text("123456"),
        ScrollViewOptions {
            scrollbar: Some(ScrollViewScrollbar::Always),
            ..ScrollViewOptions::default()
        },
    ));
    let root = component(HStack::new(
        vec![entry(scroll.clone())],
        StackOptions {
            gap: None,
            align: Some(StackAlign::Start),
        },
    ));
    let always = frame(&root, 6, 2);
    assert_eq!(visible_lines(&always.lines), vec!["12345", "6"]);
    assert_eq!(always.root.children[0].rect.width, 6);
    assert_eq!(always.root.children[0].children[0].rect.width, 5);

    scroll_mut(&scroll).set_scrollbar(ScrollViewScrollbar::Hidden);
    assert_eq!(frame(&root, 6, 2).root.children[0].children[0].rect.width, 6);
    assert!(!scroll_mut(&scroll).is_scrollbar_visible());
}

#[test]
fn measures_nested_scroll_content_from_constrained_child_geometry() {
    let inner = component(ScrollView::new(text("1\n2\n3\n4\n5\n6"), ScrollViewOptions::default()));
    let mut inner_entry = entry(inner.clone());
    inner_entry.basis = Some(StackBasis::Cells(2));
    let outer_child = component(VStack::new(
        vec![inner_entry, entry(text("tail"))],
        StackOptions::default(),
    ));
    let outer = component(ScrollView::new(outer_child, ScrollViewOptions::default()));
    frame(&outer, 10, 2);

    assert_eq!(scroll_mut(&inner).viewport_height(), 2);
    assert_eq!(scroll_mut(&outer).scroll_by(10), 9);
    assert_eq!(scroll_mut(&outer).scroll_top(), 1);
}

#[test]
fn rebuilds_geometry_after_content_changes() {
    let content = text("one");
    let root = component(VStack::new(vec![entry(content.clone())], StackOptions::default()));
    let first = frame(&root, 10, 4);
    assert_eq!(first.root.children[0].lines.as_ref().unwrap().len(), 1);

    text_mut(&content).set_text("one\ntwo\nthree");
    let second = frame(&root, 10, 4);
    assert_eq!(second.root.children[0].lines.as_ref().unwrap().len(), 3);
}
