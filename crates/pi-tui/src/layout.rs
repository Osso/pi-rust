//! Ported from `packages/tui/src/layout.ts`.
//!
//! Builds a layout tree for stacks and scroll views, paints it into a fixed
//! viewport, and computes scrollbar geometry. TypeScript stores `parent` on each
//! box; the Rust port omits that back-reference (the slice has no consumer) and
//! exposes geometry through the box tree instead.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::components::scroll_view::ScrollState;
use super::components::stack::{allocate_stack_sizes, visible_stack_entries};
use super::layout_node::{LayoutNode, LayoutViewport, StackAlign, StackBasis, StackKind, get_layout_node};
use super::terminal_image::{crop_kitty_image_line, get_kitty_image_metadata, is_image_line};
use super::tui::{CURSOR_MARKER, ComponentRef, component_id, composite_tui_line};
use super::utils::{extract_ansi_code, get_grapheme_cell_range, slice_by_column, visible_width};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutRect {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

pub struct LayoutBox {
    pub component: ComponentRef,
    pub rect: LayoutRect,
    pub clip: LayoutRect,
    pub children: Vec<LayoutBox>,
    pub lines: Option<Vec<String>>,
    pub line_offset: usize,
    pub scroll_view: Option<Rc<RefCell<ScrollState>>>,
    pub scroll_content_lines: Option<Vec<String>>,
    pub layer: i64,
}

pub struct LayoutFrame {
    pub root: LayoutBox,
    pub width: usize,
    pub height: usize,
    pub lines: Vec<String>,
    pub primary_scroll_view: Option<Rc<RefCell<ScrollState>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollbarGeometry {
    pub column: i64,
    pub track_top: i64,
    pub track_height: i64,
    pub thumb_top: i64,
    pub thumb_height: i64,
    pub max_scroll_top: i64,
}

struct LayoutContext {
    viewport: LayoutViewport,
    render_cache: HashMap<usize, HashMap<usize, Vec<String>>>,
    request_render: Rc<dyn Fn()>,
    primary_scroll_view: Option<Rc<RefCell<ScrollState>>>,
}

impl LayoutContext {
    fn render_cached(&mut self, component: &ComponentRef, width: usize) -> Vec<String> {
        let safe_width = width.max(1);
        let widths = self.render_cache.entry(component_id(component)).or_default();
        if let Some(lines) = widths.get(&safe_width) {
            return lines.clone();
        }
        let lines = component.borrow_mut().render(safe_width);
        widths.insert(safe_width, lines.clone());
        lines
    }

    fn measure_height(&mut self, component: &ComponentRef, width: usize) -> usize {
        self.render_cached(component, width).len()
    }

    fn measure_width(&mut self, component: &ComponentRef, width: usize) -> usize {
        self.render_cached(component, width)
            .iter()
            .map(|line| visible_width(line))
            .max()
            .unwrap_or(0)
    }
}

struct Placement {
    x: i64,
    y: i64,
    width: usize,
    height: Option<usize>,
    clip: LayoutRect,
}

fn intersect(a: LayoutRect, b: LayoutRect) -> LayoutRect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    LayoutRect {
        x,
        y,
        width: (right - x).max(0),
        height: (bottom - y).max(0),
    }
}

fn translate_box(box_: &mut LayoutBox, delta_y: i64) {
    box_.rect.y += delta_y;
    for child in &mut box_.children {
        translate_box(child, delta_y);
    }
}

fn update_clips(box_: &mut LayoutBox, parent_clip: LayoutRect) {
    box_.clip = intersect(parent_clip, box_.rect);
    let clip = box_.clip;
    for child in &mut box_.children {
        update_clips(child, clip);
    }
}

fn allocated_height(height: Option<usize>, fallback: usize) -> usize {
    height.unwrap_or(fallback)
}

fn layout_component(context: &mut LayoutContext, component: &ComponentRef, placement: Placement) -> LayoutBox {
    let Placement {
        x,
        y,
        width,
        height,
        clip,
    } = placement;
    let safe_width = width.max(1);
    let Some(node) = get_layout_node(component) else {
        let lines = context.render_cached(component, safe_width);
        let allocated = allocated_height(height, lines.len());
        let line_offset = line_offset_for_cursor(&lines, allocated);
        return LayoutBox {
            component: Rc::clone(component),
            rect: LayoutRect {
                x,
                y,
                width: safe_width as i64,
                height: allocated as i64,
            },
            clip: intersect(
                clip,
                LayoutRect {
                    x,
                    y,
                    width: safe_width as i64,
                    height: allocated as i64,
                },
            ),
            children: Vec::new(),
            lines: Some(lines),
            line_offset,
            scroll_view: None,
            scroll_content_lines: None,
            layer: 0,
        };
    };

    match node {
        LayoutNode::Scroll(node) => layout_scroll_node(
            context,
            component,
            node,
            Placement {
                x,
                y,
                width: safe_width,
                height,
                clip,
            },
        ),
        LayoutNode::Stack(node) => {
            let placement = Placement {
                x,
                y,
                width: safe_width,
                height,
                clip,
            };
            match node.kind {
                StackKind::VStack => layout_vstack(context, component, node.entries, node.gap, placement),
                StackKind::HStack => layout_hstack(context, component, node.entries, node.gap, node.align, placement),
            }
        }
    }
}

fn line_offset_for_cursor(lines: &[String], allocated_height: usize) -> usize {
    if lines.len() <= allocated_height || allocated_height == 0 {
        return 0;
    }
    match lines.iter().position(|line| line.contains(CURSOR_MARKER)) {
        Some(cursor_line) if cursor_line >= allocated_height => cursor_line - allocated_height + 1,
        _ => 0,
    }
}

fn layout_scroll_node(
    context: &mut LayoutContext,
    component: &ComponentRef,
    node: super::layout_node::ScrollLayoutNode,
    placement: Placement,
) -> LayoutBox {
    let Placement {
        x,
        y,
        width,
        height,
        clip,
    } = placement;
    let safe_width = width.max(1);
    let previous_scroll_top = node.state.borrow().scroll_top() as i64;
    let content_width = node.state.borrow().get_content_width(safe_width);
    let mut child_box = layout_component(
        context,
        &node.component,
        Placement {
            x,
            y: y - previous_scroll_top,
            width: content_width,
            height: None,
            clip,
        },
    );
    let content_height = child_box.rect.height.max(0) as usize;
    let viewport_height = height.unwrap_or(content_height);
    node.state
        .borrow_mut()
        .update_layout(content_height, viewport_height, Rc::clone(&context.request_render));
    let scroll_top = node.state.borrow().scroll_top() as i64;
    translate_box(&mut child_box, previous_scroll_top - scroll_top);

    let scroll_state = Rc::clone(&node.state);
    if scroll_state.borrow().primary || context.primary_scroll_view.is_none() {
        context.primary_scroll_view = Some(Rc::clone(&scroll_state));
    }

    let rect = LayoutRect {
        x,
        y,
        width: safe_width as i64,
        height: viewport_height as i64,
    };
    let child_clip = intersect(clip, rect);
    update_clips(&mut child_box, child_clip);
    let scroll_content_lines = context.render_cached(&node.component, content_width);

    LayoutBox {
        component: Rc::clone(component),
        rect,
        clip: child_clip,
        children: vec![child_box],
        lines: None,
        line_offset: 0,
        scroll_view: Some(scroll_state),
        scroll_content_lines: Some(scroll_content_lines),
        layer: 0,
    }
}

fn layout_vstack(
    context: &mut LayoutContext,
    component: &ComponentRef,
    entries: Vec<super::layout_node::StackLayoutEntry>,
    gap: usize,
    placement: Placement,
) -> LayoutBox {
    let Placement {
        x,
        y,
        width,
        height,
        clip,
    } = placement;
    let safe_width = width.max(1);
    let entries = visible_stack_entries(&entries, context.viewport);
    let gap_total = entries.len().saturating_sub(1) * gap;
    let intrinsic_heights: Vec<usize> = entries
        .iter()
        .map(|entry| match entry.basis {
            Some(StackBasis::Cells(cells)) => cells,
            Some(StackBasis::Auto) | None => context.measure_height(&entry.component, safe_width),
        })
        .collect();
    let sizes = allocate_stack_sizes(&entries, &intrinsic_heights, height, gap);
    let natural_height: usize = sizes.iter().sum::<usize>() + gap_total;
    let allocated = allocated_height(height, natural_height);
    let rect = LayoutRect {
        x,
        y,
        width: safe_width as i64,
        height: allocated as i64,
    };
    let box_clip = intersect(clip, rect);
    let mut box_ = LayoutBox {
        component: Rc::clone(component),
        rect,
        clip: box_clip,
        children: Vec::new(),
        lines: None,
        line_offset: 0,
        scroll_view: None,
        scroll_content_lines: None,
        layer: 0,
    };

    let mut child_y = y;
    for (index, entry) in entries.iter().enumerate() {
        let child = layout_component(
            context,
            &entry.component,
            Placement {
                x,
                y: child_y,
                width: safe_width,
                height: Some(sizes[index]),
                clip: box_clip,
            },
        );
        box_.children.push(child);
        child_y += sizes[index] as i64 + gap as i64;
    }
    box_
}

fn layout_hstack(
    context: &mut LayoutContext,
    component: &ComponentRef,
    entries: Vec<super::layout_node::StackLayoutEntry>,
    gap: usize,
    align: StackAlign,
    placement: Placement,
) -> LayoutBox {
    let Placement {
        x,
        y,
        width,
        height,
        clip,
    } = placement;
    let safe_width = width.max(1);
    let entries = visible_stack_entries(&entries, context.viewport);
    let intrinsic_widths: Vec<usize> = entries
        .iter()
        .map(|entry| match entry.basis {
            Some(StackBasis::Cells(cells)) => cells,
            Some(StackBasis::Auto) | None => context.measure_width(&entry.component, safe_width),
        })
        .collect();
    let widths = allocate_stack_sizes(&entries, &intrinsic_widths, Some(safe_width), gap);
    let intrinsic_heights: Vec<usize> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| context.measure_height(&entry.component, widths[index].max(1)))
        .collect();
    let allocated = allocated_height(height, intrinsic_heights.iter().copied().max().unwrap_or(0));
    let rect = LayoutRect {
        x,
        y,
        width: safe_width as i64,
        height: allocated as i64,
    };
    let box_clip = intersect(clip, rect);
    let mut box_ = LayoutBox {
        component: Rc::clone(component),
        rect,
        clip: box_clip,
        children: Vec::new(),
        lines: None,
        line_offset: 0,
        scroll_view: None,
        scroll_content_lines: None,
        layer: 0,
    };

    let mut child_x = x;
    for (index, entry) in entries.iter().enumerate() {
        let natural_child_height = intrinsic_heights[index];
        let child_height = match align {
            StackAlign::Stretch => allocated,
            StackAlign::Start | StackAlign::Center | StackAlign::End => allocated.min(natural_child_height),
        };
        let mut child_y = y;
        match align {
            StackAlign::Center => child_y += (allocated.saturating_sub(child_height) / 2) as i64,
            StackAlign::End => child_y += allocated.saturating_sub(child_height) as i64,
            StackAlign::Stretch | StackAlign::Start => {}
        }
        let child_width = widths[index];
        if child_width == 0 {
            box_.children.push(LayoutBox {
                component: Rc::clone(&entry.component),
                rect: LayoutRect {
                    x: child_x,
                    y: child_y,
                    width: 0,
                    height: child_height as i64,
                },
                clip: LayoutRect {
                    x: child_x,
                    y: child_y,
                    width: 0,
                    height: 0,
                },
                children: Vec::new(),
                lines: None,
                line_offset: 0,
                scroll_view: None,
                scroll_content_lines: None,
                layer: 0,
            });
        } else {
            let child = layout_component(
                context,
                &entry.component,
                Placement {
                    x: child_x,
                    y: child_y,
                    width: child_width,
                    height: Some(child_height),
                    clip: box_clip,
                },
            );
            box_.children.push(child);
        }
        child_x += child_width as i64 + gap as i64;
    }
    box_
}

