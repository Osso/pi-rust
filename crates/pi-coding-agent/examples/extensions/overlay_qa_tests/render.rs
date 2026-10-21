use super::*;
use pi_tui::utils::{truncate_to_width, truncate_to_width_with};
fn fg(m: &Model, c: ThemeColor, s: impl AsRef<str>) -> String {
    m.theme.fg(c, s.as_ref())
}
fn dim(m: &Model, s: &str) -> String {
    fg(m, ThemeColor::Dim, s)
}
fn accent(m: &Model, s: impl AsRef<str>) -> String {
    fg(m, ThemeColor::Accent, s)
}
pub(super) fn boxed(m: &Model, lines: Vec<String>, width: usize, title: &str) -> Vec<String> {
    let inner = width.saturating_sub(2).max(1);
    let title = truncate_to_width(&format!(" {title} "), inner);
    let left = inner.saturating_sub(visible_width(&title)) / 2;
    let right = inner.saturating_sub(visible_width(&title) + left);
    let mut result = vec![format!(
        "{}{}{}",
        fg(m, ThemeColor::Border, format!("╭{}", "─".repeat(left))),
        accent(m, title),
        fg(m, ThemeColor::Border, format!("{}╮", "─".repeat(right)))
    )];
    result.extend(lines.into_iter().map(|line| row(m, &line, inner, ThemeColor::Border)));
    result.push(fg(m, ThemeColor::Border, format!("╰{}╯", "─".repeat(inner))));
    result
}
pub(super) fn row(m: &Model, line: &str, inner: usize, color: ThemeColor) -> String {
    format!(
        "{}{}{}",
        fg(m, color, "│"),
        truncate_to_width_with(line, inner, "...", true),
        fg(m, color, "│")
    )
}
fn plain_box(m: &Model, lines: Vec<String>, width: usize, color: ThemeColor) -> Vec<String> {
    let inner = width.saturating_sub(2).max(1);
    let mut result = vec![fg(m, color, format!("╭{}╮", "─".repeat(inner)))];
    result.extend(lines.iter().map(|s| row(m, s, inner, color)));
    result.push(fg(m, color, format!("╰{}╯", "─".repeat(inner))));
    result
}
pub(super) fn render(m: &Model, width: usize) -> Vec<String> {
    let s = m.state.borrow();
    match m.demo {
        Demo::Anchors => boxed(
            m,
            vec![
                "".into(),
                format!(" Current: {}", accent(m, ANCHORS[m.index].1)),
                "".into(),
                format!(" {}", dim(m, "Space/→ = next anchor")),
                format!(" {}", dim(m, "Enter = confirm")),
                format!(" {}", dim(m, "Esc = cancel")),
                "".into(),
            ],
            width,
            "Anchor Test",
        ),
        Demo::Margins => boxed(
            m,
            vec![
                "".into(),
                format!(" {}", accent(m, MARGINS[m.index])),
                "".into(),
                format!(" {}", dim(m, "Space/→ = next config")),
                format!(" {}", dim(m, "Esc = close")),
                "".into(),
            ],
            width,
            "Margin Test",
        ),
        Demo::Percent => boxed(
            m,
            vec![
                "".into(),
                format!(" {}", accent(m, PERCENT[m.index].0)),
                "".into(),
                format!(" {}", dim(m, "Space/→ = next")),
                format!(" {}", dim(m, "Esc = close")),
                "".into(),
            ],
            width,
            "Percent Test",
        ),
        Demo::Edge => boxed(
            m,
            vec![
                "".into(),
                " This overlay is at the".into(),
                " right edge of terminal.".into(),
                "".into(),
                format!(" {}", dim(m, "Verify right border")),
                format!(" {}", dim(m, "aligns with edge.")),
                "".into(),
                format!(" {}", dim(m, "Press Esc to close")),
                "".into(),
            ],
            width,
            "Edge Test",
        ),
        Demo::MaxHeight => {
            let mut lines = vec![
                fg(m, ThemeColor::Warning, " ⚠ Rendering 21 lines, maxHeight: 10"),
                dim(m, " Lines 11-21 truncated (no bottom border)"),
                "".into(),
            ];
            lines.extend((1..=14).map(|i| format!(" Line {i} of 14")));
            lines.extend(["".into(), dim(m, " Press Esc to close")]);
            boxed(m, lines, width, "MaxHeight Test")
        }
        Demo::Stack => stack(m, width),
        Demo::Animation => animation(m, &s, width),
        Demo::Sidepanel => sidepanel(m, &s, width),
        Demo::Toggle => boxed(
            m,
            vec![
                "".into(),
                accent(m, " Toggle Demo"),
                "".into(),
                " This overlay demonstrates the".into(),
                " onHandle callback API.".into(),
                "".into(),
                format!(" Toggle count: {}", accent(m, s.toggle_count.to_string())),
                "".into(),
                dim(m, " Press 't' to hide for 1 second"),
                dim(m, " (demonstrates setHidden API)"),
                "".into(),
                dim(m, " In real usage, a global keybinding"),
                dim(m, " would toggle visibility externally."),
                "".into(),
                dim(m, " Press Esc to close"),
                "".into(),
            ],
            width,
            "Toggle Demo",
        ),
        Demo::Passive => {
            let display = if s.typed.is_empty() {
                dim(m, "(type here)")
            } else {
                String::from_utf16_lossy(&s.typed)
            };
            boxed(
                m,
                vec![
                    "".into(),
                    format!(
                        " {}",
                        dim(m, &format!("focused={} inputs={}", m.focused.get(), s.inputs))
                    ),
                    format!(
                        " {}",
                        dim(
                            m,
                            &format!("last: {}", if s.last_input.is_empty() { "none" } else { &s.last_input })
                        )
                    ),
                    "".into(),
                    format!(" > {display}"),
                    "".into(),
                    dim(m, " Type to prove input goes here."),
                    dim(m, " Press Esc to close both."),
                    "".into(),
                ],
                width,
                "Non-Capturing Demo",
            )
        }
        Demo::Focus => {
            let entries = m.entries.borrow();
            let label = entries
                .iter()
                .find(|e| e.handle.is_focused())
                .map(|e| panels::LABELS[e.index])
                .unwrap_or("Controller");
            boxed(
                m,
                vec![
                    "".into(),
                    format!(" Current focus: {}", accent(m, label)),
                    "".into(),
                    " Three overlapping panels above are".into(),
                    format!(" {} overlays controlled with", accent(m, "nonCapturing")),
                    " raw OverlayHandle.focus()/hide().".into(),
                    "".into(),
                    " Type in the focused panel's input.".into(),
                    " Focused panel renders on top.".into(),
                    "".into(),
                    dim(m, " Tab/Shift+Tab = cycle panels"),
                    dim(m, " Esc/Ctrl+D = dismiss panel"),
                    dim(m, " Ctrl+C = close all"),
                    "".into(),
                ],
                width,
                "Focus + Input Demo",
            )
        }
        Demo::Streaming => streaming(m, &s, width),
        Demo::Overflow => m
            .overflow
            .borrow()
            .as_ref()
            .expect("process initialized")
            .render(m, width),
    }
}
fn stack(m: &Model, width: usize) -> Vec<String> {
    let color = panels::COLORS[m.index];
    let inner = width.saturating_sub(2).max(1);
    let positions = ["back (red border)", "middle (green border)", "front (blue border)"];
    let mut lines = vec![
        format!(" Overlay {}", accent(m, format!("#{}", m.index + 1))),
        format!(" Layer: {}", fg(m, color, positions[m.index])),
        "".into(),
    ];
    lines.extend((0..5).map(|_| format!(" {} ", "░".repeat(inner.saturating_sub(2)))));
    lines.extend(["".into(), dim(m, " Press Enter/Esc to close")]);
    plain_box(m, lines, width, color)
}
fn sidepanel(m: &Model, s: &State, width: usize) -> Vec<String> {
    let inner = width.saturating_sub(2).max(1);
    let separator = format!(
        "{}{}{}",
        fg(m, ThemeColor::Border, "├"),
        fg(m, ThemeColor::Border, "─".repeat(inner)),
        fg(m, ThemeColor::Border, "┤")
    );
    let mut result = vec![
        fg(m, ThemeColor::Border, format!("╭{}╮", "─".repeat(inner))),
        row(m, &accent(m, " Responsive Sidepanel"), inner, ThemeColor::Border),
        separator.clone(),
    ];
    for (i, item) in ["Dashboard", "Messages", "Settings", "Help", "About"]
        .iter()
        .enumerate()
    {
        let line = if i == s.selected {
            format!("{}{}", accent(m, "→ "), accent(m, item))
        } else {
            format!("  {item}")
        };
        result.push(row(m, &line, inner, ThemeColor::Border));
    }
    result.push(separator);
    for (color, text) in [
        (ThemeColor::Warning, " ⚠ Resize terminal < 100 cols"),
        (ThemeColor::Warning, "   to see panel auto-hide"),
        (ThemeColor::Dim, " Uses visible: (w) => w >= 100"),
        (ThemeColor::Dim, " ↑↓ navigate | Esc close"),
    ] {
        result.push(row(m, &fg(m, color, text), inner, ThemeColor::Border));
    }
    result.push(fg(m, ThemeColor::Border, format!("╰{}╯", "─".repeat(inner))));
    result
}
fn animation(m: &Model, s: &State, width: usize) -> Vec<String> {
    let inner = width.saturating_sub(2).max(1);
    let bar_width = inner.saturating_sub(4).max(12);
    let pos = (((s.frame as f64 / 10.).sin() + 1.) * (bar_width - 10) as f64 / 2.)
        .floor()
        .max(0.) as usize;
    let bar = format!(
        "{}{}{}",
        " ".repeat(pos),
        accent(m, "██████████"),
        " ".repeat((bar_width - 10).saturating_sub(pos))
    );
    let rgb = hsl_to_rgb((s.frame * 3 % 360) as f64 / 360., 0.8, 0.5);
    let block = format!("\x1b[48;2;{};{};{}m{}\x1b[0m", rgb[0], rgb[1], rgb[2], "  ".repeat(10));
    plain_box(
        m,
        vec![
            accent(m, " Animation Demo (~30 FPS)"),
            "".into(),
            format!(" Frame: {}", accent(m, s.frame.to_string())),
            format!(" FPS: {}", fg(m, ThemeColor::Success, s.fps.to_string())),
            "".into(),
            format!(" {bar}"),
            format!(
                " Spinner: {}",
                fg(m, ThemeColor::Warning, ["◐", "◓", "◑", "◒"][s.frame % 4])
            ),
            format!(" Color: {block}"),
            "".into(),
            dim(m, " This proves overlays can handle"),
            dim(m, " real-time game-like rendering."),
            dim(m, " (pi-doom uses same approach)"),
            "".into(),
            dim(m, " Press Esc to close"),
        ],
        width,
        ThemeColor::Border,
    )
}
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    if s == 0. {
        return [(l * 255.).round() as u8; 3];
    }
    let q = if l < 0.5 { l * (1. + s) } else { l + s - l * s };
    let p = 2. * l - q;
    [h + 1. / 3., h, h - 1. / 3.].map(|mut t| {
        if t < 0. {
            t += 1.;
        }
        if t > 1. {
            t -= 1.;
        }
        let value = if t < 1. / 6. {
            p + (q - p) * 6. * t
        } else if t < 0.5 {
            q
        } else if t < 2. / 3. {
            p + (q - p) * (2. / 3. - t) * 6.
        } else {
            p
        };
        (value * 255.).round() as u8
    })
}
fn streaming(m: &Model, s: &State, width: usize) -> Vec<String> {
    let focused = if s.focus_index == -1 {
        fg(m, ThemeColor::Success, "Controller (this panel)")
    } else {
        m.entries
            .borrow()
            .get(s.focus_index as usize)
            .map(|entry| panels::STREAM_LABELS[entry.index])
            .unwrap_or("?")
            .to_string()
    };
    let separator = dim(m, &" ─".repeat(width.saturating_sub(2) / 2));
    let mut lines = vec![
        "".into(),
        format!(" Current focus: {}", accent(m, focused)),
        "".into(),
        " Simulated streaming output:".into(),
        separator.clone(),
    ];
    lines.extend(s.stream_lines.iter().map(|line| format!(" {}", dim(m, line))));
    while lines.len() < 12 {
        lines.push("".into());
    }
    lines.extend([
        separator,
        "".into(),
        format!(" Three {} input panels on the left.", accent(m, "nonCapturing")),
        " Tab cycles: Controller → Panel A → B → C → Controller".into(),
        " Type in each panel to test input routing.".into(),
        "".into(),
        dim(m, " Tab = cycle focus | Esc = close all"),
        "".into(),
    ]);
    boxed(m, lines, width, "Streaming + Input Test")
}
