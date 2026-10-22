#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/virtual_terminal.rs"]
mod virtual_terminal;

use pi_js_compat::js_json::JsString;
use pi_js_compat::process_env;
use pi_tui::tui::{TuiExt, component};
use pi_tui::{Component, TUI};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

struct Lines(Rc<RefCell<Vec<JsString>>>);
impl Component for Lines {
    fn render(&mut self, _width: usize) -> Vec<JsString> {
        self.0.borrow().clone()
    }
}

#[test]
fn debug_files_match_frozen_paths_and_payloads() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    assert_ne!(home, std::path::PathBuf::from("/home/osso"), "sandbox HOME required");
    let mut oracle = oracle::TsOracle::spawn("tui-core-oracle.ts", "packages/tui/src");
    let expected = oracle.request(&json!({"id":"debug", "operation":"tui_debug"}));
    assert_eq!(expected["ok"], true, "{expected}");
    let log = home.join(".pi/agent/pi-debug.log");
    std::fs::remove_file(&log).unwrap();
    std::fs::remove_dir_all("/tmp/tui").unwrap();
    process_env::set("PI_CODING_AGENT_DIR", &home.join("wrong-agent-dir").to_string_lossy());
    let lines = Rc::new(RefCell::new(vec![JsString::from("first")]));
    let (_, terminal) = virtual_terminal::concrete(20, 5);
    let tui = TUI::new(terminal);
    process_env::set("PI_DEBUG_REDRAW", "1");
    process_env::set("PI_TUI_DEBUG", "1");
    tui.add_child(component(Lines(Rc::clone(&lines))));
    tui.start().unwrap();
    tui.flush_pending();
    *lines.borrow_mut() = vec![JsString::from("changed")];
    tui.request_render(false);
    tui.flush_pending();
    tui.stop();
    process_env::remove("PI_CODING_AGENT_DIR");
    process_env::remove("PI_DEBUG_REDRAW");
    process_env::remove("PI_TUI_DEBUG");
    let debug = std::fs::read_to_string(log).expect("debug file must use frozen home path");
    let normalized = regex::Regex::new(r"\[[^\]]+\]").unwrap().replace_all(&debug, "[TIME]");
    assert_eq!(normalized, expected["output"]["debug"].as_str().unwrap());
    let mut dumps = std::fs::read_dir("/tmp/tui")
        .expect("frozen debug dump directory")
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    dumps.sort();
    assert_eq!(json!(dumps), expected["output"]["dumps"]);
}