fn style_scrollbar_cell(
    line: &str,
    column: usize,
    total_width: usize,
    style: &super::components::text::StyleFn,
) -> String {
    if is_image_line(line) {
        return line.to_string();
    }

    let grapheme_range = get_grapheme_cell_range(line, column);
    let start = grapheme_range.map_or(column, |range| range.start);
    let end = grapheme_range.map_or(column + 1, |range| range.end);
    let before = slice_by_column(line, 0, start, true);
    let target = slice_by_column(line, start, end - start, true);
    let after = slice_by_column(line, end, total_width.saturating_sub(end), true);

    let mut target_prefix = String::new();
    let mut target_index = 0;
    while target_index < target.len() {
        let Some(ansi) = extract_ansi_code(&target, target_index) else {
            break;
        };
        target_prefix.push_str(ansi.code);
        target_index += ansi.length;
    }
    let target_text = if target_index < target.len() {
        target[target_index..].to_string()
    } else {
        " ".repeat(end - start)
    };
    let before_padding = " ".repeat(start.saturating_sub(visible_width(&before)));
    format!("{before}{before_padding}{target_prefix}{}{after}", style(&target_text))
}

pub fn get_scrollbar_geometry(box_: &LayoutBox) -> Option<ScrollbarGeometry> {
    let scroll_view = box_.scroll_view.as_ref()?;
    if !scroll_view.borrow().is_scrollbar_visible() || box_.rect.width <= 0 || box_.rect.height <= 0 {
        return None;
    }

    let content_height = box_
        .children
        .first()
        .map(|child| child.rect.height)
        .or_else(|| box_.scroll_content_lines.as_ref().map(|lines| lines.len() as i64))
        .unwrap_or(0);
    let track_height = box_.rect.height;

    let min_thumb_height = 2.min(track_height);
    let proportional = if content_height <= 0 {
        track_height as f64
    } else {
        (track_height * track_height) as f64 / content_height as f64
    };
    let thumb_height = min_thumb_height.max((proportional + 0.5).floor().min(track_height as f64) as i64);
    let max_scroll_top = (content_height - track_height).max(0);
    let max_thumb_top = track_height - thumb_height;
    let thumb_offset = if max_scroll_top == 0 {
        0
    } else {
        let scroll_top = scroll_view.borrow().scroll_top() as f64;
        ((scroll_top / max_scroll_top as f64) * max_thumb_top as f64 + 0.5).floor() as i64
    };
    let column = box_.rect.x + box_.rect.width - 1;
    if column < box_.clip.x || column >= box_.clip.x + box_.clip.width {
        return None;
    }

    Some(ScrollbarGeometry {
        column,
        track_top: box_.rect.y,
        track_height,
        thumb_top: box_.rect.y + thumb_offset,
        thumb_height,
        max_scroll_top,
    })
}

