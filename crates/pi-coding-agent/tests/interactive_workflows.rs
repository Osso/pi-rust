//! Behavioral tests for interactive workflow helpers.

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::cell::RefCell;
use std::rc::Rc;

use pi_coding_agent::modes::interactive::interactive_root_compositor::{
    InteractiveRootCompositorOptions, create_interactive_root_compositor,
};
use pi_coding_agent::modes::interactive::working_editor::{
    clear_working_editor, clear_working_editor_position, position_working_editor, supports_working_prompt_animation,
    sync_working_editor,
};
use pi_tui::tui::component;
use pi_tui::{Component, EditorComponent, LoaderIndicatorOptions, RootCompositor};

#[derive(Default)]
struct TestEditor {
    working: bool,
    indicator: Option<LoaderIndicatorOptions>,
    origin: Option<(i64, i64)>,
}

impl Component for TestEditor {
    fn render(&mut self, _width: usize) -> Vec<JsString> {
        let frames = self
            .indicator
            .as_ref()
            .and_then(|indicator| indicator.frames.as_ref())
            .map(|frames| JsString::join(frames, ""))
            .unwrap_or_else(|| JsString::from("default"));
        vec![js_format!(
            "{}:{}",
            if self.working { "working" } else { "idle" },
            frames
        )]
    }
}

impl EditorComponent for TestEditor {
    fn get_text(&self) -> Result<JsString, pi_js_compat::callback_error::CallbackError> {
        Ok(JsString::default())
    }

    fn set_text(&mut self, _text: &JsString) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        Ok(())
    }

    fn supports_working_prompt(&self) -> bool {
        true
    }

    fn set_working(&mut self, working: bool) {
        self.working = working;
    }

    fn set_working_indicator(&mut self, options: Option<LoaderIndicatorOptions>) {
        self.indicator = options;
    }

    fn set_screen_origin(&mut self, row: i64, col: i64) {
        self.origin = Some((row, col));
    }

    fn clear_screen_origin(&mut self) {
        self.origin = None;
    }
}

#[derive(Default)]
struct PlainEditor;

impl Component for PlainEditor {
    fn render(&mut self, _width: usize) -> Vec<JsString> {
        vec![JsString::from("plain")]
    }
}

impl EditorComponent for PlainEditor {
    fn get_text(&self) -> Result<JsString, pi_js_compat::callback_error::CallbackError> {
        Ok(JsString::default())
    }

    fn set_text(&mut self, _text: &JsString) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        Ok(())
    }
}

#[test]
fn working_editor_syncs_indicator_state_and_origin() {
    let mut editor = TestEditor::default();
    let indicator = LoaderIndicatorOptions {
        frames: Some(vec![JsString::from("a"), JsString::from("b")]),
        interval_ms: Some(120),
    };

    sync_working_editor(&mut editor, true, Some(indicator.clone()));
    assert_eq!(editor.indicator, Some(indicator));
    assert!(editor.working);
    assert_eq!(editor.render(20), vec!["working:ab"]);

    position_working_editor(&mut editor, 7, 2);
    assert_eq!(editor.origin, Some((7, 2)));

    clear_working_editor(&mut editor);
    assert!(!editor.working);
    assert_eq!(editor.origin, None);

    clear_working_editor_position(&mut editor);
    assert_eq!(editor.origin, None);
}

#[test]
fn working_editor_rejects_wrong_width_indicators() {
    let mut editor = TestEditor::default();
    let wide = LoaderIndicatorOptions {
        frames: Some(vec![JsString::from("ab")]),
        interval_ms: None,
    };
    assert!(!supports_working_prompt_animation(&editor, Some(&wide)));

    sync_working_editor(&mut editor, true, Some(wide.clone()));
    assert!(!editor.working);
    assert_eq!(editor.indicator, None);

    let narrow = LoaderIndicatorOptions {
        frames: Some(vec![JsString::from("x")]),
        interval_ms: Some(120),
    };
    assert!(supports_working_prompt_animation(&editor, Some(&narrow)));
    sync_working_editor(&mut editor, true, Some(narrow.clone()));
    assert!(editor.working);
    assert_eq!(editor.indicator, Some(narrow));
}

#[test]
fn working_editor_leaves_arbitrary_custom_editors_unchanged() {
    let mut editor = PlainEditor;
    assert!(!supports_working_prompt_animation(
        &editor,
        Some(&LoaderIndicatorOptions {
            frames: Some(vec![JsString::from("x")]),
            interval_ms: None,
        })
    ));
    sync_working_editor(
        &mut editor,
        true,
        Some(LoaderIndicatorOptions {
            frames: Some(vec![JsString::from("x")]),
            interval_ms: None,
        }),
    );
    position_working_editor(&mut editor, 3, 1);
    clear_working_editor(&mut editor);
    assert_eq!(editor.render(20), vec!["plain"]);
}

