#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::terminal::{ProcessTerminal, Terminal};
use pi_tui::tui::{Component, OverlayOptions, SizeValue, TuiExt, TuiRenderer, component};
use serde_json::json;
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct CaptureTerminal {
    writes: Vec<JsString>,
    pending_input: Vec<JsString>,
    stop_error: Option<pi_js_compat::callback_error::CallbackError>,
    stop_events: Vec<serde_json::Value>,
}
impl Terminal for CaptureTerminal {
    fn start(&mut self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.write(&"\x1b[?2004h".into());
        Ok(())
    }
    fn stop(&mut self) -> Result<(), pi_js_compat::callback_error::CallbackError> {
        self.write(&"\x1b[?2004l".into());
        let log = std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".pi/agent/pi-crash.log");
        self.stop_events.push(json!({"stop":true,"log":log.exists()}));
        match &self.stop_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
    fn drain_input(&mut self, _: u64, _: u64) {}
    fn write(&mut self, value: &JsString) {
        self.writes.push(value.clone());
    }
    fn poll_events(&mut self) -> Vec<pi_tui::terminal::TerminalEvent> {
        std::mem::take(&mut self.pending_input)
            .into_iter()
            .map(pi_tui::terminal::TerminalEvent::Input)
            .collect()
    }
    fn columns(&self) -> usize {
        12
    }
    fn rows(&self) -> usize {
        5
    }
    fn kitty_protocol_active(&self) -> bool {
        true
    }
    fn move_by(&mut self, lines: isize) {
        if lines > 0 {
            self.write(&format!("\x1b[{lines}B").into());
        } else if lines < 0 {
            self.write(&format!("\x1b[{}A", -lines).into());
        }
    }
    fn hide_cursor(&mut self) {
        self.write(&"\x1b[?25l".into());
    }
    fn show_cursor(&mut self) {
        self.write(&"\x1b[?25h".into());
    }
    fn clear_line(&mut self) {
        self.write(&"\x1b[K".into());
    }
    fn clear_from_cursor(&mut self) {
        self.write(&"\x1b[J".into());
    }
    fn clear_screen(&mut self) {
        self.write(&"\x1b[2J\x1b[H".into());
    }
    fn set_title(&mut self, value: &JsString) {
        self.write(&(JsString::from("\x1b]0;") + value + "\x07"));
    }
    fn set_progress(&mut self, _: bool) {}
}
struct Lines(Rc<RefCell<Vec<JsString>>>);
impl Component for Lines {
    fn render(&mut self, _: usize) -> Vec<JsString> {
        self.0.borrow().clone()
    }
}

#[test]
fn raw_frames_overlays_fixed_cells_and_diff_buffers_match_native_source_writes() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    pi_tui::terminal_image::set_capabilities(pi_tui::terminal_image::TerminalCapabilities {
        images: None,
        true_color: false,
        hyperlinks: false,
    });
    let units = vec![0xd800, 0x61, 0xdc00, 0x62, 0xd83d, 0xde00];
    let response = oracle.request(&json!({"operation":"frame", "units":units}));
    assert_eq!(response["ok"], true, "{response}");
    let raw = JsString::from_utf16(units);
    let terminal = Rc::new(RefCell::new(CaptureTerminal::default()));
    let tui = TuiRenderer::new(terminal.clone());
    let lines = Rc::new(RefCell::new(vec![JsString::from("x") + &raw, JsString::from("tail")]));
    tui.add_child(component(Lines(lines.clone())));
    let mut frames = Vec::new();
    tui.start().unwrap();
    tui.flush_pending().unwrap();
    frames.push(std::mem::take(&mut terminal.borrow_mut().writes));
    lines.borrow_mut()[0] = JsString::from("y") + &raw;
    tui.request_render(false);
    tui.flush_pending().unwrap();
    frames.push(std::mem::take(&mut terminal.borrow_mut().writes));
    let overlay = component(Lines(Rc::new(RefCell::new(vec![raw.clone()]))));
    let handle = tui.show_overlay(
        overlay,
        Some(OverlayOptions {
            width: Some(SizeValue::Absolute(4.0)),
            row: Some(SizeValue::Absolute(0.0)),
            col: Some(SizeValue::Absolute(2.0)),
            ..OverlayOptions::default()
        }),
    );
    tui.flush_pending().unwrap();
    frames.push(std::mem::take(&mut terminal.borrow_mut().writes));
    handle.hide();
    tui.flush_pending().unwrap();
    frames.push(std::mem::take(&mut terminal.borrow_mut().writes));
    let cell = TuiRenderer::create_fixed_cell(&tui);
    cell.place(0, 1);
    let updated = cell.update(JsString::from_utf16(vec![0xd800, 0x7a, 0xdc00]));
    frames.push(std::mem::take(&mut terminal.borrow_mut().writes));
    tui.stop().unwrap();
    frames.push(std::mem::take(&mut terminal.borrow_mut().writes));
    let stop = JsString::join(frames.pop().unwrap(), "");
    let actual = serde_json::to_string(&(frames, stop, updated)).unwrap();
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(response["json"].as_str().unwrap()).unwrap()
    );
}