fn paint_scrollbar(box_: &LayoutBox, screen: &mut [String], total_width: usize) {
    let Some(geometry) = get_scrollbar_geometry(box_) else {
        return;
    };
    let Some(scroll_view) = &box_.scroll_view else {
        return;
    };
    let style = scroll_view.borrow().scrollbar_style().clone();

    for offset in 0..geometry.thumb_height {
        let row = geometry.thumb_top + offset;
        if row < box_.clip.y || row >= box_.clip.y + box_.clip.height || row < 0 || row >= screen.len() as i64 {
            continue;
        }
        let row_index = row as usize;
        screen[row_index] = style_scrollbar_cell(&screen[row_index], geometry.column as usize, total_width, &style);
    }
}

fn strip_osc133_zones(line: &str) -> &str {
    let mut rest = line;
    loop {
        let Some(after_prefix) = rest.strip_prefix("\x1b]133;") else {
            return rest;
        };
        let mut characters = after_prefix.chars();
        let Some(zone) = characters.next() else {
            return rest;
        };
        if !matches!(zone, 'A' | 'B' | 'C') {
            return rest;
        }
        let after_zone = &after_prefix[zone.len_utf8()..];
        if let Some(stripped) = after_zone.strip_prefix('\x07') {
            rest = stripped;
        } else if let Some(stripped) = after_zone.strip_prefix("\x1b\\") {
            rest = stripped;
        } else {
            return rest;
        }
    }
}

