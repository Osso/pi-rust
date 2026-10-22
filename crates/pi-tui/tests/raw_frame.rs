#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::{JsString, parse_lossless};
use pi_tui::terminal::{ProcessTerminal, Terminal, TerminalRawModeError};
use pi_tui::tui::{Component, OverlayOptions, SizeValue, TuiExt, TuiRenderer, component};
use serde_json::json;
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct CaptureTerminal {
    writes: Vec<JsString>,
}
impl Terminal for CaptureTerminal {
    fn start(&mut self) -> Result<(), TerminalRawModeError> {
        self.write(&"\x1b[?2004h".into());
        Ok(())
    }
    fn stop(&mut self) {
        self.write(&"\x1b[?2004l".into());
    }
    fn drain_input(&mut self, _: u64, _: u64) {}
    fn write(&mut self, value: &JsString) {
        self.writes.push(value.clone());
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
    tui.flush_pending();
    frames.push(std::mem::take(&mut terminal.borrow_mut().writes));
    lines.borrow_mut()[0] = JsString::from("y") + &raw;
    tui.request_render(false);
    tui.flush_pending();
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
    tui.flush_pending();
    frames.push(std::mem::take(&mut terminal.borrow_mut().writes));
    handle.hide();
    tui.flush_pending();
    frames.push(std::mem::take(&mut terminal.borrow_mut().writes));
    let cell = TuiRenderer::create_fixed_cell(&tui);
    cell.place(0, 1);
    let updated = cell.update(JsString::from_utf16(vec![0xd800, 0x7a, 0xdc00]));
    frames.push(std::mem::take(&mut terminal.borrow_mut().writes));
    tui.stop();
    frames.push(std::mem::take(&mut terminal.borrow_mut().writes));
    let stop = JsString::join(frames.pop().unwrap(), "");
    let actual = serde_json::to_string(&(frames, stop, updated)).unwrap();
    assert_eq!(
        parse_lossless(actual).unwrap(),
        parse_lossless(response["json"].as_str().unwrap()).unwrap()
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