#[test]
fn width_error_preserves_cleanup_throw_precedence_and_crash_log() {
    use pi_tui::tui::RendererApi;
    const CHILD: &str = "PI_TUI_WIDTH_PROOF_HOME";
    let Some(home) = std::env::var_os(CHILD) else {
        let mut failures = Vec::new();
        for stop_error in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "width_error_preserves_cleanup_throw_precedence_and_crash_log",
                    "--nocapture",
                ])
                .env(CHILD, directory.path())
                .env("HOME", directory.path())
                .env("PI_TUI_WIDTH_STOP_ERROR", if stop_error { "1" } else { "0" })
                .output()
                .unwrap();
            if !output.status.success() {
                failures.push(format!(
                    "stop_error={stop_error}, stdout {} stderr {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        return;
    };
    let home = std::path::PathBuf::from(home);
    let stop_error = std::env::var("PI_TUI_WIDTH_STOP_ERROR").unwrap() == "1";
    let mut source = oracle::TsOracle::spawn("tui-width-error-oracle.mts", "packages/tui/src");
    let expected = source.request(&json!({"home":home,"stopError":stop_error}));
    assert_eq!(expected["ok"], true, "{expected}");
    std::fs::remove_file(home.join(".pi/agent/pi-crash.log")).unwrap();
    let terminal = Rc::new(RefCell::new(CaptureTerminal::default()));
    if stop_error {
        terminal.borrow_mut().stop_error = Some(pi_js_compat::callback_error::CallbackError::from_rejection(
            pi_js_compat::abort::AbortReason::Value(pi_js_compat::js_json::JsValue::String(
                JsString::from("stop-") + JsString::from_utf16(vec![0xd800]),
            )),
        ));
    }
    let tui = TuiRenderer::new(terminal.clone());
    let lines = Rc::new(RefCell::new(vec![JsString::from("safe")]));
    tui.add_child(component(Lines(lines.clone())));
    tui.start().unwrap();
    tui.do_render().unwrap();
    *lines.borrow_mut() = vec!["overwidth-overwidth".into()];
    let error = tui.do_render().unwrap_err();
    let actual = match error.thrown_value() {
        pi_js_compat::abort::AbortReason::Value(pi_js_compat::js_json::JsValue::String(value)) => {
            json!({"rawUnits":value.units()})
        }
        pi_js_compat::abort::AbortReason::Error(error) => json!({"name":error.name,"message":error.message}),
        other => panic!("unexpected rejection {other:?}"),
    };
    assert_eq!(actual, expected["output"]["error"]);
    assert_eq!(json!(terminal.borrow().stop_events), expected["output"]["events"]);
    let log = std::fs::read_to_string(home.join(".pi/agent/pi-crash.log")).unwrap();
    assert_eq!(
        log.split_once('\n').unwrap().1,
        expected["output"]["log"].as_str().unwrap()
    );
}

#[derive(Clone)]
struct Bytes(Arc<Mutex<Vec<u8>>>);
impl Write for Bytes {
    fn write(&mut self, value: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(value);
        Ok(value.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn process_terminal_encodes_native_write_buffer_only_at_actual_io() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    for units in [
        vec![0xd800],
        vec![0xdc00],
        vec![0xd83d, 0xde00],
        vec![0xd800, 0x61, 0xdc00],
    ] {
        let expected = oracle.request(&json!({"operation":"terminal_bytes", "units":units}));
        assert_eq!(expected["ok"], true, "{expected}");
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let mut terminal = ProcessTerminal::with_output(Box::new(Bytes(bytes.clone())));
        Terminal::write(&mut terminal, &JsString::from_utf16(units));
        let hex: String = bytes.lock().unwrap().iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(serde_json::to_string(&hex).unwrap(), expected["json"].as_str().unwrap());
    }
}

#[test]
fn raw_terminal_title_reaches_osc_output_without_sanitation() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let units = vec![0xd800, 0x1b, 0x07, 0x61, 0xdc00, 0xd83d, 0xde00];
    let expected = oracle.request(&json!({"operation":"terminal_title", "units":units}));
    assert_eq!(expected["ok"], true, "{expected}");
    let title = JsString::from_utf16(units);
    let mut mock = CaptureTerminal::default();
    mock.set_title(&title);
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let mut terminal = ProcessTerminal::with_output(Box::new(Bytes(bytes.clone())));
    Terminal::set_title(&mut terminal, &title);
    let hex: String = bytes.lock().unwrap().iter().map(|byte| format!("{byte:02x}")).collect();
    let actual = serde_json::to_string(&(mock.writes, hex)).unwrap();
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(expected["json"].as_str().unwrap()).unwrap()
    );
}

#[test]
fn native_input_listener_rewrites_and_terminal_events_reach_focused_input_losslessly() {
    let mut oracle = oracle::TsOracle::spawn("raw-tui-oracle.ts", "packages/tui/src");
    let expected = oracle.request(&json!({"operation":"input_route"}));
    assert_eq!(expected["ok"], true, "{expected}");
    let terminal = Rc::new(RefCell::new(CaptureTerminal::default()));
    let tui = TuiRenderer::new(terminal.clone());
    let input = Rc::new(RefCell::new(pi_tui::components::input::Input::new(Default::default())));
    let submitted = Rc::new(RefCell::new(Vec::new()));
    let captured = submitted.clone();
    input.borrow_mut().on_submit = Some(Rc::new(move |value| captured.borrow_mut().push(value.clone())));
    tui.add_child(input.clone());
    tui.set_focus(Some(input.clone()));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let captured = seen.clone();
    tui.add_input_listener(Rc::new(move |value| {
        captured.borrow_mut().push(value.clone());
        let data = if value == "x" {
            Some(JsString::from_utf16(vec![0xd800]))
        } else if value == "y" {
            Some(JsString::from_utf16(vec![0xdc00]))
        } else {
            None
        };
        pi_tui::tui::TuiInputListenerResult { consume: false, data }
    }));
    tui.start().unwrap();
    tui.flush_pending().unwrap();
    terminal.borrow_mut().pending_input = vec![
        JsString::from("x"),
        JsString::from("y"),
        JsString::from_utf16(vec![0xd800]),
        JsString::from("z"),
        JsString::from_utf16(vec![0xdc00]),
        JsString::from("\r"),
    ];
    tui.flush_pending().unwrap();
    let actual = serde_json::to_string(&(
        input.borrow().get_value(),
        submitted.borrow().clone(),
        seen.borrow().clone(),
    ))
    .unwrap();
    tui.stop().unwrap();
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(expected["json"].as_str().unwrap()).unwrap()
    );
}