struct TestComponent {
    lines: Rc<RefCell<Vec<String>>>,
}

impl TestComponent {
    fn new(lines: &[&str]) -> Self {
        Self {
            lines: Rc::new(RefCell::new(lines.iter().map(|line| line.to_string()).collect())),
        }
    }
}

impl Component for TestComponent {
    fn render(&mut self, _width: usize) -> Vec<JsString> {
        self.lines
            .borrow()
            .clone()
            .into_iter()
            .map(JsString::from)
            .collect::<Vec<_>>()
    }
}

#[test]
fn interactive_root_compositor_preserves_order_and_reports_layouts() {
    let terminal_height = Rc::new(RefCell::new(12usize));
    let header = TestComponent::new(&["header"]);
    let loaded_resources = TestComponent::new(&["resources"]);
    let chat = TestComponent::new(&["chat"]);
    let transcript_tail = TestComponent::new(&["tail"]);
    let pending_messages = TestComponent::new(&["pending"]);
    let status = TestComponent::new(&["status"]);
    let widget_above = TestComponent::new(&["above"]);
    let editor = TestComponent::new(&["editor top", "editor prompt", "editor bottom"]);
    let widget_below = TestComponent::new(&["below"]);
    let footer = TestComponent::new(&["footer"]);

    let editor_origin = Rc::new(RefCell::new(None));
    let transcript_tail_layout = Rc::new(RefCell::new(None));
    let get_height = {
        let terminal_height = terminal_height.clone();
        Rc::new(move || *terminal_height.borrow())
    };
    let on_editor_layout = {
        let editor_origin = editor_origin.clone();
        Rc::new(move |rect: pi_tui::RenderRegionRect| {
            *editor_origin.borrow_mut() = Some((rect.row, rect.col));
        })
    };
    let on_transcript_tail_layout = {
        let transcript_tail_layout = transcript_tail_layout.clone();
        Rc::new(move |layout| {
            *transcript_tail_layout.borrow_mut() = Some(layout);
        })
    };

    let editor_lines = editor.lines.clone();
    let footer_lines = footer.lines.clone();
    let mut compositor: RootCompositor = create_interactive_root_compositor(InteractiveRootCompositorOptions {
        get_height,
        header: component(header),
        loaded_resources: component(loaded_resources),
        chat: component(chat),
        transcript_tail: component(transcript_tail),
        pending_messages: component(pending_messages),
        on_transcript_tail_layout,
        status: component(status),
        widget_above: component(widget_above),
        editor: component(editor),
        widget_below: component(widget_below),
        footer: component(footer),
        on_chat_layout: Rc::new(|_| {}),
        on_widget_above_layout: None,
        on_status_layout: Rc::new(|_| {}),
        on_editor_layout,
    });

    assert_eq!(
        compositor.render(20),
        vec![
            "header",
            "resources",
            "chat",
            "tail",
            "pending",
            "status",
            "above",
            "editor top",
            "editor prompt",
            "editor bottom",
            "below",
            "footer",
        ]
    );
    assert_eq!(*editor_origin.borrow(), Some((7, 0)));
    let layout = transcript_tail_layout.borrow().expect("tail layout");
    assert_eq!(
        (layout.rect.row, layout.rect.col, layout.rect.width, layout.rect.height),
        (3, 0, 20, 1)
    );
    assert_eq!(layout.flow_end_row, 5);
    assert_eq!(layout.bottom_row, 5);
    assert_eq!(layout.bottom_height, 7);
    assert_eq!(layout.viewport_height, 12);

    *terminal_height.borrow_mut() = 14;
    compositor.render(20);
    assert_eq!(*editor_origin.borrow(), Some((9, 0)));

    *footer_lines.borrow_mut() = vec!["footer 1".to_string(), "footer 2".to_string()];
    compositor.render(20);
    assert_eq!(*editor_origin.borrow(), Some((8, 0)));

    *editor_lines.borrow_mut() = vec![
        "editor top".to_string(),
        "editor prompt".to_string(),
        "editor middle".to_string(),
        "editor bottom".to_string(),
    ];
    compositor.render(20);
    assert_eq!(*editor_origin.borrow(), Some((7, 0)));
}