fn paint_box(box_: &LayoutBox, screen: &mut [String], total_width: usize) {
    if let Some(lines) = &box_.lines {
        let offset = box_.line_offset;
        let first_row = box_.rect.y.max(box_.clip.y).max(0);
        let last_row = (box_.rect.y + box_.rect.height)
            .min(box_.clip.y + box_.clip.height)
            .min(screen.len() as i64);
        for row in first_row..last_row {
            let source_index = offset as i64 + row - box_.rect.y;
            let Some(source_line) = usize::try_from(source_index).ok().and_then(|index| lines.get(index)) else {
                continue;
            };
            let mut line = strip_osc133_zones(source_line).to_string();
            if let Some(metadata) = get_kitty_image_metadata(&line) {
                let clip_bottom = (screen.len() as i64).min(box_.clip.y + box_.clip.height);
                let visible_rows = (metadata.rows as i64).min(clip_bottom - row);
                if visible_rows < metadata.rows as i64 {
                    line = crop_kitty_image_line(&line, 0, visible_rows);
                }
            }
            let row_index = row as usize;
            if box_.rect.x == 0
                && box_.rect.width >= total_width as i64
                && (is_image_line(&line) || screen[row_index].is_empty())
            {
                screen[row_index] = line;
            } else {
                screen[row_index] = composite_tui_line(
                    &screen[row_index],
                    &line,
                    box_.rect.x as usize,
                    box_.rect.width as usize,
                    total_width,
                );
            }
        }
    }
    for child in &box_.children {
        paint_box(child, screen, total_width);
    }

    if let (Some(scroll_view), Some(scroll_content_lines)) = (&box_.scroll_view, &box_.scroll_content_lines) {
        let scroll_top = scroll_view.borrow().scroll_top();
        if scroll_top > 0 && box_.rect.height > 0 {
            let mut image_row = scroll_top as i64 - 1;
            while image_row >= 0 {
                let image_line = scroll_content_lines
                    .get(image_row as usize)
                    .map(String::as_str)
                    .unwrap_or("");
                if let Some(metadata) = get_kitty_image_metadata(image_line) {
                    let hidden_rows = scroll_top as i64 - image_row;
                    if hidden_rows < metadata.rows as i64 {
                        let visible_rows = box_.rect.height.min(metadata.rows as i64 - hidden_rows);
                        let cropped = crop_kitty_image_line(image_line, hidden_rows, visible_rows);
                        if box_.rect.x == 0
                            && box_.rect.width >= total_width as i64
                            && let Some(target) = screen.get_mut(box_.rect.y as usize)
                        {
                            *target = cropped;
                        }
                    }
                    break;
                }
                if !image_line.is_empty() {
                    break;
                }
                image_row -= 1;
            }
        }
    }

    paint_scrollbar(box_, screen, total_width);
}

