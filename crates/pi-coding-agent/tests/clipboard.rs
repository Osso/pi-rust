#[path = "support/clipboard_harness.rs"]
mod harness;

use pi_coding_agent::utils::clipboard::{copy_to_clipboard, is_remote_session};
use pi_coding_agent::utils::clipboard_native::{ClipboardError, ClipboardModule};
use pi_js_compat::js_json::JsString;

use harness::{Scenario, assert_child_passed, child_environment, osc52_sequences, run_scenario};

struct StaticClipboard {
    text: Result<String, ClipboardError>,
    set_result: Result<(), ClipboardError>,
}

impl StaticClipboard {
    fn with_text(text: &str) -> Self {
        Self {
            text: Ok(text.to_owned()),
            set_result: Ok(()),
        }
    }

    fn failing() -> Self {
        Self {
            text: Err(ClipboardError::new("clipboard unavailable")),
            set_result: Err(ClipboardError::new("native failed")),
        }
    }
}

impl ClipboardModule for StaticClipboard {
    fn get_text(&self) -> Result<String, ClipboardError> {
        self.text.clone()
    }

    fn set_text(&self, _text: &str) -> Result<(), ClipboardError> {
        self.set_result.clone()
    }

    fn has_image(&self) -> Result<bool, ClipboardError> {
        Ok(false)
    }

    fn get_image_binary(&self) -> Result<Vec<u8>, ClipboardError> {
        Err(ClipboardError::new("no image"))
    }
}

#[test]
fn detects_remote_sessions() {
    for name in ["SSH_CONNECTION", "SSH_CLIENT", "MOSH_CONNECTION"] {
        assert!(is_remote_session(&harness::environment(&[(name, "client server")])));
        assert!(!is_remote_session(&harness::environment(&[(name, "")])));
    }
    assert!(!is_remote_session(&harness::environment(&[])));
}

#[tokio::test]
async fn local_native_copy_skips_osc52_and_shell_tools() {
    let scenario_name = "local_native_copy_skips_osc52_and_shell_tools";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [(
            "pbcopy",
            "printf 'called\\n' >> \"$PI_CLIPBOARD_FAKE_STATE/pbcopy.log\"\nexit 0\n",
        )];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(scenario_name, &scenario, "darwin", &[]);
        assert_child_passed(&output);
        assert!(osc52_sequences(&output.stdout).is_empty());
        assert!(!scenario.state.join("pbcopy.log").exists());
        return;
    };

    let mut clipboard = StaticClipboard::with_text("");
    clipboard.set_result = Ok(());
    copy_to_clipboard(&JsString::from("hello"), &child.environment, Some(&clipboard))
        .await
        .expect("copy");
}

#[tokio::test]
async fn remote_native_copy_emits_osc52_after_the_native_write() {
    let scenario_name = "remote_native_copy_emits_osc52_after_the_native_write";
    let Some(child) = child_environment(scenario_name) else {
        let scenario = Scenario::new(&[]);
        let output = run_scenario(
            scenario_name,
            &scenario,
            "darwin",
            &[("SSH_CONNECTION", "client server")],
        );
        assert_child_passed(&output);
        let sequences = osc52_sequences(&output.stdout);
        assert_eq!(sequences, vec![harness::osc52_sequence("hello")]);
        return;
    };

    let clipboard = StaticClipboard::with_text("");
    copy_to_clipboard(&JsString::from("hello"), &child.environment, Some(&clipboard))
        .await
        .expect("copy");
}

/// Records the text the native addon receives.
struct RecordingClipboard {
    received: std::sync::Mutex<Vec<String>>,
}

impl ClipboardModule for RecordingClipboard {
    fn get_text(&self) -> Result<String, ClipboardError> {
        Ok(String::new())
    }

    fn set_text(&self, text: &str) -> Result<(), ClipboardError> {
        self.received.lock().unwrap().push(text.to_owned());
        Ok(())
    }

    fn has_image(&self) -> Result<bool, ClipboardError> {
        Ok(false)
    }

    fn get_image_binary(&self) -> Result<Vec<u8>, ClipboardError> {
        Err(ClipboardError::new("no image"))
    }
}

#[tokio::test]
async fn unpaired_surrogates_reach_native_and_osc52_as_replacement_characters() {
    let scenario_name = "unpaired_surrogates_reach_native_and_osc52_as_replacement_characters";
    let Some(child) = child_environment(scenario_name) else {
        let scenario = Scenario::new(&[]);
        let output = run_scenario(
            scenario_name,
            &scenario,
            "darwin",
            &[("SSH_CONNECTION", "client server")],
        );
        assert_child_passed(&output);
        assert_eq!(
            osc52_sequences(&output.stdout),
            vec![harness::osc52_sequence("a\u{FFFD}b")]
        );
        return;
    };

    let clipboard = RecordingClipboard {
        received: std::sync::Mutex::new(Vec::new()),
    };
    let text = JsString::from_utf16(vec![u16::from(b'a'), 0xD800, u16::from(b'b')]);
    copy_to_clipboard(&text, &child.environment, Some(&clipboard))
        .await
        .expect("copy");
    assert_eq!(*clipboard.received.lock().unwrap(), vec!["a\u{FFFD}b".to_owned()]);
}

#[tokio::test]
async fn local_shell_fallback_success_skips_osc52() {
    let scenario_name = "local_shell_fallback_success_skips_osc52";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [(
            "pbcopy",
            "printf '%s\\n' \"$*\" >> \"$PI_CLIPBOARD_FAKE_STATE/pbcopy.log\"\nwhile read -r line; do :; done\nexit 0\n",
        )];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(scenario_name, &scenario, "darwin", &[]);
        assert_child_passed(&output);
        assert!(osc52_sequences(&output.stdout).is_empty());
        assert!(scenario.state.join("pbcopy.log").exists());
        return;
    };

    copy_to_clipboard(
        &JsString::from("hello"),
        &child.environment,
        Some(&StaticClipboard::failing()),
    )
    .await
    .expect("copy");
}

