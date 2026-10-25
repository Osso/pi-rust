//! Shared primitives of the question/questionnaire native forms.
use pi_coding_agent::core::extensions::ExtensionUiFactoryContext;
use pi_coding_agent::modes::interactive::theme::{Theme, ThemeColor as C};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_tui::{
    components::{
        editor::{Editor, EditorOptions, EditorTheme},
        select_list::SelectListTheme,
    },
    utils::{visible_width, wrap_text_with_ansi},
};
use std::{cell::RefCell, rc::Rc};

pub struct FormEditor {
    pub editor: Editor,
    submitted: Rc<RefCell<Option<JsString>>>,
}
impl FormEditor {
    pub fn new(context: &ExtensionUiFactoryContext) -> Self {
        let style = |color| {
            let theme = context.theme.clone();
            Rc::new(move |text: &JsString| theme.fg(color, text)) as pi_tui::components::StyleFn
        };
        let theme = EditorTheme {
            border_color: style(C::Accent),
            prompt_prefix: None,
            select_list: SelectListTheme {
                selected_prefix: style(C::Accent),
                selected_text: style(C::Accent),
                description: style(C::Muted),
                scroll_info: style(C::Dim),
                no_match: style(C::Warning),
            },
        };
        let mut editor = Editor::new(Rc::new(context.tui.clone()), theme, EditorOptions::default());
        let submitted = Rc::new(RefCell::new(None));
        let captured = submitted.clone();
        editor.on_submit = Some(Rc::new(move |text| *captured.borrow_mut() = Some(text.to_owned())));
        Self { editor, submitted }
    }
    pub fn input(&mut self, data: &JsString) -> Option<JsString> {
        pi_tui::tui::Component::handle_input(&mut self.editor, &JsString::from(data));
        self.submitted.borrow_mut().take()
    }
}

pub fn wrap_prefixed(lines: &mut Vec<JsString>, width: usize, prefix: impl Into<JsString>, text: impl Into<JsString>) {
    let prefix = prefix.into();
    let text = text.into();
    let prefix_width = visible_width(&prefix);
    if prefix_width >= width {
        lines.extend(wrap_text_with_ansi(&js_format!("{}{}", prefix, text), width));
        return;
    }
    let continuation = JsString::from(" ".repeat(prefix_width));
    lines.extend(
        wrap_text_with_ansi(&text, width - prefix_width)
            .into_iter()
            .enumerate()
            .map(|(i, line)| js_format!("{}{}", if i == 0 { &prefix } else { &continuation }, line)),
    );
}

pub fn option_lines(
    lines: &mut Vec<JsString>,
    width: usize,
    theme: &Theme,
    options: &[(JsString, Option<JsString>, bool)],
    selected: i64,
    editing: bool,
) {
    for (i, (label, description, other)) in options.iter().enumerate() {
        let selected = i as i64 == selected;
        let prefix = if selected {
            theme.fg(C::Accent, "> ")
        } else {
            "  ".into()
        };
        let label = js_format!("{}. {}{}", i + 1, label, if *other && editing { " ✎" } else { "" });
        wrap_prefixed(
            lines,
            width,
            &prefix,
            theme.fg(
                if selected || (*other && editing) {
                    C::Accent
                } else {
                    C::Text
                },
                &label,
            ),
        );
        if let Some(description) = description.as_ref().filter(|s| !s.is_empty()) {
            wrap_prefixed(lines, width, "     ", theme.fg(C::Muted, description));
        }
    }
}

pub fn editor_lines(lines: &mut Vec<JsString>, width: usize, theme: &Theme, editor: &mut Editor) {
    lines.push(JsString::default());
    wrap_prefixed(lines, width, " ", theme.fg(C::Muted, "Your answer:"));
    lines.extend(
        pi_tui::tui::Component::render(editor, width.saturating_sub(2).max(1))
            .into_iter()
            .map(|line| js_format!(" {}", line)),
    );
}
