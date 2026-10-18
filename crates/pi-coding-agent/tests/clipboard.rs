#[path = "support/clipboard_harness.rs"]
mod harness;

use pi_coding_agent::utils::clipboard::{copy_to_clipboard, is_remote_session, read_clipboard_text};
use pi_coding_agent::utils::clipboard_native::{ClipboardError, ClipboardModule};

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
async fn returns_native_clipboard_text() {
    let environment = harness::environment(&[("DISPLAY", ":0")]);
    let clipboard = StaticClipboard::with_text("clipboard text");
    assert_eq!(
        read_clipboard_text(&environment, Some(&clipboard)).await.as_deref(),
        Some("clipboard text")
    );
}

#[tokio::test]
async fn returns_none_for_empty_or_unavailable_text() {
    let environment = harness::environment(&[]);
    assert_eq!(
        read_clipboard_text(&environment, Some(&StaticClipboard::with_text(""))).await,
        None
    );
    assert_eq!(
        read_clipboard_text(&environment, Some(&StaticClipboard::failing())).await,
        None
    );
    assert_eq!(read_clipboard_text(&environment, None).await, None);
}

#[tokio::test]
async fn wayland_text_prefers_wl_paste_over_the_stale_native_clipboard() {
    let scenario_name = "wayland_text_prefers_wl_paste_over_the_stale_native_clipboard";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [(
            "wl-paste",
            "printf '%s\\n' \"$*\" >> \"$PI_CLIPBOARD_FAKE_STATE/wl-paste.log\"\nprintf 'Wayland text'\n",
        )];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(
            scenario_name,
            &scenario,
            "linux",
            &[("WAYLAND_DISPLAY", "wayland-0"), ("XDG_SESSION_TYPE", "wayland")],
        );
        assert_child_passed(&output);
        let log = std::fs::read_to_string(scenario.state.join("wl-paste.log")).unwrap();
        assert_eq!(log, "--no-newline --type text\n");
        return;
    };

    let clipboard = StaticClipboard::with_text("stale X11 text");
    assert_eq!(
        read_clipboard_text(&child.environment, Some(&clipboard))
            .await
            .as_deref(),
        Some("Wayland text")
    );
}

#[tokio::test]
async fn empty_wayland_text_does_not_fall_back_to_the_native_clipboard() {
    let scenario_name = "empty_wayland_text_does_not_fall_back_to_the_native_clipboard";
    let Some(child) = child_environment(scenario_name) else {
        let scripts = [("wl-paste", "exit 0\n")];
        let scenario = Scenario::new(&scripts);
        let output = run_scenario(
            scenario_name,
            &scenario,
            "linux",
            &[("WAYLAND_DISPLAY", "wayland-0"), ("XDG_SESSION_TYPE", "wayland")],
        );
        assert_child_passed(&output);
        return;
    };

    let clipboard = StaticClipboard::with_text("stale X11 text");
    assert_eq!(read_clipboard_text(&child.environment, Some(&clipboard)).await, None);
}

#[tokio::test]
async fn missing_wl_paste_falls_back_to_the_native_clipboard() {
    let scenario_name = "missing_wl_paste_falls_back_to_the_native_clipboard";
    let Some(child) = child_environment(scenario_name) else {
        let scenario = Scenario::new(&[]);
        let output = run_scenario(
            scenario_name,
            &scenario,
            "linux",
            &[("WAYLAND_DISPLAY", "wayland-0"), ("XDG_SESSION_TYPE", "wayland")],
        );
        assert_child_passed(&output);
        return;
    };

    let clipboard = StaticClipboard::with_text("X11 fallback text");
    assert_eq!(
        read_clipboard_text(&child.environment, Some(&clipboard))
            .await
            .as_deref(),
        Some("X11 fallback text")
    );
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
    copy_to_clipboard("hello", &child.environment, Some(&clipboard))
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
    copy_to_clipboard("hello", &child.environment, Some(&clipboard))
        .await
        .expect("copy");
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

    copy_to_clipboard("hello", &child.environment, Some(&StaticClipboard::failing()))
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

    copy_to_clipboard("hello", &child.environment, Some(&StaticClipboard::failing()))
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
        &"x".repeat(80_000),
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

    copy_to_clipboard("hello", &child.environment, Some(&StaticClipboard::failing()))
        .await
        .expect("copy");
}

#[tokio::test]
async fn failed_wl_copy_falls_back_to_xclip_when_x11_is_available() {
    let scenario_name = "failed_wl_copy_falls_back_to_xclip_when_x11_is_available";
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
        let log = std::fs::read_to_string(scenario.state.join("xclip.log")).unwrap();
        assert_eq!(log, "-selection clipboard\n");
        return;
    };

    copy_to_clipboard("hello", &child.environment, Some(&StaticClipboard::failing()))
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

    copy_to_clipboard("hello", &child.environment, Some(&StaticClipboard::failing()))
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

    copy_to_clipboard("hello", &child.environment, Some(&StaticClipboard::failing()))
        .await
        .expect("copy");
}
