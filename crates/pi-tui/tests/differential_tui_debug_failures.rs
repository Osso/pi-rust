#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/virtual_terminal.rs"]
mod virtual_terminal;

use pi_js_compat::process_env;
use pi_tui::tui::{TuiExt, component};
use pi_tui::{TUI, Text};
use serde_json::json;

#[test]
fn redraw_log_failure_is_not_silently_suppressed() {
    let home = tempfile::tempdir().unwrap();
    let mut oracle = oracle::TsOracle::spawn("tui-core-oracle.ts", "packages/tui/src");
    let expected =
        oracle.request(&json!({"id":"failure", "operation":"tui_debug_failure", "home":home.path().to_str().unwrap()}));
    assert_eq!(expected["ok"], true, "{expected}");
    process_env::set("HOME", home.path().to_str().unwrap());
    process_env::set("PI_DEBUG_REDRAW", "1");
    let (_, terminal) = virtual_terminal::concrete(20, 5);
    let tui = TUI::new(terminal);
    tui.add_child(component(Text::new("first", 0, 0)));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tui.render_now(false)));
    process_env::remove("HOME");
    process_env::remove("PI_DEBUG_REDRAW");
    assert_eq!(json!({"failed": result.is_err()}), expected["output"]);
}
