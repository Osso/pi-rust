//! Behavioral tests for `checkTmuxKeyboardSetup` in
//! `modes/interactive/interactive-mode.ts`.
//!
//! The host tmux is never consulted: the environment and the tmux program are
//! injected, and the program is a recording script.

#[path = "support/executable_script.rs"]
mod executable_script;

use pi_js_compat::js_json::JsString;
use std::ffi::OsStr;
use std::path::Path;
use std::time::Instant;

use pi_coding_agent::modes::interactive::interactive_mode::check_tmux_keyboard_setup_with;

const EXTENDED_KEYS_OFF_WARNING: &str = "tmux extended-keys is off. Modified Enter keys may not work. Add `set -g extended-keys on` to ~/.tmux.conf and restart tmux.";
const EXTENDED_KEYS_FORMAT_WARNING: &str = "tmux extended-keys-format is xterm. Pi works best with csi-u. Add `set -g extended-keys-format csi-u` to ~/.tmux.conf and restart tmux.";

fn script(path: &Path, contents: &str) {
    executable_script::write_executable_script(path, contents);
}

fn tmux_script(path: &Path, extended_keys: &str, extended_keys_format: &str) {
    script(
        path,
        &format!(
            "#!/bin/sh\ncase \"$3\" in\n  extended-keys) printf '%s\\n' '{extended_keys}' ;;\n  extended-keys-format) printf '%s\\n' '{extended_keys_format}' ;;\nesac\n"
        ),
    );
}

fn tmux_env() -> Option<&'static OsStr> {
    Some(OsStr::new("/tmp/tmux-1000/default,1234,0"))
}

#[tokio::test]
async fn skips_the_check_outside_tmux() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("tmux");
    tmux_script(&path, "off", "xterm");

    assert_eq!(
        check_tmux_keyboard_setup_with(None, &JsString::from(&path.display().to_string())).await,
        None
    );
}

#[tokio::test]
async fn accepts_enabled_extended_keys_and_csi_u() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("tmux");
    tmux_script(&path, "on", "csi-u");

    assert_eq!(
        check_tmux_keyboard_setup_with(tmux_env(), &JsString::from(&path.display().to_string())).await,
        None
    );
}

#[tokio::test]
async fn warns_when_extended_keys_are_off() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("tmux");
    tmux_script(&path, "off", "csi-u");

    assert_eq!(
        check_tmux_keyboard_setup_with(tmux_env(), &JsString::from(&path.display().to_string())).await,
        Some(JsString::from(EXTENDED_KEYS_OFF_WARNING.to_owned()))
    );
}

#[tokio::test]
async fn warns_when_the_format_is_xterm() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("tmux");
    tmux_script(&path, "always", "xterm");

    assert_eq!(
        check_tmux_keyboard_setup_with(tmux_env(), &JsString::from(&path.display().to_string())).await,
        Some(JsString::from(EXTENDED_KEYS_FORMAT_WARNING.to_owned()))
    );
}

#[tokio::test]
async fn trims_whitespace_from_tmux_output() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("tmux");
    tmux_script(&path, "  on  ", " csi-u ");

    assert_eq!(
        check_tmux_keyboard_setup_with(tmux_env(), &JsString::from(&path.display().to_string())).await,
        None
    );
}

#[tokio::test]
async fn ignores_unqueryable_tmux() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("tmux");
    script(&path, "#!/bin/sh\nexit 1\n");

    assert_eq!(
        check_tmux_keyboard_setup_with(tmux_env(), &JsString::from(&path.display().to_string())).await,
        None
    );
}

#[tokio::test]
async fn ignores_a_missing_format_value() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("tmux");
    script(
        &path,
        "#!/bin/sh\ncase \"$3\" in\n  extended-keys) printf '%s\\n' 'on' ;;\n  extended-keys-format) exit 1 ;;\nesac\n",
    );

    assert_eq!(
        check_tmux_keyboard_setup_with(tmux_env(), &JsString::from(&path.display().to_string())).await,
        None
    );
}

#[tokio::test]
async fn times_out_after_two_seconds_without_warning() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("tmux");
    script(&path, "#!/bin/sh\nsleep 5\n");

    let started = Instant::now();
    let warning = check_tmux_keyboard_setup_with(tmux_env(), &JsString::from(&path.display().to_string())).await;
    assert_eq!(warning, None);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(4),
        "both tmux queries must time out after two seconds"
    );
}