/// Lay out `root` into a `width` × `height` viewport and paint the frame.
pub fn render_layout_frame(
    root: &ComponentRef,
    width: usize,
    height: usize,
    request_render: Rc<dyn Fn()>,
) -> LayoutFrame {
    let safe_width = width.max(1);
    let safe_height = height.max(1);
    let mut context = LayoutContext {
        viewport: LayoutViewport {
            width: safe_width,
            height: safe_height,
        },
        render_cache: HashMap::new(),
        request_render,
        primary_scroll_view: None,
    };
    let viewport_clip = LayoutRect {
        x: 0,
        y: 0,
        width: safe_width as i64,
        height: safe_height as i64,
    };
    let root_box = layout_component(
        &mut context,
        root,
        Placement {
            x: 0,
            y: 0,
            width: safe_width,
            height: Some(safe_height),
            clip: viewport_clip,
        },
    );
    let mut lines = vec![String::new(); safe_height];
    paint_box(&root_box, &mut lines, safe_width);
    LayoutFrame {
        root: root_box,
        width: safe_width,
        height: safe_height,
        lines,
        primary_scroll_view: context.primary_scroll_view,
    }
}

fn contains_point(rect: LayoutRect, x: i64, y: i64) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

pub fn get_scroll_view_box<'a>(
    frame: &'a LayoutFrame,
    scroll_view: &Rc<RefCell<ScrollState>>,
) -> Option<&'a LayoutBox> {
    fn visit<'a>(box_: &'a LayoutBox, scroll_view: &Rc<RefCell<ScrollState>>) -> Option<&'a LayoutBox> {
        if box_
            .scroll_view
            .as_ref()
            .is_some_and(|state| Rc::ptr_eq(state, scroll_view))
        {
            return Some(box_);
        }
        box_.children.iter().find_map(|child| visit(child, scroll_view))
    }
    visit(&frame.root, scroll_view)
}

pub fn get_scroll_views_at(frame: &LayoutFrame, x: i64, y: i64) -> Vec<Rc<RefCell<ScrollState>>> {
    fn visit(box_: &LayoutBox, x: i64, y: i64, depth: usize, result: &mut Vec<(Rc<RefCell<ScrollState>>, usize)>) {
        if !contains_point(box_.clip, x, y) {
            return;
        }
        if let Some(scroll_view) = &box_.scroll_view
            && contains_point(box_.rect, x, y)
        {
            result.push((Rc::clone(scroll_view), depth));
        }
        for child in &box_.children {
            visit(child, x, y, depth + 1, result);
        }
    }
    let mut result = Vec::new();
    visit(&frame.root, x, y, 0, &mut result);
    result.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    result.into_iter().map(|(state, _)| state).collect()
}
