//! Opt-in native port of `overlay-test.ts`.
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor};
use pi_tui::tui::CURSOR_MARKER;
use pi_tui::{matches_key, visible_width};
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Item {
    label: &'static str,
    input: bool,
    text: Vec<char>,
    cursor: usize,
}
impl Item {
    fn edit_input(&mut self, data: &str) {
        if matches_key(data, "backspace") {
            if self.cursor > 0 {
                self.cursor -= 1;
                self.text.remove(self.cursor);
            }
        } else if matches_key(data, "left") {
            self.cursor = self.cursor.saturating_sub(1);
        } else if matches_key(data, "right") {
            self.cursor = (self.cursor + 1).min(self.text.len());
        } else if data.encode_utf16().count() == 1
            && let Some(ch) = data.chars().next().filter(|c| *c >= ' ')
        {
            self.text.insert(self.cursor, ch);
            self.cursor += 1;
        }
    }
}
struct State {
    selected: usize,
    items: Vec<Item>,
}
struct OverlayTest {
    focused: AtomicBool,
    state: Mutex<State>,
    theme: Arc<Theme>,
    done: ExtensionUiDone,
}
impl OverlayTest {
    fn new(theme: Arc<Theme>, done: ExtensionUiDone) -> Self {
        Self {
            focused: AtomicBool::new(false),
            theme,
            done,
            state: Mutex::new(State {
                selected: 0,
                items: [("Search", true), ("Run", true), ("Settings", false), ("Cancel", false)]
                    .into_iter()
                    .map(|(label, input)| Item {
                        label,
                        input,
                        text: Vec::new(),
                        cursor: 0,
                    })
                    .collect(),
            }),
        }
    }
    fn row(&self, content: &str) -> String {
        format!(
            "{}{}{}{}",
            self.theme.fg(ThemeColor::Border, "│"),
            content,
            " ".repeat(68usize.saturating_sub(visible_width(content))),
            self.theme.fg(ThemeColor::Border, "│")
        )
    }
    fn menu_row(&self, item: &Item, selected: bool) -> String {
        let prefix = if selected { " ▶ " } else { "   " };
        let color = if selected { ThemeColor::Accent } else { ThemeColor::Text };
        if !item.input {
            return self.row(&format!("{prefix}{}", self.theme.fg(color, item.label)));
        }
        let mut input: String = item.text.iter().collect();
        if selected {
            let before: String = item.text[..item.cursor].iter().collect();
            let ch = item.text.get(item.cursor).copied().unwrap_or(' ');
            let after: String = item.text.iter().skip(item.cursor + 1).collect();
            let marker = if self.focused.load(Ordering::SeqCst) {
                CURSOR_MARKER
            } else {
                ""
            };
            input = format!("{before}{marker}\x1b[7m{ch}\x1b[27m{after}");
        }
        self.row(&format!(
            "{prefix}{} {input}",
            self.theme.fg(color, &format!("{}:", item.label))
        ))
    }
}
impl ExtensionUiComponent for OverlayTest {
    fn width(&self) -> Option<pi_tui::tui::SizeValue> {
        Some(pi_tui::tui::SizeValue::Absolute(70.0))
    }

    fn focused(&self) -> Option<bool> {
        Some(self.focused.load(Ordering::SeqCst))
    }
    fn set_focused(&self, value: bool) {
        self.focused.store(value, Ordering::SeqCst);
    }
    fn handle_input(&self, data: &str) {
        if matches_key(data, "escape") {
            self.done.call(None);
            return;
        }
        let mut state = self.state.lock().expect("overlay state");
        let selected = state.selected;
        let current = &mut state.items[selected];
        if matches_key(data, "return") {
            let result = if current.input {
                json!({"action":current.label,"query":current.text.iter().collect::<String>()})
            } else {
                json!({"action":current.label})
            };
            drop(state);
            self.done.call(Some(result));
            return;
        }
        if matches_key(data, "up") {
            state.selected = state.selected.saturating_sub(1);
        } else if matches_key(data, "down") {
            state.selected = (state.selected + 1).min(state.items.len() - 1);
        } else if current.input {
            current.edit_input(data);
        }
    }
    fn render(&self, _width: usize) -> Vec<String> {
        let th = &self.theme;
        let mut lines = vec![
            th.fg(ThemeColor::Border, &format!("╭{}╮", "─".repeat(68))),
            self.row(&format!(" {}", th.fg(ThemeColor::Accent, "🧪 Overlay Test"))),
            self.row(""),
            self.row(&format!(
                " {}",
                th.fg(ThemeColor::Dim, "─── Edge Cases (borders should align) ───")
            )),
            self.row(&format!(
                " Wide: {}",
                th.fg(
                    ThemeColor::Warning,
                    "中文日本語한글テスト漢字繁體简体ひらがなカタカナ가나다라마바"
                )
            )),
            self.row(&format!(
                " Styled: {} {} {} {} {} {} {}",
                th.fg(ThemeColor::Error, "RED"),
                th.fg(ThemeColor::Success, "GREEN"),
                th.fg(ThemeColor::Warning, "YELLOW"),
                th.fg(ThemeColor::Accent, "ACCENT"),
                th.fg(ThemeColor::Dim, "DIM"),
                th.fg(ThemeColor::Error, "more"),
                th.fg(ThemeColor::Success, "colors")
            )),
            self.row(" Emoji: 👨‍👩‍👧‍👦 🇯🇵 🚀 💻 🎉 🔥 😀 🎯 🌟 💡 🎨 🔧 📦 🏆 🌈 🎪 🎭 🎬 🎮 🎲"),
            self.row(""),
            self.row(&format!(" {}", th.fg(ThemeColor::Dim, "─── Actions ───"))),
        ];
        let state = self.state.lock().expect("overlay state");
        for (i, item) in state.items.iter().enumerate() {
            lines.push(self.menu_row(item, i == state.selected));
        }
        lines.push(self.row(""));
        lines.push(self.row(&format!(
            " {}",
            th.fg(
                ThemeColor::Dim,
                "↑↓ navigate • type to input • Enter select • Esc cancel"
            )
        )));
        lines.push(th.fg(ThemeColor::Border, &format!("╰{}╯", "─".repeat(68))));
        lines
    }
}

pub fn overlay_test_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "overlay-test",
        Some("Test overlay rendering with edge cases".into()),
        None,
        Arc::new(|_, ctx| {
            Box::pin(async move {
                let ui = ctx.ui()?;
                let result = ui
                    .custom(
                        Arc::new(|context, done| {
                            pi_coding_agent::core::extensions::ready_custom_component(Box::new(OverlayTest::new(
                                context.theme,
                                done,
                            )))
                        }),
                        Some(ExtensionUiCustomOptions {
                            overlay: Some(true),
                            ..Default::default()
                        }),
                    )
                    .await
                    .map_err(HandlerError::new)?;
                if let Some(result) = result {
                    let action = result["action"].as_str().expect("overlay action");
                    let message = match result["query"].as_str().filter(|s| !s.is_empty()) {
                        Some(query) => format!("{action}: \"{query}\""),
                        None => action.to_owned(),
                    };
                    ui.notify(&message, Some(NotifyType::Info));
                }
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