#[tokio::test]
async fn osc52_fallback_is_used_when_native_and_shell_tools_fail() {
    let scenario_name = "osc52_fallback_is_used_when_native_and_shell_tools_fail";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [("pbcopy", "exit 1\n")];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(scenario_name, &scenario, "darwin", &[]);
        assert_child_passed(&output);
        assert_eq!(osc52_sequences(&output.stdout), vec![harness::osc52_sequence("hello")]);
        return;
    };

    copy_to_clipboard(
        &JsString::from("hello"),
        &child.environment,
        Some(&StaticClipboard::failing()),
    )
    .await
    .expect("copy");
}

#[tokio::test]
async fn oversized_osc52_payloads_fail_instead_of_writing() {
    let scenario_name = "oversized_osc52_payloads_fail_instead_of_writing";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [("pbcopy", "exit 1\n")];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(scenario_name, &scenario, "darwin", &[]);
        assert_child_passed(&output);
        assert!(osc52_sequences(&output.stdout).is_empty());
        return;
    };

    let error = copy_to_clipboard(
        &JsString::from("x".repeat(80_000)),
        &child.environment,
        Some(&StaticClipboard::failing()),
    )
    .await
    .expect_err("oversized payload must fail");
    assert_eq!(error.to_string(), "Failed to copy to clipboard");
}

#[tokio::test]
async fn linux_wayland_copy_uses_wl_copy() {
    let scenario_name = "linux_wayland_copy_uses_wl_copy";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [
            ("which", "exit 0\n"),
            (
                "wl-copy",
                "printf 'called\\n' >> \"$PI_CLIPBOARD_FAKE_STATE/wl-copy.log\"\nwhile read -r line; do :; done\nexit 0\n",
            ),
        ];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(
            scenario_name,
            &scenario,
            "linux",
            &[("WAYLAND_DISPLAY", "wayland-0"), ("XDG_SESSION_TYPE", "wayland")],
        );
        assert_child_passed(&output);
        assert!(scenario.state.join("wl-copy.log").exists());
        assert!(osc52_sequences(&output.stdout).is_empty());
        return;
    };

    copy_to_clipboard(
        &JsString::from("hello"),
        &child.environment,
        Some(&StaticClipboard::failing()),
    )
    .await
    .expect("copy");
    // Keep the fixture runtime alive while the unref stdin task drains.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while child.invocations("wl-copy").is_empty() {
        assert!(std::time::Instant::now() < deadline, "private wl-copy must start");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn nonzero_wl_copy_does_not_fall_back_to_xclip_after_spawn() {
    let scenario_name = "nonzero_wl_copy_does_not_fall_back_to_xclip_after_spawn";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [
            ("which", "exit 0\n"),
            ("wl-copy", "exit 1\n"),
            (
                "xclip",
                "printf '%s\\n' \"$*\" >> \"$PI_CLIPBOARD_FAKE_STATE/xclip.log\"\nwhile read -r line; do :; done\nexit 0\n",
            ),
        ];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(
            scenario_name,
            &scenario,
            "linux",
            &[
                ("WAYLAND_DISPLAY", "wayland-0"),
                ("XDG_SESSION_TYPE", "wayland"),
                ("DISPLAY", ":0"),
            ],
        );
        assert_child_passed(&output);
        // Frozen clipboard.ts:92-103 marks copied immediately, regardless of exit.
        assert!(!scenario.state.join("xclip.log").exists());
        return;
    };

    copy_to_clipboard(
        &JsString::from("hello"),
        &child.environment,
        Some(&StaticClipboard::failing()),
    )
    .await
    .expect("copy");
}

#[tokio::test]
async fn linux_x11_copy_falls_back_from_xclip_to_xsel() {
    let scenario_name = "linux_x11_copy_falls_back_from_xclip_to_xsel";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [
            ("xclip", "exit 1\n"),
            (
                "xsel",
                "printf '%s\\n' \"$*\" >> \"$PI_CLIPBOARD_FAKE_STATE/xsel.log\"\nwhile read -r line; do :; done\nexit 0\n",
            ),
        ];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(scenario_name, &scenario, "linux", &[("DISPLAY", ":0")]);
        assert_child_passed(&output);
        let log = std::fs::read_to_string(scenario.state.join("xsel.log")).unwrap();
        assert_eq!(log, "--clipboard --input\n");
        return;
    };

    copy_to_clipboard(
        &JsString::from("hello"),
        &child.environment,
        Some(&StaticClipboard::failing()),
    )
    .await
    .expect("copy");
}

#[tokio::test]
async fn linux_termux_copy_uses_termux_clipboard_set() {
    let scenario_name = "linux_termux_copy_uses_termux_clipboard_set";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [(
            "termux-clipboard-set",
            "printf 'called\\n' >> \"$PI_CLIPBOARD_FAKE_STATE/termux.log\"\nwhile read -r line; do :; done\nexit 0\n",
        )];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(scenario_name, &scenario, "linux", &[("TERMUX_VERSION", "0.118")]);
        assert_child_passed(&output);
        assert!(scenario.state.join("termux.log").exists());
        return;
    };

    copy_to_clipboard(
        &JsString::from("hello"),
        &child.environment,
        Some(&StaticClipboard::failing()),
    )
    .await
    .expect("copy");
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
