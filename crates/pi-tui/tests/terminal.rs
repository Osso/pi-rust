use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pi_tui::terminal::{
    ProcessTerminal, Terminal, TerminalEvent, TerminalRawModeError, is_apple_terminal_session,
    is_terminal_raw_mode_failure, normalize_apple_terminal_input, normalize_native_shift_enter_input,
    parse_keyboard_protocol_negotiation_sequence, resolve_columns, resolve_escape_timeout_ms, resolve_rows,
};

fn env_lookup(values: BTreeMap<&'static str, &'static str>) -> impl Fn(&str) -> Option<String> {
    move |name| values.get(name).map(|value| (*value).to_string())
}

#[derive(Clone, Default)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl SharedWriter {
    fn contents(&self) -> String {
        String::from_utf8(self.0.lock().expect("writer lock").clone()).expect("writes are UTF-8")
    }
}

impl Write for SharedWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("writer lock").extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn negotiation_terminal() -> (ProcessTerminal, SharedWriter) {
    let writer = SharedWriter::default();
    let mut terminal = ProcessTerminal::with_output(Box::new(writer.clone()));
    terminal.begin_keyboard_protocol_negotiation();
    (terminal, writer)
}

#[test]
fn uses_configured_escape_timeout() {
    assert_eq!(
        resolve_escape_timeout_ms(&env_lookup(BTreeMap::from([("PI_TUI_ESC_TIMEOUT", "80")]))),
        80.0
    );
    assert_eq!(
        resolve_escape_timeout_ms(&env_lookup(BTreeMap::from([
            ("PI_TUI_ESC_TIMEOUT", "80"),
            ("SSH_TTY", "/dev/pts/1"),
        ]))),
        80.0
    );
}

#[test]
fn ignores_invalid_escape_timeout_values() {
    for value in ["abc", "0", "-5", ""] {
        assert_eq!(
            resolve_escape_timeout_ms(&env_lookup(BTreeMap::from([("PI_TUI_ESC_TIMEOUT", value)]))),
            10.0,
            "value {value:?}"
        );
    }
}

#[test]
fn defaults_to_100ms_over_ssh() {
    assert_eq!(
        resolve_escape_timeout_ms(&env_lookup(BTreeMap::from([("SSH_CONNECTION", "10.0.0.1 22")]))),
        100.0
    );
    assert_eq!(
        resolve_escape_timeout_ms(&env_lookup(BTreeMap::from([("SSH_TTY", "/dev/pts/1")]))),
        100.0
    );
}

#[test]
fn defaults_to_10ms_otherwise() {
    assert_eq!(resolve_escape_timeout_ms(&env_lookup(BTreeMap::new())), 10.0);
}

#[test]
fn rewrites_return_to_csi_u_shift_enter_when_native_shift_detection_applies() {
    assert_eq!(normalize_native_shift_enter_input("\r", true, true), "\x1b[13;2u");
}

#[test]
fn leaves_return_unchanged_when_native_shift_detection_is_disabled() {
    assert_eq!(normalize_native_shift_enter_input("\r", false, true), "\r");
}

#[test]
fn leaves_return_unchanged_when_shift_is_not_pressed() {
    assert_eq!(normalize_native_shift_enter_input("\r", true, false), "\r");
}

#[test]
fn leaves_non_return_input_unchanged() {
    assert_eq!(
        normalize_native_shift_enter_input("\x1b[13;2u", true, true),
        "\x1b[13;2u"
    );
    assert_eq!(normalize_native_shift_enter_input("a", true, true), "a");
}

#[test]
fn rewrites_apple_terminal_return_to_csi_u_shift_enter() {
    assert_eq!(normalize_apple_terminal_input("\r", true, true), "\x1b[13;2u");
    assert_eq!(normalize_apple_terminal_input("\r", true, false), "\r");
    assert_eq!(normalize_apple_terminal_input("\r", false, true), "\r");
    assert_eq!(normalize_apple_terminal_input("\x1b[13;2u", true, true), "\x1b[13;2u");
    assert_eq!(normalize_apple_terminal_input("a", true, true), "a");
}

#[test]
fn detects_apple_terminal_only_on_macos_with_the_apple_program() {
    assert!(is_apple_terminal_session("macos", Some("Apple_Terminal")));
    assert!(!is_apple_terminal_session("linux", Some("Apple_Terminal")));
    assert!(!is_apple_terminal_session("macos", Some("iTerm.app")));
    assert!(!is_apple_terminal_session("macos", None));
}

#[test]
fn converts_raw_mode_failures_into_typed_errors() {
    let error = TerminalRawModeError::new(true, "setRawMode failed with errno: 5");

    assert_eq!(
        error.to_string(),
        "Failed to enable terminal raw mode: setRawMode failed with errno: 5"
    );
    assert!(is_terminal_raw_mode_failure(&error));
    assert_eq!(
        TerminalRawModeError::new(false, "boom").to_string(),
        "Failed to restore terminal raw mode: boom"
    );
}

#[test]
fn recognizes_bun_style_raw_mode_failure_messages() {
    let error = std::io::Error::other("setRawMode failed with errno: 5");
    assert!(is_terminal_raw_mode_failure(&error));
    assert!(!is_terminal_raw_mode_failure(&std::io::Error::other("other failure")));
}

