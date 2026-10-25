#[path = "support/env_probe.rs"]
mod env_probe;
#[path = "support/oracle.rs"]
mod oracle;

use std::ffi::OsString;

use pi_coding_agent::utils::clipboard_native::clipboard;
use serde_json::json;

#[test]
fn clipboard_module_guard_matches_frozen_initialization() {
    for (termux, display, wayland) in [
        ("", ":1", ""),
        ("", "", "wayland-test"),
        ("", "", ""),
        ("0.118", ":1", ""),
        ("0", ":1", ""),
    ] {
        let environment = [
            ("TERMUX_VERSION", OsString::from(termux)),
            ("DISPLAY", OsString::from(display)),
            ("WAYLAND_DISPLAY", OsString::from(wayland)),
        ];
        let mut source = oracle::TsOracle::spawn_with_environment(
            "clipboard-native-oracle.ts",
            "packages/coding-agent/src/utils/clipboard-native.ts",
            &environment,
        );
        let response = source.request(&json!({
            "id": 1, "nextTermux": "changed", "nextDisplay": "",
        }));
        assert_eq!(response["ok"], true, "{response}");
        let output = env_probe::run_probe("clipboard_guard_child", &environment);
        let stdout = String::from_utf8(output.stdout).unwrap();
        let actual = stdout
            .lines()
            .find_map(|line| line.split_once("CLIPBOARD_GUARD:").map(|(_, value)| value))
            .unwrap();
        let actual: serde_json::Value = serde_json::from_str(actual).unwrap();
        assert_eq!(
            actual, response["output"],
            "Termux={termux:?}, DISPLAY={display:?}, Wayland={wayland:?}"
        );
    }
}

#[test]
#[ignore = "child process probe"]
fn clipboard_guard_child() {
    if !env_probe::in_probe() {
        return;
    }
    let before = clipboard().is_some();
    pi_js_compat::process_env::set("TERMUX_VERSION", "changed");
    pi_js_compat::process_env::set("DISPLAY", "");
    pi_js_compat::process_env::set("WAYLAND_DISPLAY", "");
    println!("CLIPBOARD_GUARD:{}", json!([before, clipboard().is_some()]));
}