#[test]
fn parses_keyboard_protocol_negotiation_sequences() {
    assert_eq!(
        parse_keyboard_protocol_negotiation_sequence("\x1b[?7u"),
        Some(pi_tui::terminal::KeyboardProtocolNegotiationSequence::KittyFlags { flags: 7 })
    );
    assert_eq!(
        parse_keyboard_protocol_negotiation_sequence("\x1b[?62;4;52c"),
        Some(pi_tui::terminal::KeyboardProtocolNegotiationSequence::DeviceAttributes)
    );
    assert_eq!(parse_keyboard_protocol_negotiation_sequence("\x1b[?u"), None);
    assert_eq!(parse_keyboard_protocol_negotiation_sequence("\x1b[7u"), None);
}

#[test]
fn queries_kitty_mode_before_enabling_modify_other_keys_fallback() {
    let (terminal, writer) = negotiation_terminal();
    let writes = writer.contents();

    assert!(writes.starts_with("\x1b[>7u\x1b[?u\x1b[c"));
    assert!(!writes.contains("\x1b[>4;2m"));
    assert!(!terminal.kitty_protocol_active());
}

#[test]
fn activates_kitty_mode_for_non_zero_negotiated_flags() {
    let (mut terminal, writer) = negotiation_terminal();
    terminal.feed_stdin("\x1b[?7u");
    let events = terminal.poll_at(Instant::now());

    assert!(events.is_empty());
    assert!(terminal.kitty_protocol_active());
    assert!(!writer.contents().contains("\x1b[>4;2m"));
    assert!(!writer.contents().contains("\x1b[>4;0m"));

    terminal.stop();
    assert_eq!(writer.contents().matches("\x1b[<u").count(), 1);
    assert!(!writer.contents().contains("\x1b[>4;0m"));
}

#[test]
fn falls_back_to_modify_other_keys_for_zero_kitty_flags() {
    let (mut terminal, writer) = negotiation_terminal();
    terminal.feed_stdin("\x1b[?0u");
    let events = terminal.poll_at(Instant::now());

    assert!(events.is_empty());
    assert!(!terminal.kitty_protocol_active());
    assert_eq!(writer.contents().matches("\x1b[>4;2m").count(), 1);

    terminal.stop();
    assert_eq!(writer.contents().matches("\x1b[>4;0m").count(), 1);
}

#[test]
fn falls_back_to_modify_other_keys_for_device_attributes_without_kitty_flags() {
    let (mut terminal, writer) = negotiation_terminal();
    terminal.feed_stdin("\x1b[?62;4;52c");
    let events = terminal.poll_at(Instant::now());

    assert!(events.is_empty());
    assert!(!terminal.kitty_protocol_active());
    assert_eq!(writer.contents().matches("\x1b[>4;2m").count(), 1);
}

#[test]
fn forwards_normal_input_while_waiting_for_kitty_response() {
    let (mut terminal, _writer) = negotiation_terminal();
    terminal.feed_stdin("a");

    assert_eq!(
        terminal.poll_at(Instant::now()),
        vec![TerminalEvent::Input("a".to_string())]
    );
    assert!(!terminal.kitty_protocol_active());
}

#[test]
fn tracks_split_kitty_confirmation() {
    let start = Instant::now();
    let (mut terminal, _writer) = negotiation_terminal();
    terminal.feed_stdin_at("\x1b[?7", start);
    assert!(terminal.poll_at(start + Duration::from_millis(10)).is_empty());

    terminal.feed_stdin_at("u", start + Duration::from_millis(10));
    assert!(terminal.kitty_protocol_active());
}

#[test]
fn replays_buffered_csi_prefix_input_when_it_is_not_a_kitty_response() {
    let start = Instant::now();
    let (mut terminal, _writer) = negotiation_terminal();
    terminal.feed_stdin_at("\x1b[", start);
    assert!(terminal.poll_at(start + Duration::from_millis(60)).is_empty());

    assert_eq!(
        terminal.poll_at(start + Duration::from_millis(400)),
        vec![TerminalEvent::Input("\x1b[".to_string())]
    );
}

#[test]
fn writes_a_valid_osc_9_4_clear_sequence() {
    let writer = SharedWriter::default();
    let mut terminal = ProcessTerminal::with_output(Box::new(writer.clone()));

    terminal.set_progress(false);

    assert_eq!(writer.contents(), "\x1b]9;4;0\x07");
}

#[test]
fn falls_back_to_columns_and_lines_before_default_dimensions() {
    assert_eq!(resolve_columns(None, Some("123".to_string())), 123);
    assert_eq!(resolve_rows(None, Some("45".to_string())), 45);
    assert_eq!(resolve_columns(Some(0), Some("123".to_string())), 123);
    assert_eq!(resolve_columns(None, None), 80);
    assert_eq!(resolve_rows(None, None), 24);
    assert_eq!(resolve_columns(Some(100), Some("123".to_string())), 100);
    assert_eq!(resolve_rows(Some(50), Some("45".to_string())), 50);
}

#[test]
fn environment_numbers_use_javascript_number_parsing() {
    // `Number("0x10")` is 16 and `Number("2e1")` is 20.
    assert_eq!(
        resolve_escape_timeout_ms(&env_lookup(BTreeMap::from([("PI_TUI_ESC_TIMEOUT", "0x10")]))),
        16.0
    );
    assert_eq!(
        resolve_escape_timeout_ms(&env_lookup(BTreeMap::from([("PI_TUI_ESC_TIMEOUT", "2e1")]))),
        20.0
    );
    assert_eq!(resolve_columns(None, Some("0x10".to_owned())), 16);
    assert_eq!(resolve_rows(None, Some("0b1010".to_owned())), 10);
    assert_eq!(resolve_columns(None, Some("1e2".to_owned())), 100);
}
